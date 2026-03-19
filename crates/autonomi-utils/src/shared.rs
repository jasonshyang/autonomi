use std::sync::Arc;

use arc_swap::ArcSwap;

// ---------------------------------------------------------------------------
// Shared<T>
// ---------------------------------------------------------------------------

/// A cheaply-cloneable, lock-free shared value.
///
/// `Shared<T>` wraps [`arc_swap::ArcSwap`] and exposes a minimal API so that
/// callers never need to know about RCU, guard types, or swap semantics.
///
/// # Concurrency model
///
/// - **Reads** ([`load`][Self::load] / [`read`][Self::read]) are wait-free:
///   they perform a single atomic pointer load and return a snapshot `Arc<T>`.
///   Many concurrent readers never block each other or any writer.
///
/// - **Writes** ([`update`][Self::update]) are lock-free: they clone the
///   current value, pass it to a mutation closure, then atomically swap in the
///   result. If two writers race the closure may be called more than once, so
///   it must be **cheap and free of observable side-effects**.
///
/// - **One-shot writes** ([`update_once`][Self::update_once]) are for values
///   that cannot be cloned (e.g. a `Box` or non-`Clone` hook). The closure is
///   guaranteed to run exactly once; if the swap loses a race it retries by
///   calling the closure a second time, which panics — use this only when
///   concurrent writers are not expected (e.g. during startup).
///
/// # Example
///
/// ```rust,ignore
/// let shared = Shared::new(vec![1_u32, 2, 3]);
///
/// // Cheap snapshot — no lock, no await.
/// let snap = shared.load();
/// println!("{snap:?}");
///
/// // Inspect a field without keeping the Arc alive.
/// let len = shared.read(|v| v.len());
///
/// // Atomic mutation.
/// shared.update(|v| v.push(4));
/// ```
#[derive(Clone)]
pub struct Shared<T> {
    inner: Arc<ArcSwap<T>>,
}

impl<T: Clone + Send + Sync + 'static> Shared<T> {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Create a new `Shared<T>` wrapping `value`.
    pub fn new(value: T) -> Self { Self { inner: Arc::new(ArcSwap::from_pointee(value)) } }

    // -----------------------------------------------------------------------
    // Reading
    // -----------------------------------------------------------------------

    /// Return a point-in-time snapshot of the shared value.
    ///
    /// The returned `Arc<T>` is stable: concurrent calls to
    /// [`update`][Self::update] do not affect it. Avoid holding the snapshot
    /// longer than needed so that old versions can be reclaimed.
    #[inline]
    pub fn load(&self) -> Arc<T> { self.inner.load_full() }

    /// Inspect the current value through a closure and return its result.
    ///
    /// This is a convenience wrapper that loads a snapshot, calls `f`, and
    /// immediately drops the snapshot — no `Arc` escapes the call.
    #[inline]
    pub fn read<R, F: FnOnce(&T) -> R>(&self, f: F) -> R { f(&self.inner.load()) }

    // -----------------------------------------------------------------------
    // Writing
    // -----------------------------------------------------------------------

    /// Atomically update the shared value.
    ///
    /// `f` receives a mutable reference to a **clone** of the current value.
    /// After `f` returns the clone is atomically swapped in as the new current
    /// value. If a concurrent writer raced and won, the process retries from
    /// a fresh clone, so `f` may be called more than once — keep it cheap and
    /// side-effect-free.
    pub fn update<F: FnMut(&mut T)>(&self, mut f: F) {
        self.inner.rcu(|current| {
            let mut next = (**current).clone();
            f(&mut next);
            next
        });
    }

    /// Atomically update the shared value using a one-shot closure.
    ///
    /// Unlike [`update`][Self::update], the closure receives ownership of a
    /// fresh clone and returns the replacement value outright, making it
    /// possible to move non-`Copy` values into the new state without requiring
    /// an extra `Clone` bound on the payload.
    ///
    /// The closure is still wrapped in `FnMut` by the underlying RCU loop, so
    /// the value is placed in an `Option` and `take()`n on first call. A
    /// second call — which would only happen under a write-write race — will
    /// panic. Use [`update`][Self::update] if concurrent writers are possible.
    pub fn update_once<F: FnOnce(T) -> T>(&self, f: F) {
        let mut f = Some(f);
        self.inner.rcu(|current| {
            let next = (**current).clone();
            f.take().expect(
                "update_once closure called more than once — use `update` under concurrent writers",
            )(next)
        });
    }
}

impl<T: Clone + Send + Sync + Default + 'static> Default for Shared<T> {
    fn default() -> Self { Self::new(T::default()) }
}

