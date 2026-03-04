// Content-Length framed LSP transport

use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, Mutex};
use std::sync::Arc;
use std::collections::HashMap;

use crate::error::{FcpRustError, Result};
use super::types::{JsonRpcResponse, JsonRpcNotification};

/// Encode a message body with Content-Length header framing.
pub fn encode_message(body: &[u8]) -> Vec<u8> {
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut buf = Vec::with_capacity(header.len() + body.len());
    buf.extend_from_slice(header.as_bytes());
    buf.extend_from_slice(body);
    buf
}

/// Decode a Content-Length framed message from an async reader.
pub async fn decode_message<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<serde_json::Value> {
    let mut content_length: Option<usize> = None;

    // Read headers line by line
    loop {
        let mut line = Vec::new();
        // Read until \n
        loop {
            let mut byte = [0u8; 1];
            let n = reader.read(&mut byte).await?;
            if n == 0 {
                return Err(FcpRustError::Transport("unexpected EOF reading headers".to_string()));
            }
            line.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }

        let line_str = String::from_utf8_lossy(&line);
        let line_str = line_str.trim();

        if line_str.is_empty() {
            // Empty line = end of headers
            break;
        }

        if let Some(value) = line_str.strip_prefix("Content-Length:") {
            let value = value.trim();
            content_length = Some(
                value
                    .parse::<usize>()
                    .map_err(|e| FcpRustError::Transport(format!("invalid Content-Length: {}", e)))?,
            );
        }
    }

    let content_length = content_length
        .ok_or_else(|| FcpRustError::Transport("missing Content-Length header".to_string()))?;

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).await?;

    let value: serde_json::Value = serde_json::from_slice(&body)?;
    Ok(value)
}

/// Writer wrapper for sending LSP messages over an async writer.
pub struct LspWriter<W: AsyncWrite + Unpin> {
    writer: Arc<Mutex<W>>,
}

impl<W: AsyncWrite + Unpin> LspWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: Arc::new(Mutex::new(writer)),
        }
    }

    #[allow(dead_code)]
    pub fn from_shared(writer: Arc<Mutex<W>>) -> Self {
        Self { writer }
    }

    pub async fn send_request(
        &self,
        id: serde_json::Value,
        method: &str,
        params: serde_json::Value,
    ) -> Result<()> {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let body = serde_json::to_vec(&msg)?;
        let frame = encode_message(&body);
        let mut writer = self.writer.lock().await;
        writer.write_all(&frame).await?;
        writer.flush().await?;
        Ok(())
    }

    pub async fn send_notification(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<()> {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let body = serde_json::to_vec(&msg)?;
        let frame = encode_message(&body);
        let mut writer = self.writer.lock().await;
        writer.write_all(&frame).await?;
        writer.flush().await?;
        Ok(())
    }
}

impl<W: AsyncWrite + Unpin> Clone for LspWriter<W> {
    fn clone(&self) -> Self {
        Self {
            writer: Arc::clone(&self.writer),
        }
    }
}

