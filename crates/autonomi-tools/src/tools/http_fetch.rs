//! # HttpFetch
//!
//! A single async tool that performs a GET request and returns the response body.
//!
//! | Tool | Description |
//! |------|-------------|
//! | [`HttpFetchTool`] | GET a URL and return the response body as a string |
//!
//! The tool is async and stateless — it implements [`AsyncTool<Toolbox>`] and
//! can be registered with [`ToolboxBuilder::with_async_tool`].

use std::borrow::Cow;

use rmcp::{schemars, ErrorData};
use serde::{Deserialize, Serialize};

use crate::toolbox::Toolbox;
use crate::{AsyncTool, ToolBase};

// ═══════════════════════════════════════════════════════════════════════════════
// HttpFetchTool
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct HttpFetchParams {
    /// The URL to fetch (must be http:// or https://).
    pub url: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct HttpFetchOutput {
    /// HTTP status code returned by the server.
    pub status: u16,
    /// Response body as a UTF-8 string.
    pub body: String,
}

/// Perform an HTTP GET request and return the status code and response body.
pub struct HttpFetchTool;

impl ToolBase for HttpFetchTool {
    type Parameter = HttpFetchParams;
    type Output = HttpFetchOutput;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        "http_fetch".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "Perform an HTTP GET request to the given URL and return the \
             response status code and body."
                .into(),
        )
    }
}

impl AsyncTool<Toolbox> for HttpFetchTool {
    async fn invoke(_ctx: &Toolbox, p: HttpFetchParams) -> Result<HttpFetchOutput, ErrorData> {
        let response = reqwest::get(&p.url).await.map_err(|e| {
            ErrorData::invalid_params(format!("GET '{}' failed: {}", p.url, e), None)
        })?;

        let status = response.status().as_u16();

        let body = response.text().await.map_err(|e| {
            ErrorData::invalid_params(
                format!("failed to read response body from '{}': {}", p.url, e),
                None,
            )
        })?;

        Ok(HttpFetchOutput { status, body })
    }
}