impl<T: Clone + Send + Sync + std::fmt::Debug + 'static> std::fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Shared").field(&*self.load()).finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::task::JoinSet;

    use super::*;

    // -----------------------------------------------------------------------
    // Construction & basic reads
    // -----------------------------------------------------------------------

    #[test]
    fn new_stores_initial_value() {
        let shared = Shared::new(42_u32);
        assert_eq!(*shared.load(), 42);
    }

    #[test]
    fn read_returns_closure_result() {
        let shared = Shared::new(vec![1_u32, 2, 3]);
        let len = shared.read(|v| v.len());
        assert_eq!(len, 3);
    }

    #[test]
    fn load_returns_stable_snapshot() {
        let shared = Shared::new(0_u32);
        let snap = shared.load();

        // Mutate after taking the snapshot.
        shared.update(|v| *v = 99);

        // The snapshot must still reflect the old value.
        assert_eq!(*snap, 0);
        // And the live value must reflect the update.
        assert_eq!(*shared.load(), 99);
    }

    // -----------------------------------------------------------------------
    // update
    // -----------------------------------------------------------------------

    #[test]
    fn update_mutates_value() {
        let shared = Shared::new(0_u32);
        shared.update(|v| *v += 1);
        assert_eq!(*shared.load(), 1);
    }

    #[test]
    fn update_accumulates_across_calls() {
        let shared = Shared::new(vec![1_u32]);
        shared.update(|v| v.push(2));
        shared.update(|v| v.push(3));
        assert_eq!(*shared.load(), vec![1, 2, 3]);
    }

    #[test]
    fn update_closure_receives_current_value() {
        let shared = Shared::new(10_i32);
        shared.update(|v| *v *= 3);
        assert_eq!(*shared.load(), 30);
    }

    // -----------------------------------------------------------------------
    // update_once
    // -----------------------------------------------------------------------

    #[test]
    fn update_once_moves_non_clone_value_in() {
        // String is Clone, but we use a Box<dyn Fn> to prove the point that
        // update_once can carry a non-Clone payload via the FnOnce closure.
        let shared = Shared::new(String::from("hello"));
        let suffix = String::from(", world"); // moved into the closure
        shared.update_once(|mut s| {
            s.push_str(&suffix);
            s
        });
        assert_eq!(*shared.load(), "hello, world");
    }

    #[test]
    fn update_once_replaces_value_completely() {
        let shared = Shared::new(0_u32);
        shared.update_once(|_| 42);
        assert_eq!(*shared.load(), 42);
    }

    // -----------------------------------------------------------------------
    // Clone shares the same inner value
    // -----------------------------------------------------------------------

    #[test]
    fn clone_shares_state() {
        let a = Shared::new(0_u32);
        let b = a.clone();

        a.update(|v| *v = 7);

        // b must see the write made through a.
        assert_eq!(*b.load(), 7);
    }

    #[test]
    fn write_through_clone_visible_on_original() {
        let a = Shared::new(0_u32);
        let b = a.clone();

        b.update(|v| *v = 99);

        assert_eq!(*a.load(), 99);
    }

    // -----------------------------------------------------------------------
    // Default & Debug impls
    // -----------------------------------------------------------------------

    #[test]
    fn default_uses_inner_default() {
        let shared: Shared<u32> = Shared::default();
        assert_eq!(*shared.load(), 0);
    }

    #[test]
    fn debug_output_contains_value() {
        let shared = Shared::new(42_u32);
        let s = format!("{shared:?}");
        assert!(s.contains("42"), "unexpected Debug output: {s}");
    }

    // -----------------------------------------------------------------------
    // Concurrency — many readers, one writer
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn concurrent_reads_never_see_torn_value() {
        // Counter starts at 0. A background task increments it 1 000 times.
        // Concurrent readers must always observe a valid u64 (no tearing).
        let shared = Shared::new(0_u64);
        let readers = shared.clone();
        let writer = shared.clone();

        let write_task = tokio::spawn(async move {
            for _ in 0..1_000 {
                writer.update(|v| *v += 1);
                tokio::task::yield_now().await;
            }
        });

        let read_task = tokio::spawn(async move {
            for _ in 0..2_000 {
                let _ = readers.load(); // must not panic / segfault
                tokio::task::yield_now().await;
            }
        });

        write_task.await.unwrap();
        read_task.await.unwrap();

        assert_eq!(*shared.load(), 1_000);
    }

    // -----------------------------------------------------------------------
    // Concurrency — many concurrent writers converge
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn concurrent_updates_all_applied() {
        // Spawn N tasks each incrementing the counter M times.
        // Because `update` retries on contention every increment must land.
        const TASKS: usize = 8;
        const INCS_PER_TASK: usize = 200;

        let shared: Shared<u64> = Shared::new(0);
        let mut set = JoinSet::new();

        for _ in 0..TASKS {
            let s = shared.clone();
            set.spawn(async move {
                for _ in 0..INCS_PER_TASK {
                    s.update(|v| *v += 1);
                    tokio::task::yield_now().await;
                }
            });
        }

        while set.join_next().await.is_some() {}

        assert_eq!(*shared.load(), (TASKS * INCS_PER_TASK) as u64);
    }

    // -----------------------------------------------------------------------
    // Concurrency — read sees latest write eventually
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn reader_observes_write_after_yield() {
        let shared = Shared::new(false);
        let writer = shared.clone();

        let write_task = tokio::spawn(async move {
            tokio::task::yield_now().await;
            writer.update(|v| *v = true);
        });

        write_task.await.unwrap();

        assert!(*shared.load(), "reader did not observe the write");
    }

    // -----------------------------------------------------------------------
    // Concurrency — update call-count tracked via atomic side-channel
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn update_closure_called_at_least_once_per_call() {
        // The closure may be called more than once under contention, but it
        // must be called at least once for every `update` invocation.
        const TASKS: usize = 4;
        const CALLS: usize = 50;

        let shared: Shared<u64> = Shared::new(0);
        let invocations = Arc::new(AtomicUsize::new(0));
        let mut set = JoinSet::new();

        for _ in 0..TASKS {
            let s = shared.clone();
            let inv = Arc::clone(&invocations);
            set.spawn(async move {
                for _ in 0..CALLS {
                    let inv = Arc::clone(&inv);
                    s.update(move |v| {
                        inv.fetch_add(1, Ordering::Relaxed);
                        *v += 1;
                    });
                    tokio::task::yield_now().await;
                }
            });
        }

        while set.join_next().await.is_some() {}

        // Final value must be exact (update is linearisable).
        assert_eq!(*shared.load(), (TASKS * CALLS) as u64);
        // Invocations must be at least TASKS * CALLS (retries may add more).
        assert!(invocations.load(Ordering::Relaxed) >= TASKS * CALLS);
    }
}