/// Read loop that dispatches incoming messages to pending response handlers or the notification channel.
pub async fn read_loop<R: AsyncRead + Unpin + Send + 'static>(
    reader: R,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>>,
    notification_tx: mpsc::Sender<JsonRpcNotification>,
) {
    let mut buf_reader = BufReader::new(reader);

    loop {
        let msg = match decode_message(&mut buf_reader).await {
            Ok(msg) => msg,
            Err(_) => return, // EOF or read error — exit the loop
        };

        // Check if it's a response (has "id" field and no "method" field)
        if msg.get("id").is_some() && msg.get("method").is_none() {
            if let Ok(resp) = serde_json::from_value::<JsonRpcResponse>(msg) {
                let id_str = match &resp.id {
                    Some(serde_json::Value::Number(n)) => n.to_string(),
                    Some(serde_json::Value::String(s)) => s.clone(),
                    _ => continue,
                };

                let mut map = pending.lock().await;
                if let Some(sender) = map.remove(&id_str) {
                    let _ = sender.send(resp);
                }
            }
        } else if msg.get("method").is_some() && msg.get("id").is_none() {
            // Notification
            if let Ok(notif) = serde_json::from_value::<JsonRpcNotification>(msg) {
                let _ = notification_tx.send(notif).await;
            }
        }
        // Requests from server (have both id and method) are ignored for now
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::BufReader;

    #[test]
    fn test_encode_message() {
        let body = b"{\"jsonrpc\":\"2.0\"}";
        let encoded = encode_message(body);
        let expected = format!("Content-Length: {}\r\n\r\n{{\"jsonrpc\":\"2.0\"}}", body.len());
        assert_eq!(encoded, expected.as_bytes());
    }

    #[tokio::test]
    async fn test_decode_message() {
        let body = b"{\"jsonrpc\":\"2.0\",\"id\":1}";
        let frame = encode_message(body);
        let mut reader = BufReader::new(&frame[..]);
        let msg = decode_message(&mut reader).await.unwrap();
        assert_eq!(msg["jsonrpc"], "2.0");
        assert_eq!(msg["id"], 1);
    }

    #[tokio::test]
    async fn test_roundtrip() {
        let original = json!({"jsonrpc": "2.0", "id": 42, "method": "test", "params": null});
        let body = serde_json::to_vec(&original).unwrap();
        let frame = encode_message(&body);
        let mut reader = BufReader::new(&frame[..]);
        let decoded = decode_message(&mut reader).await.unwrap();
        assert_eq!(decoded, original);
    }

    #[tokio::test]
    async fn test_decode_eof() {
        let data: &[u8] = b"";
        let mut reader = BufReader::new(data);
        let result = decode_message(&mut reader).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_decode_invalid_header() {
        let data = b"Content-Length: abc\r\n\r\n";
        let mut reader = BufReader::new(&data[..]);
        let result = decode_message(&mut reader).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_decode_missing_content_length() {
        let data = b"Content-Type: application/json\r\n\r\n{}";
        let mut reader = BufReader::new(&data[..]);
        let result = decode_message(&mut reader).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_loop_dispatches_response() {
        let resp = json!({"jsonrpc": "2.0", "id": 1, "result": {"ok": true}});
        let body = serde_json::to_vec(&resp).unwrap();
        let frame = encode_message(&body);

        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert("1".to_string(), tx);

        let (notif_tx, _notif_rx) = mpsc::channel(16);

        let frame: &'static [u8] = Box::leak(frame.into_boxed_slice());
        tokio::spawn(read_loop(frame, Arc::clone(&pending), notif_tx));

        let result = rx.await.unwrap();
        assert_eq!(result.id, Some(json!(1)));
        assert!(result.result.is_some());
    }

    #[tokio::test]
    async fn test_read_loop_dispatches_notification() {
        let notif = json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": {"uri": "file:///test.rs"}});
        let body = serde_json::to_vec(&notif).unwrap();
        let frame = encode_message(&body);

        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (notif_tx, mut notif_rx) = mpsc::channel(16);

        let frame: &'static [u8] = Box::leak(frame.into_boxed_slice());
        tokio::spawn(read_loop(frame, Arc::clone(&pending), notif_tx));

        let received = notif_rx.recv().await.unwrap();
        assert_eq!(received.method, "textDocument/publishDiagnostics");
    }

    #[tokio::test]
    async fn test_read_loop_eof() {
        let data: &[u8] = b"";
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (notif_tx, _notif_rx) = mpsc::channel(16);

        // read_loop should return cleanly on EOF
        let handle = tokio::spawn(read_loop(data, Arc::clone(&pending), notif_tx));
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_send_request_format() {
        let (client, mut server) = tokio::io::duplex(4096);
        let writer = LspWriter::new(client);

        writer
            .send_request(json!(1), "textDocument/definition", json!({"key": "value"}))
            .await
            .unwrap();

        // Drop writer so server side sees EOF after the message
        drop(writer);

        let mut buf = Vec::new();
        server.read_to_end(&mut buf).await.unwrap();
        let buf_str = String::from_utf8(buf).unwrap();

        assert!(buf_str.starts_with("Content-Length:"));
        // Extract body
        let body_start = buf_str.find("\r\n\r\n").unwrap() + 4;
        let body: serde_json::Value = serde_json::from_str(&buf_str[body_start..]).unwrap();
        assert_eq!(body["jsonrpc"], "2.0");
        assert_eq!(body["id"], 1);
        assert_eq!(body["method"], "textDocument/definition");
    }

    #[tokio::test]
    async fn test_send_notification_format() {
        let (client, mut server) = tokio::io::duplex(4096);
        let writer = LspWriter::new(client);

        writer
            .send_notification("initialized", json!({}))
            .await
            .unwrap();

        drop(writer);

        let mut buf = Vec::new();
        server.read_to_end(&mut buf).await.unwrap();
        let buf_str = String::from_utf8(buf).unwrap();

        let body_start = buf_str.find("\r\n\r\n").unwrap() + 4;
        let body: serde_json::Value = serde_json::from_str(&buf_str[body_start..]).unwrap();
        assert_eq!(body["jsonrpc"], "2.0");
        assert_eq!(body["method"], "initialized");
        assert!(body.get("id").is_none());
    }

    #[tokio::test]
    async fn test_concurrent_requests() {
        let (client, mut server) = tokio::io::duplex(8192);
        let writer = LspWriter::new(client);
        let w1 = writer.clone();
        let w2 = writer.clone();

        let h1 = tokio::spawn(async move {
            w1.send_request(json!(1), "method/a", json!({})).await.unwrap();
        });
        let h2 = tokio::spawn(async move {
            w2.send_request(json!(2), "method/b", json!({})).await.unwrap();
        });

        h1.await.unwrap();
        h2.await.unwrap();
        drop(writer);

        let mut buf = Vec::new();
        server.read_to_end(&mut buf).await.unwrap();
        let buf_str = String::from_utf8(buf).unwrap();

        // Both messages should be present
        assert!(buf_str.contains("method/a"));
        assert!(buf_str.contains("method/b"));
    }
}
