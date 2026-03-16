//! # Built-in tools
//!
//! Ready-made tools that can be registered with any [`Toolbox`] server.
//!
//! ## Modules
//!
//! | Module | Tools |
//! |--------|-------|
//! | [`readonly_filesystem`] | [`ReadFileTool`], [`ListDirTool`], [`FileMetadataTool`] |
//! | [`http_fetch`] | [`HttpFetchTool`] |
//! | [`process_info`] | [`EnvVarsTool`], [`WorkingDirTool`], [`HostnameTool`] |

pub mod http_fetch;
pub mod process_info;
pub mod readonly_filesystem;

pub use http_fetch::HttpFetchTool;
pub use process_info::{EnvVarsTool, HostnameTool, WorkingDirTool};
pub use readonly_filesystem::{FileMetadataTool, ListDirTool, ReadFileTool};
