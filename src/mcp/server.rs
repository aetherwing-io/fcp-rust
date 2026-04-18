// MCP server integration via rmcp

use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::*,
    tool, tool_handler, tool_router,
};
use tokio::sync::Mutex;
use url::Url;

use crate::domain::model::RustModel;
use crate::domain::mutation::dispatch_mutation;
use crate::domain::query::dispatch_query;
use crate::domain::verbs::{register_mutation_verbs, register_query_verbs, register_session_verbs};
use crate::fcpcore::verb_registry::VerbRegistry;
use crate::lsp::client::LspClient;
use crate::lsp::lifecycle::ServerStatus;
use crate::lsp::types::{JsonRpcNotification, PublishDiagnosticsParams, SymbolInformation};
use crate::resolver::index::SymbolEntry;
use super::params::*;

#[derive(Clone)]
pub struct RustServer {
    pub(crate) model: Arc<Mutex<RustModel>>,
    pub(crate) registry: Arc<VerbRegistry>,
    tool_router: ToolRouter<Self>,
    notification_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

fn make_registry() -> VerbRegistry {
    let mut reg = VerbRegistry::new();
    register_query_verbs(&mut reg);
    register_mutation_verbs(&mut reg);
    register_session_verbs(&mut reg);
    reg
}

impl Default for RustServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl RustServer {
    pub fn new() -> Self {
        let model = RustModel::new(Url::parse("file:///").unwrap());
        Self {
            model: Arc::new(Mutex::new(model)),
            registry: Arc::new(make_registry()),
            tool_router: Self::tool_router(),
            notification_task: Arc::new(Mutex::new(None)),
        }
    }

    #[tool(description = "Execute Rust mutation operations. Examples: 'rename Config Settings', 'extract validate @file:server.rs @lines:15-30', 'inline helper_fn @file:lib.rs', 'generate Display @struct:Config', 'import HashMap @file:main.rs @line:5'")]
    async fn rust(
        &self,
        Parameters(p): Parameters<MutationParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut results = Vec::new();
        for op in &p.ops {
            let model = self.model.lock().await;
            let result = dispatch_mutation(&model, &self.registry, op).await;
            results.push(result);
        }
        // Refresh diagnostics after mutations
        self.maybe_refresh_diagnostics().await;
        Ok(CallToolResult::success(vec![Content::text(
            results.join("\n\n"),
        )]))
    }

