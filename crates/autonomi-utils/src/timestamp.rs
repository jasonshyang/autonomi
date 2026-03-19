use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Unix timestamp in whole seconds, derived from the system clock via `chrono`.
///
/// # Example
///
/// ```rust
/// use autonomi_utils::Timestamp;
///
/// let now = Timestamp::now();
/// println!("seconds since epoch: {}", now.as_secs());
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Capture the current UTC time as a [`Timestamp`].
    pub fn now() -> Self {
        let secs = Utc::now().timestamp().max(0) as u64;
        Self(secs)
    }

    /// Return the inner seconds value.
    pub fn as_secs(self) -> u64 {
        self.0
    }
}

impl From<u64> for Timestamp {
    fn from(secs: u64) -> Self {
        Self(secs)
    }
}

impl From<Timestamp> for u64 {
    fn from(ts: Timestamp) -> u64 {
        ts.0
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
