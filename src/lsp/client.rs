// LSP client implementation

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use serde::de::DeserializeOwned;
use tokio::io::BufReader;
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::error::{FcpRustError, Result};
use super::transport::{read_loop, LspWriter};
use super::types::*;

pub struct LspClient {
    writer: LspWriter<tokio::process::ChildStdin>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>>,
    next_id: Arc<AtomicI64>,
    server_capabilities: Option<ServerCapabilities>,
    child: Child,
    notification_rx: mpsc::Receiver<JsonRpcNotification>,
}

impl LspClient {
    /// Spawn an LSP server process and initialize the protocol.
    pub async fn spawn(command: &str, args: &[&str], root_uri: &str) -> Result<Self> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        let stdin = child.stdin.take().ok_or_else(|| {
            FcpRustError::Transport("failed to capture child stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            FcpRustError::Transport("failed to capture child stdout".to_string())
        })?;

        let writer = LspWriter::new(stdin);
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (notification_tx, notification_rx) = mpsc::channel(64);

        // Start the read loop in the background
        let reader = BufReader::new(stdout);
        tokio::spawn(read_loop(reader, Arc::clone(&pending), notification_tx));

        let mut client = Self {
            writer,
            pending,
            next_id: Arc::new(AtomicI64::new(1)),
            server_capabilities: None,
            child,
            notification_rx,
        };

        let caps = client.initialize(root_uri).await?;
        client.server_capabilities = Some(caps);

        // Send initialized notification
        client.notify("initialized", serde_json::json!({})).await?;

        Ok(client)
    }

    /// Perform the LSP initialize handshake.
    async fn initialize(&mut self, root_uri: &str) -> Result<ServerCapabilities> {
        let params = serde_json::json!({
            "processId": std::process::id() as i64,
            "rootUri": root_uri,
            "capabilities": {
                "general": {
                    "positionEncodings": ["utf-32"]
                },
                "textDocument": {
                    "codeAction": {
                        "codeActionLiteralSupport": {
                            "codeActionKind": {
                                "valueSet": [
                                    "quickfix",
                                    "refactor",
                                    "refactor.extract",
                                    "refactor.inline",
                                    "refactor.rewrite",
                                    "source",
                                    "source.organizeImports"
                                ]
                            }
                        },
                        // Don't advertise resolveSupport so rust-analyzer
                        // returns fully-resolved code actions with edits
                        // included, rather than lazy actions needing a
                        // separate codeAction/resolve round-trip.
                    },
                    "rename": {
                        "prepareSupport": false
                    }
                },
                "workspace": {
                    "applyEdit": true,
                    "workspaceEdit": {
                        "documentChanges": true,
                        "resourceOperations": ["create", "rename", "delete"]
                    }
                }
            },
            "initializationOptions": null
        });

        let result: InitializeResult = self.request("initialize", params).await?;
        Ok(result.capabilities)
    }

