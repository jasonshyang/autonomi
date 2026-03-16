//! # ReadonlyFilesystem
//!
//! Three read-only tools for inspecting the local filesystem:
//!
//! | Tool | Description |
//! |------|-------------|
//! | [`ReadFileTool`] | Read the UTF-8 contents of a file |
//! | [`ListDirTool`] | List the entries inside a directory |
//! | [`FileMetadataTool`] | Stat a path (size, kind, permissions, timestamps) |
//!
//! All tools are synchronous and stateless — they implement
//! [`SyncTool<Toolbox>`] and can be registered directly with
//! [`ToolboxBuilder::with_sync_tool`].

use std::borrow::Cow;

use rmcp::{schemars, ErrorData};
use serde::{Deserialize, Serialize};

use crate::toolbox::Toolbox;
use crate::{SyncTool, ToolBase};

// ── Shared helpers ─────────────────────────────────────────────────────────────

fn io_err(context: &str, path: &str, e: std::io::Error) -> ErrorData {
    ErrorData::invalid_params(format!("{context} '{path}': {e}"), None)
}

// ═══════════════════════════════════════════════════════════════════════════════
// ReadFileTool
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct ReadFileParams {
    /// Absolute or relative path to the file to read.
    pub path: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ReadFileOutput {
    /// UTF-8 contents of the file.
    pub content: String,
}

/// Read the full UTF-8 contents of a file.
pub struct ReadFileTool;

impl ToolBase for ReadFileTool {
    type Parameter = ReadFileParams;
    type Output = ReadFileOutput;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        "read_file".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Read the full UTF-8 contents of a file at the given path.".into())
    }
}

impl SyncTool<Toolbox> for ReadFileTool {
    fn invoke(_ctx: &Toolbox, p: ReadFileParams) -> Result<ReadFileOutput, ErrorData> {
        let content = std::fs::read_to_string(&p.path)
            .map_err(|e| io_err("failed to read file", &p.path, e))?;

        Ok(ReadFileOutput { content })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ListDirTool
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct ListDirParams {
    /// Path to the directory to list.
    pub path: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DirEntry {
    /// Entry name (not the full path).
    pub name: String,
    /// `"file"`, `"dir"`, or `"symlink"`.
    pub kind: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ListDirOutput {
    /// Sorted list of entries inside the directory.
    pub entries: Vec<DirEntry>,
}

/// List the entries inside a directory.
pub struct ListDirTool;

impl ToolBase for ListDirTool {
    type Parameter = ListDirParams;
    type Output = ListDirOutput;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        "list_dir".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("List all files and directories directly inside the given directory path.".into())
    }
}

impl SyncTool<Toolbox> for ListDirTool {
    fn invoke(_ctx: &Toolbox, p: ListDirParams) -> Result<ListDirOutput, ErrorData> {
        let read = std::fs::read_dir(&p.path)
            .map_err(|e| io_err("failed to read directory", &p.path, e))?;

        let mut entries: Vec<DirEntry> = read
            .filter_map(|res| {
                let entry = res.ok()?;
                let name = entry.file_name().to_string_lossy().to_string();
                let kind = entry
                    .file_type()
                    .map(|ft| {
                        if ft.is_dir() {
                            "dir"
                        } else if ft.is_symlink() {
                            "symlink"
                        } else {
                            "file"
                        }
                    })
                    .unwrap_or("unknown")
                    .to_string();
                Some(DirEntry { name, kind })
            })
            .collect();

        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(ListDirOutput { entries })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// FileMetadataTool
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct FileMetadataParams {
    /// Path to the file or directory to stat.
    pub path: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FileMetadataOutput {
    /// `"file"`, `"dir"`, or `"symlink"`.
    pub kind: String,
    /// Size in bytes (0 for directories).
    pub size_bytes: u64,
    /// Whether the current process can read the path.
    pub readonly: bool,
    /// Last-modified time as a Unix timestamp (seconds), if available.
    pub modified_secs: Option<u64>,
    /// Creation time as a Unix timestamp (seconds), if available.
    pub created_secs: Option<u64>,
}

/// Retrieve metadata (size, kind, permissions, timestamps) for a path.
pub struct FileMetadataTool;

impl ToolBase for FileMetadataTool {
    type Parameter = FileMetadataParams;
    type Output = FileMetadataOutput;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        "file_metadata".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "Return metadata for a file or directory: kind, size, \
             read-only flag, and modification / creation timestamps."
                .into(),
        )
    }
}

impl SyncTool<Toolbox> for FileMetadataTool {
    fn invoke(_ctx: &Toolbox, p: FileMetadataParams) -> Result<FileMetadataOutput, ErrorData> {
        let meta =
            std::fs::metadata(&p.path).map_err(|e| io_err("failed to stat path", &p.path, e))?;

        let kind = if meta.is_dir() {
            "dir"
        } else if meta.is_symlink() {
            "symlink"
        } else {
            "file"
        }
        .to_string();

        let modified_secs = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        let created_secs = meta
            .created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        Ok(FileMetadataOutput {
            kind,
            size_bytes: meta.len(),
            readonly: meta.permissions().readonly(),
            modified_secs,
            created_secs,
        })
    }
}
