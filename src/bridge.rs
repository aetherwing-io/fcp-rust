// Slipstream bridge — connects FCP server to the Slipstream daemon via Unix socket.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::domain::mutation::dispatch_mutation;
use crate::domain::query::dispatch_query;
use crate::mcp::server::RustServer;

// ---------------------------------------------------------------------------
// JSON-RPC types (private to this module)
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct JsonRpcRequest {
    id: serde_json::Value,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(serde::Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(serde::Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

// ---------------------------------------------------------------------------
// Socket discovery
// ---------------------------------------------------------------------------

fn discover_socket() -> Option<String> {
    if let Ok(path) = std::env::var("SLIPSTREAM_SOCKET") {
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }
    if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        let path = format!("{}/slipstream/daemon.sock", xdg);
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }
    #[cfg(unix)]
    {
        let uid = unsafe { libc::getuid() };
        let path = format!("/tmp/slipstream-{}/daemon.sock", uid);
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Bridge entry point
// ---------------------------------------------------------------------------

/// Connect to the slipstream daemon and handle requests.
/// Silently returns on any failure. Call via `tokio::spawn`.
pub async fn connect(server: RustServer) {
    let _ = run_bridge(server).await;
}

async fn run_bridge(server: RustServer) -> Result<(), Box<dyn std::error::Error>> {
    let path = discover_socket().ok_or("no socket")?;
    let stream = UnixStream::connect(&path).await?;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    // Send registration
    let register = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "fcp.register",
        "params": {
            "handler_name": "fcp-rust",
            "extensions": ["rs"],
            "capabilities": ["ops", "query", "session"]
        }
    });
    writer
        .write_all(format!("{}\n", register).as_bytes())
        .await?;

    // Request loop
    while let Some(line) = lines.next_line().await? {
        let req: JsonRpcRequest = serde_json::from_str(&line)?;
        let response = handle_request(&server, req).await;
        let json = serde_json::to_string(&response)?;
        writer
            .write_all(format!("{}\n", json).as_bytes())
            .await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Request dispatch
// ---------------------------------------------------------------------------

async fn handle_request(server: &RustServer, req: JsonRpcRequest) -> JsonRpcResponse {
    let params = req.params.unwrap_or(serde_json::Value::Null);

    let text = match req.method.as_str() {
        "fcp.session" => {
            let action = params
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            server.handle_session(action).await
        }
        "fcp.ops" => {
            let ops: Vec<String> = params
                .get("ops")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let mut results = Vec::new();
            for op in &ops {
                let model = server.model.lock().await;
                let result = dispatch_mutation(&model, &server.registry, op).await;
                results.push(result);
            }
            results.join("\n")
        }
        "fcp.query" => {
            let q = params.get("q").and_then(|v| v.as_str()).unwrap_or("");
            let model = server.model.lock().await;
            dispatch_query(&model, &server.registry, q).await
        }
        "fcp.enrich" => {
            let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let result = fcp_core::enrich::enrich(path);
            serde_json::to_string(&result).unwrap_or_else(|_| {
                r#"{"diagnostics":[],"symbols":[]}"#.to_string()
            })
        }
        _ => format!("unknown method: {}", req.method),
    };

    JsonRpcResponse {
        jsonrpc: "2.0",
        id: req.id,
        result: Some(serde_json::json!({"text": text})),
        error: None,
    }
}
