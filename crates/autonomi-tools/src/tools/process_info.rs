//! # ProcessInfo
//!
//! Three read-only tools for inspecting the current process environment:
//!
//! | Tool | Description |
//! |------|-------------|
//! | [`EnvVarsTool`] | List all environment variables visible to the process |
//! | [`WorkingDirTool`] | Return the current working directory |
//! | [`HostnameTool`] | Return the machine hostname |
//!
//! All tools are synchronous and stateless — they implement
//! [`SyncTool<Toolbox>`] and can be registered with
//! [`ToolboxBuilder::with_sync_tool`].

use std::borrow::Cow;

use rmcp::{schemars, ErrorData};
use serde::{Deserialize, Serialize};

use crate::toolbox::Toolbox;
use crate::{SyncTool, ToolBase};

// ═══════════════════════════════════════════════════════════════════════════════
// EnvVarsTool
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct EnvVarsParams {
    /// Optional prefix filter — only variables whose name starts with this
    /// string (case-sensitive) will be returned.  Leave empty to return all.
    #[serde(default)]
    pub filter_prefix: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct EnvVarEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct EnvVarsOutput {
    /// Sorted list of environment variable key/value pairs.
    pub vars: Vec<EnvVarEntry>,
}

/// List environment variables visible to the current process.
pub struct EnvVarsTool;

impl ToolBase for EnvVarsTool {
    type Parameter = EnvVarsParams;
    type Output = EnvVarsOutput;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        "env_vars".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "List all environment variables visible to the server process. \
             Optionally filter by a key prefix."
                .into(),
        )
    }
}

impl SyncTool<Toolbox> for EnvVarsTool {
    fn invoke(_ctx: &Toolbox, p: EnvVarsParams) -> Result<EnvVarsOutput, ErrorData> {
        let mut vars: Vec<EnvVarEntry> = std::env::vars()
            .filter(|(k, _)| p.filter_prefix.is_empty() || k.starts_with(&p.filter_prefix))
            .map(|(key, value)| EnvVarEntry { key, value })
            .collect();

        vars.sort_by(|a, b| a.key.cmp(&b.key));

        Ok(EnvVarsOutput { vars })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// WorkingDirTool
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct WorkingDirParams {}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct WorkingDirOutput {
    /// Absolute path of the current working directory.
    pub path: String,
}

/// Return the current working directory of the server process.
pub struct WorkingDirTool;

impl ToolBase for WorkingDirTool {
    type Parameter = WorkingDirParams;
    type Output = WorkingDirOutput;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        "working_dir".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Return the current working directory of the server process.".into())
    }
}

impl SyncTool<Toolbox> for WorkingDirTool {
    fn invoke(_ctx: &Toolbox, _p: WorkingDirParams) -> Result<WorkingDirOutput, ErrorData> {
        let path = std::env::current_dir()
            .map_err(|e| {
                ErrorData::invalid_params(format!("failed to get working dir: {e}"), None)
            })?
            .to_string_lossy()
            .to_string();

        Ok(WorkingDirOutput { path })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// HostnameTool
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct HostnameParams {}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct HostnameOutput {
    /// The machine hostname as reported by the OS.
    pub hostname: String,
}

/// Return the hostname of the machine the server is running on.
pub struct HostnameTool;

impl ToolBase for HostnameTool {
    type Parameter = HostnameParams;
    type Output = HostnameOutput;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        "hostname".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some("Return the hostname of the machine the server process is running on.".into())
    }
}

impl SyncTool<Toolbox> for HostnameTool {
    fn invoke(_ctx: &Toolbox, _p: HostnameParams) -> Result<HostnameOutput, ErrorData> {
        let hostname = hostname::get()
            .map_err(|e| ErrorData::invalid_params(format!("failed to get hostname: {e}"), None))?
            .to_string_lossy()
            .to_string();

        Ok(HostnameOutput { hostname })
    }
}