    #[tool(description = "Execute a read-only FCP query on the Rust workspace. Examples: 'find Config', 'def main @file:main.rs', 'diagnose', 'unused', 'map'")]
    async fn rust_query(
        &self,
        Parameters(p): Parameters<QueryParams>,
    ) -> Result<CallToolResult, McpError> {
        // Auto-refresh diagnostics if files changed
        let trimmed = p.input.trim_start();
        if trimmed.starts_with("diagnose") || trimmed.starts_with("unused") {
            self.maybe_refresh_diagnostics().await;
        }
        let model = self.model.lock().await;
        let result = dispatch_query(&model, &self.registry, &p.input).await;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Manage the Rust workspace session. Actions: 'open PATH' to open a workspace, 'status' to check server status, 'close' to close the workspace.")]
    async fn rust_session(
        &self,
        Parameters(p): Parameters<SessionParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = self.handle_session(&p.action).await;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Show the FCP Rust reference card with all available verbs and their syntax.")]
    async fn rust_help(&self) -> Result<CallToolResult, McpError> {
        let extra = std::collections::HashMap::from([
            (
                "Selectors".to_string(),
                "  @file:PATH    — filter by file path\n  \
                 @struct:NAME  — filter by containing struct\n  \
                 @trait:NAME   — filter by containing trait\n  \
                 @kind:KIND    — filter by symbol kind (function, struct, enum, ...)\n  \
                 @module:NAME  — filter by module\n  \
                 @line:N       — filter by line number\n  \
                 @lines:N-M    — line range for extract"
                    .to_string(),
            ),
            (
                "Mutation Examples".to_string(),
                "  rust [\"rename Config Settings\"]              — cross-file semantic rename\n  \
                 rust [\"extract validate @file:server.rs @lines:15-30\"] — extract function\n  \
                 rust [\"inline helper_fn @file:lib.rs\"]         — inline function at call sites\n  \
                 rust [\"generate Debug @struct:Config\"]         — add #[derive(Debug)] (or extend existing)\n  \
                 rust [\"import HashMap @file:main.rs @line:5\"]  — add missing use statement"
                    .to_string(),
            ),
        ]);
        let card = self.registry.generate_reference_card(Some(&extra));
        Ok(CallToolResult::success(vec![Content::text(card)]))
    }

    #[tool(description = "Enrich a Rust source file with diagnostics (from cargo check) and symbols (top-level items). Takes an absolute file path, returns JSON with diagnostics and symbols arrays. Fast: warm cargo check <2s, symbols <100ms.")]
    async fn enrich(
        &self,
        Parameters(p): Parameters<EnrichParams>,
    ) -> Result<CallToolResult, McpError> {
        // Run enrichment in a blocking task since cargo check is a subprocess
        let path = p.path.clone();
        let result = tokio::task::spawn_blocking(move || {
            fcp_core::enrich::enrich(&path)
        })
        .await
        .unwrap_or_else(|_| fcp_core::enrich::EnrichResult {
            diagnostics: Vec::new(),
            symbols: Vec::new(),
        });

        let json = serde_json::to_string_pretty(&result).unwrap_or_else(|_| {
            r#"{"diagnostics":[],"symbols":[]}"#.to_string()
        });

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

impl RustServer {
    pub(crate) async fn handle_session(&self, action: &str) -> String {
        let tokens: Vec<&str> = action.split_whitespace().collect();
        if tokens.is_empty() {
            return "! empty session action.".to_string();
        }

        match tokens[0] {
            "open" => {
                if tokens.len() < 2 {
                    return "! open requires a path.".to_string();
                }
                let path = tokens[1];
                self.handle_open(path).await
            }
            "status" => self.handle_status().await,
            "close" => self.handle_close().await,
            _ => format!("! unknown session action '{}'.", tokens[0]),
        }
    }

    async fn handle_open(&self, path: &str) -> String {
        let uri = if path.starts_with("file://") {
            match Url::parse(path) {
                Ok(u) => u,
                Err(e) => return format!("! invalid URI: {}", e),
            }
        } else {
            match Url::from_file_path(path) {
                Ok(u) => u,
                Err(_) => return format!("! invalid path: {}", path),
            }
        };

        // Check path exists
        if !path.starts_with("file://") {
            let p = std::path::Path::new(path);
            if !p.exists() {
                return format!("! path not found: {}", path);
            }
        }

        // Spawn rust-analyzer
        let mut client = match LspClient::spawn("rust-analyzer", &[], uri.as_str()).await {
            Ok(c) => c,
            Err(e) => return format!("! failed to start rust-analyzer: {}", e),
        };

        // Take notification receiver before wrapping in mutex
        let notification_rx = client
            .take_notification_rx()
            .expect("notification_rx should be available on fresh client");

        let client = Arc::new(Mutex::new(client));

        // Set initial model state
        let rs_file_count = {
            let mut model = self.model.lock().await;
            model.root_uri = uri;
            model.lsp_client = Some(Arc::clone(&client));
            if let Ok(root_path) = model.root_uri.to_file_path() {
                model.rs_file_count = count_rs_files(&root_path);
            }
            model.rs_file_count
        };

        // Spawn notification handler
        let model_clone = Arc::clone(&self.model);
        let task = tokio::spawn(Self::notification_handler(notification_rx, model_clone));
        *self.notification_task.lock().await = Some(task);

        // Populate initial symbol index
        let symbol_count = Self::populate_initial_index(&client, &self.model).await;

        // Mark as ready
        {
            let mut model = self.model.lock().await;
            model.server_status = ServerStatus::Ready;
        }

        // Record initial reload time
        {
            let mut model = self.model.lock().await;
            model.last_reload = Some(std::time::SystemTime::now());
        }

        format!(
            "Opened workspace: {} ({} files, {} symbols)",
            path, rs_file_count, symbol_count,
        )
    }

    async fn handle_status(&self) -> String {
        let model = self.model.lock().await;
        let status_str = match model.server_status {
            ServerStatus::NotStarted => "not started",
            ServerStatus::Starting => "starting",
            ServerStatus::Ready => "ready",
            ServerStatus::Indexing => "indexing",
            ServerStatus::Crashed => "crashed",
            ServerStatus::Stopped => "stopped",
        };
        let (errors, warnings) = model.total_diagnostics();
        format!(
            "Status: {}\nWorkspace: {}\nFiles: {}\nSymbols: {}\nDiagnostics: {} errors, {} warnings",
            status_str,
            model.root_uri.as_str(),
            model.rs_file_count,
            model.symbol_index.size(),
            errors,
            warnings,
        )
    }

    /// Auto-refresh diagnostics if .rs files have been modified since last reload.
    async fn maybe_refresh_diagnostics(&self) {
        // Step 1: Read model state briefly
        let (root_path, client_arc, last_reload) = {
            let model = self.model.lock().await;
            let root_path = match model.root_uri.to_file_path() {
                Ok(p) => p,
                Err(_) => return,
            };
            let client = match &model.lsp_client {
                Some(c) => Arc::clone(c),
                None => return,
            };
            (root_path, client, model.last_reload)
        };

        // Step 2: Check if any .rs files are newer than last reload
        let newest = newest_rs_mtime(&root_path);
        let needs_refresh = match (newest, last_reload) {
            (Some(newest_time), Some(reload_time)) => newest_time > reload_time,
            (Some(_), None) => true,
            _ => false,
        };

        if !needs_refresh {
            return;
        }

        // Step 3: Collect changed file URIs
        let changed_uris = changed_rs_files(&root_path, last_reload);
        if changed_uris.is_empty() {
            return;
        }

        // Step 4: Clear diagnostics cache
        {
            let mut model = self.model.lock().await;
            model.diagnostics.clear();
        }

        // Step 5: Send didChangeWatchedFiles notification (scoped client lock)
        {
            let changes: Vec<serde_json::Value> = changed_uris
                .iter()
                .map(|uri| serde_json::json!({"uri": uri, "type": 2}))
                .collect();
            let client = client_arc.lock().await;
            let _ = client
                .notify(
                    "workspace/didChangeWatchedFiles",
                    serde_json::json!({"changes": changes}),
                )
                .await;
        }

        // Step 6: Poll for diagnostics to settle
        let mut prev_count: usize = 0;
        let mut stable_rounds = 0;
        let mut seen_nonzero = false;
        for _ in 0..10 {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let count = {
                let model = self.model.lock().await;
                model.diagnostic_count()
            };
            if count > 0 {
                seen_nonzero = true;
            }
            if seen_nonzero && count == prev_count {
                stable_rounds += 1;
                if stable_rounds >= 2 {
                    break;
                }
            } else {
                stable_rounds = 0;
            }
            prev_count = count;
        }

        // Step 7: Update last_reload timestamp
        {
            let mut model = self.model.lock().await;
            model.last_reload = Some(std::time::SystemTime::now());
        }
    }

    async fn handle_close(&self) -> String {
        // Abort notification task
        if let Some(task) = self.notification_task.lock().await.take() {
            task.abort();
        }

        let mut model = self.model.lock().await;

        // Shutdown LSP client
        if let Some(client) = model.lsp_client.take() {
            let mut client = client.lock().await;
            let _ = client.shutdown().await;
        }

        // Clear model state
        model.server_status = ServerStatus::Stopped;
        model.symbol_index = crate::resolver::index::SymbolIndex::new();
        model.diagnostics.clear();
        model.open_documents.clear();

        "Workspace closed.".to_string()
    }

    async fn notification_handler(
        mut rx: tokio::sync::mpsc::Receiver<JsonRpcNotification>,
        model: Arc<Mutex<RustModel>>,
    ) {
        while let Some(notif) = rx.recv().await {
            match notif.method.as_str() {
                "textDocument/publishDiagnostics" => {
                    if let Some(params) = notif.params {
                        if let Ok(diag_params) =
                            serde_json::from_value::<PublishDiagnosticsParams>(params)
                        {
                            let mut model = model.lock().await;
                            model.update_diagnostics(
                                &diag_params.uri,
                                diag_params.diagnostics,
                            );
                        }
                    }
                }
                _ => {
                    tracing::trace!("ignoring notification: {}", notif.method);
                }
            }
        }
    }

    async fn populate_initial_index(
        client: &Arc<Mutex<LspClient>>,
        model: &Arc<Mutex<RustModel>>,
    ) -> usize {
        const MAX_RETRIES: usize = 10;
        const RETRY_DELAY_MS: u64 = 500;

        let mut symbols: Vec<SymbolInformation> = Vec::new();

        for attempt in 0..MAX_RETRIES {
            let result = {
                let client = client.lock().await;
                client
                    .request::<Vec<SymbolInformation>>(
                        "workspace/symbol",
                        serde_json::json!({"query": "*"}),
                    )
                    .await
            };

            match result {
                Ok(s) if !s.is_empty() => {
                    symbols = s;
                    break;
                }
                Ok(_) => {
                    tracing::debug!(
                        "populate_initial_index: attempt {}/{} returned 0 symbols, retrying",
                        attempt + 1,
                        MAX_RETRIES,
                    );
                }
                Err(e) => {
                    tracing::debug!(
                        "populate_initial_index: attempt {}/{} failed: {}, retrying",
                        attempt + 1,
                        MAX_RETRIES,
                        e,
                    );
                }
            }

            if attempt + 1 < MAX_RETRIES {
                tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
            }
        }

        let count = symbols.len();
        if count > 0 {
            let mut model = model.lock().await;
            for sym in &symbols {
                model.symbol_index.insert(SymbolEntry {
                    name: sym.name.clone(),
                    kind: sym.kind,
                    container_name: sym.container_name.clone(),
                    uri: sym.location.uri.clone(),
                    range: sym.location.range.clone(),
                    selection_range: sym.location.range.clone(),
                });
            }
        }
        count
    }
}

fn should_skip_dir(name: &std::ffi::OsStr) -> bool {
    let s = name.to_string_lossy();
    s == "target" || s.starts_with('.')
}

fn count_rs_files(path: &std::path::Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if !should_skip_dir(&entry.file_name()) {
                    count += count_rs_files(&p);
                }
            } else if p.extension().is_some_and(|ext| ext == "rs") {
                count += 1;
            }
        }
    }
    count
}

/// Find the newest mtime among .rs files, skipping target/ and hidden dirs.
fn newest_rs_mtime(path: &std::path::Path) -> Option<std::time::SystemTime> {
    let mut newest: Option<std::time::SystemTime> = None;
    walk_rs_files(path, &mut |entry| {
        if let Ok(meta) = entry.metadata() {
            if let Ok(mtime) = meta.modified() {
                newest = Some(match newest {
                    Some(prev) if mtime > prev => mtime,
                    Some(prev) => prev,
                    None => mtime,
                });
            }
        }
    });
    newest
}

/// Collect URIs of .rs files modified since `since`.
fn changed_rs_files(
    path: &std::path::Path,
    since: Option<std::time::SystemTime>,
) -> Vec<String> {
    let mut uris = Vec::new();
    walk_rs_files(path, &mut |entry| {
        let dominated = match since {
            Some(since_time) => entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|mt| mt > since_time)
                .unwrap_or(false),
            None => true,
        };
        if dominated {
            if let Ok(canonical) = entry.path().canonicalize() {
                if let Ok(uri) = Url::from_file_path(&canonical) {
                    uris.push(uri.to_string());
                }
            }
        }
    });
    uris
}

/// Walk .rs files under `path`, skipping target/ and hidden dirs.
fn walk_rs_files(path: &std::path::Path, cb: &mut dyn FnMut(&std::fs::DirEntry)) {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if !should_skip_dir(&entry.file_name()) {
                    walk_rs_files(&p, cb);
                }
            } else if p.extension().is_some_and(|ext| ext == "rs") {
                cb(&entry);
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for RustServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "fcp-rust".to_string(),
                version: "0.1.0".to_string(),
                ..Default::default()
            },
            instructions: Some(
                "FCP Rust server for querying and navigating Rust codebases via rust-analyzer. \
                 Use rust_session to open a workspace directory containing a Cargo.toml, \
                 rust_query for read-only queries like finding definitions, references, \
                 diagnostics, and symbols, rust for refactoring operations, and rust_help \
                 for the full verb reference. Start every interaction with rust_session."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rust_tool_no_workspace() {
        let server = RustServer::new();
        let params = MutationParams {
            ops: vec!["rename Config Settings".to_string()],
        };
        let result = server.rust(Parameters(params)).await.unwrap();
        let text = get_text(&result);
        assert!(text.contains("no workspace open"));
    }

    #[tokio::test]
    async fn test_rust_query_dispatches() {
        let server = RustServer::new();
        let params = QueryParams {
            input: "find Config".to_string(),
        };
        let result = server.rust_query(Parameters(params)).await.unwrap();
        let text = get_text(&result);
        // Should dispatch and return a result (no symbols in empty model)
        assert!(text.contains("no symbols matching"));
    }

    #[tokio::test]
    async fn test_rust_query_empty_input() {
        let server = RustServer::new();
        let params = QueryParams {
            input: "".to_string(),
        };
        let result = server.rust_query(Parameters(params)).await.unwrap();
        let text = get_text(&result);
        assert!(text.contains("parse error"));
    }

    #[tokio::test]
    async fn test_rust_session_status_no_workspace() {
        let server = RustServer::new();
        let params = SessionParams {
            action: "status".to_string(),
        };
        let result = server.rust_session(Parameters(params)).await.unwrap();
        let text = get_text(&result);
        assert!(text.contains("Status:"));
    }

    #[tokio::test]
    async fn test_rust_session_close() {
        let server = RustServer::new();
        let params = SessionParams {
            action: "close".to_string(),
        };
        let result = server.rust_session(Parameters(params)).await.unwrap();
        let text = get_text(&result);
        assert!(text.contains("closed"));
    }

    #[tokio::test]
    async fn test_rust_session_open_invalid_path() {
        let server = RustServer::new();
        let params = SessionParams {
            action: "open /nonexistent/path/that/should/not/exist".to_string(),
        };
        let result = server.rust_session(Parameters(params)).await.unwrap();
        let text = get_text(&result);
        assert!(text.contains("not found") || text.contains("invalid"));
    }

    #[tokio::test]
    async fn test_rust_help_returns_reference_card() {
        let server = RustServer::new();
        let result = server.rust_help().await.unwrap();
        let text = get_text(&result);
        assert!(text.contains("find QUERY"));
        assert!(text.contains("def SYMBOL"));
        assert!(text.contains("rename SYMBOL"));
        assert!(text.contains("open PATH"));
        assert!(text.contains("SELECTORS:"));
        assert!(text.contains("MUTATION:"));
        assert!(text.contains("MUTATION EXAMPLES:"));
    }

    #[tokio::test]
    async fn test_server_info() {
        let server = RustServer::new();
        let info = server.get_info();
        assert!(info.instructions.is_some());
        let instructions = info.instructions.unwrap();
        assert!(instructions.contains("FCP Rust"));
    }

    #[tokio::test]
    async fn test_rust_session_empty_action() {
        let server = RustServer::new();
        let params = SessionParams {
            action: "".to_string(),
        };
        let result = server.rust_session(Parameters(params)).await.unwrap();
        let text = get_text(&result);
        assert!(text.contains("empty session action"));
    }

    #[tokio::test]
    async fn test_enrich_nonexistent_file() {
        let server = RustServer::new();
        let params = EnrichParams {
            path: "/nonexistent/path/to/file.rs".to_string(),
            content_hash: None,
        };
        let result = server.enrich(Parameters(params)).await.unwrap();
        let text = get_text(&result);
        // Should return valid JSON with empty arrays, not an error
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(parsed["diagnostics"].as_array().unwrap().is_empty());
        assert!(parsed["symbols"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_enrich_returns_symbols() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(
            &file_path,
            "fn main() {\n    println!(\"hello\");\n}\n\nstruct Config {\n    port: u16,\n}\n",
        )
        .unwrap();
        let server = RustServer::new();
        let params = EnrichParams {
            path: file_path.to_str().unwrap().to_string(),
            content_hash: None,
        };
        let result = server.enrich(Parameters(params)).await.unwrap();
        let text = get_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        let symbols = parsed["symbols"].as_array().unwrap();
        assert!(symbols.len() >= 2, "expected at least 2 symbols, got: {}", text);
        let names: Vec<&str> = symbols.iter().filter_map(|s| s["name"].as_str()).collect();
        assert!(names.contains(&"main"), "missing main in {:?}", names);
        assert!(names.contains(&"Config"), "missing Config in {:?}", names);
    }

    fn get_text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| match &c.raw {
                RawContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    fn create_test_project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/main.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n\nfn helper() -> i32 {\n    42\n}\n",
        )
        .unwrap();
        dir
    }

    #[tokio::test]
    #[ignore]
    async fn test_open_real_workspace() {
        let dir = create_test_project();
        let server = RustServer::new();
        let path = dir.path().to_str().unwrap();
        let result = server.handle_session(&format!("open {}", path)).await;
        assert!(result.contains("Opened workspace:"), "got: {}", result);
        assert!(result.contains("files"), "got: {}", result);
        assert!(result.contains("symbols"), "got: {}", result);

        // Verify status is ready
        let status = server.handle_session("status").await;
        assert!(status.contains("ready"), "got: {}", status);

        // Clean up
        server.handle_session("close").await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_query_after_open() {
        let dir = create_test_project();
        let server = RustServer::new();
        let path = dir.path().to_str().unwrap();
        server.handle_session(&format!("open {}", path)).await;

        // Give rust-analyzer time to index
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        let model = server.model.lock().await;
        let result = dispatch_query(&model, &server.registry, "find main").await;
        drop(model);
        assert!(
            result.contains("main") && !result.contains("no symbols"),
            "got: {}",
            result
        );

        server.handle_session("close").await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_close_after_open() {
        let dir = create_test_project();
        let server = RustServer::new();
        let path = dir.path().to_str().unwrap();
        server.handle_session(&format!("open {}", path)).await;

        let result = server.handle_session("close").await;
        assert!(result.contains("closed"), "got: {}", result);

        // Verify clean state
        let model = server.model.lock().await;
        assert!(model.lsp_client.is_none());
        assert_eq!(model.symbol_index.size(), 0);
        assert!(model.diagnostics.is_empty());
        assert!(model.open_documents.is_empty());
        assert_eq!(model.server_status, ServerStatus::Stopped);
    }
}