    /// Send a JSON-RPC request and wait for the response.
    pub async fn request<R: DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<R> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let id_val = serde_json::Value::Number(serde_json::Number::from(id));

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.to_string(), tx);

        self.writer
            .send_request(id_val, method, params)
            .await?;

        let response = rx.await.map_err(|_| {
            FcpRustError::Transport("response channel closed".to_string())
        })?;

        if let Some(err) = response.error {
            return Err(FcpRustError::LspServer {
                code: err.code,
                message: err.message,
            });
        }

        // JSON null deserializes as None for Option<Value>, but null is a
        // valid JSON-RPC result (e.g. shutdown). Treat missing result as null
        // when there is no error — per JSON-RPC 2.0, no error means success.
        let result = response.result.unwrap_or(serde_json::Value::Null);

        serde_json::from_value(result).map_err(Into::into)
    }

    /// Send a JSON-RPC notification (no response expected).
    pub async fn notify(&self, method: &str, params: serde_json::Value) -> Result<()> {
        self.writer.send_notification(method, params).await
    }

    /// Send textDocument/didOpen notification.
    pub async fn did_open(&self, uri: &str, text: &str) -> Result<()> {
        let params = serde_json::json!({
            "textDocument": {
                "uri": uri,
                "languageId": "rust",
                "version": 1,
                "text": text,
            }
        });
        self.notify("textDocument/didOpen", params).await
    }

    /// Send textDocument/didChange notification (full sync).
    #[allow(dead_code)]
    pub async fn did_change(&self, uri: &str, version: i32, text: &str) -> Result<()> {
        let params = serde_json::json!({
            "textDocument": {
                "uri": uri,
                "version": version,
            },
            "contentChanges": [{"text": text}]
        });
        self.notify("textDocument/didChange", params).await
    }

    /// Send textDocument/didClose notification.
    #[allow(dead_code)]
    pub async fn did_close(&self, uri: &str) -> Result<()> {
        let params = serde_json::json!({
            "textDocument": {
                "uri": uri,
            }
        });
        self.notify("textDocument/didClose", params).await
    }

    /// Send shutdown request and exit notification.
    pub async fn shutdown(&mut self) -> Result<()> {
        let _: serde_json::Value = self.request("shutdown", serde_json::Value::Null).await?;
        self.notify("exit", serde_json::json!(null)).await?;
        let _ = self.child.wait().await;
        Ok(())
    }

    /// Get the server capabilities from the initialize response.
    #[allow(dead_code)]
    pub fn capabilities(&self) -> Option<&ServerCapabilities> {
        self.server_capabilities.as_ref()
    }

    /// Take the notification receiver (for use in external processing).
    pub fn take_notification_rx(&mut self) -> Option<mpsc::Receiver<JsonRpcNotification>> {
        let mut rx = mpsc::channel(1).1; // dummy
        std::mem::swap(&mut self.notification_rx, &mut rx);
        Some(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use super::super::transport::encode_message;

    /// Test that the request method correctly sends and receives via duplex streams.
    #[tokio::test]
    async fn test_request_response_via_duplex() {
        let (client_read, mut server_write) = tokio::io::duplex(8192);
        let (server_read, client_write) = tokio::io::duplex(8192);

        let writer = LspWriter::new(client_write);
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (notification_tx, _notification_rx) = mpsc::channel(64);

        // Start read loop on the client's reading side
        tokio::spawn(read_loop(
            client_read,
            Arc::clone(&pending),
            notification_tx,
        ));

        // Register a pending request for id=1
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert("1".to_string(), tx);

        // Send a request
        writer
            .send_request(json!(1), "test/method", json!({"key": "value"}))
            .await
            .unwrap();

        // Read what was sent on the server side
        let mut buf_reader = tokio::io::BufReader::new(server_read);
        let msg = super::super::transport::decode_message(&mut buf_reader).await.unwrap();
        assert_eq!(msg["method"], "test/method");
        assert_eq!(msg["id"], 1);

        // Server sends back a response
        let resp = json!({"jsonrpc": "2.0", "id": 1, "result": {"status": "ok"}});
        let body = serde_json::to_vec(&resp).unwrap();
        let frame = encode_message(&body);
        server_write.write_all(&frame).await.unwrap();
        server_write.flush().await.unwrap();

        // Client receives the response
        let response = rx.await.unwrap();
        assert_eq!(response.result.unwrap()["status"], "ok");
    }

    /// Test that notifications are dispatched correctly.
    #[tokio::test]
    async fn test_notification_dispatch_via_duplex() {
        let (client_read, mut server_write) = tokio::io::duplex(8192);

        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (notification_tx, mut notification_rx) = mpsc::channel(64);

        tokio::spawn(read_loop(
            client_read,
            Arc::clone(&pending),
            notification_tx,
        ));

        // Server sends a notification
        let notif = json!({"jsonrpc": "2.0", "method": "window/logMessage", "params": {"message": "hello"}});
        let body = serde_json::to_vec(&notif).unwrap();
        let frame = encode_message(&body);
        server_write.write_all(&frame).await.unwrap();
        server_write.flush().await.unwrap();

        let received = notification_rx.recv().await.unwrap();
        assert_eq!(received.method, "window/logMessage");
    }

    /// Test error response handling.
    #[tokio::test]
    async fn test_error_response_via_duplex() {
        let (client_read, mut server_write) = tokio::io::duplex(8192);

        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (notification_tx, _notification_rx) = mpsc::channel(64);

        tokio::spawn(read_loop(
            client_read,
            Arc::clone(&pending),
            notification_tx,
        ));

        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert("1".to_string(), tx);

        // Server sends an error response
        let resp = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {"code": -32601, "message": "Method not found"}
        });
        let body = serde_json::to_vec(&resp).unwrap();
        let frame = encode_message(&body);
        server_write.write_all(&frame).await.unwrap();
        server_write.flush().await.unwrap();

        let response = rx.await.unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }

    use tokio::io::AsyncWriteExt;

    /// Integration test that requires rust-analyzer to be installed.
    #[tokio::test]
    #[ignore]
    async fn test_spawn_rust_analyzer() {
        let root_uri = "file:///tmp/test-project";
        let client = LspClient::spawn("rust-analyzer", &[], root_uri).await;
        assert!(client.is_ok());
        let mut client = client.unwrap();
        assert!(client.capabilities().is_some());
        client.shutdown().await.unwrap();
    }
}
