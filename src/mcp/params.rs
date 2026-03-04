// MCP tool parameter types
#![allow(dead_code)]

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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HelpParams {}
