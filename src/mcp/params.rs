// MCP tool parameter types

use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryParams {
    /// FCP query operation string, e.g. 'def main @file:main.rs'
    pub input: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionParams {
    /// Session action: 'open PATH', 'status', 'close'
    pub action: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MutationParams {
    /// Mutation operation strings, e.g. 'rename Config Settings', 'extract validate @file:server.rs @lines:15-30'
    pub ops: Vec<String>,
}

#[allow(dead_code)] // content_hash reserved for future cache integration
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EnrichParams {
    /// Absolute path to a Rust source file to enrich with diagnostics and symbols.
    pub path: String,
    /// Optional content hash for caching — if the file hasn't changed, cached results may be returned faster.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

#[allow(dead_code)] // constructed via serde Deserialize
#[derive(Debug, Deserialize, JsonSchema)]
pub struct HelpParams {}
