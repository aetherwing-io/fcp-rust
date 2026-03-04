// LSP lifecycle manager with crash recovery

use std::collections::HashMap;
use std::time::Instant;

use crate::error::Result;
use super::client::LspClient;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum ServerStatus {
    NotStarted,
    Starting,
    Ready,
    Indexing,
    Crashed,
    Stopped,
}

#[allow(dead_code)]
pub struct LifecycleManager {
    command: String,
    args: Vec<String>,
    root_uri: String,
    client: Option<LspClient>,
    status: ServerStatus,
    restart_count: u32,
    max_restarts: u32,
    last_restart: Option<Instant>,
    tracked_documents: HashMap<String, String>,
}

#[allow(dead_code)]
impl LifecycleManager {
    pub fn new(command: String, args: Vec<String>, root_uri: String) -> Self {
        Self {
            command,
            args,
            root_uri,
            client: None,
            status: ServerStatus::NotStarted,
            restart_count: 0,
            max_restarts: 3,
            last_restart: None,
            tracked_documents: HashMap::new(),
        }
    }

    /// Ensure the LSP client is running. Starts or restarts if needed.
    pub async fn ensure_client(&mut self) -> Result<&mut LspClient> {
        if self.client.is_some() && self.status == ServerStatus::Ready {
            return Ok(self.client.as_mut().unwrap());
        }

        if self.status == ServerStatus::Crashed && self.restart_count >= self.max_restarts {
            return Err(crate::error::FcpRustError::LspProtocol(
                "max restarts exceeded".to_string(),
            ));
        }

        self.status = ServerStatus::Starting;

        let args_refs: Vec<&str> = self.args.iter().map(|s| s.as_str()).collect();
        match LspClient::spawn(&self.command, &args_refs, &self.root_uri).await {
            Ok(client) => {
                self.client = Some(client);
                self.status = ServerStatus::Ready;
                self.last_restart = Some(Instant::now());

                // Replay tracked documents
                let docs: Vec<(String, String)> =
                    self.tracked_documents.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                for (uri, text) in docs {
                    if let Some(ref client) = self.client {
                        let _ = client.did_open(&uri, &text).await;
                    }
                }

                Ok(self.client.as_mut().unwrap())
            }
            Err(e) => {
                self.status = ServerStatus::Crashed;
                self.restart_count += 1;
                Err(e)
            }
        }
    }

    /// Track a document for replay on restart.
    pub fn track_document(&mut self, uri: String, text: String) {
        self.tracked_documents.insert(uri, text);
    }

    /// Untrack a document.
    pub fn untrack_document(&mut self, uri: &str) {
        self.tracked_documents.remove(uri);
    }

    /// Shutdown the LSP server gracefully.
    pub async fn shutdown(&mut self) -> Result<()> {
        if let Some(ref mut client) = self.client {
            client.shutdown().await?;
        }
        self.client = None;
        self.status = ServerStatus::Stopped;
        Ok(())
    }

    /// Get the current server status.
    pub fn status(&self) -> &ServerStatus {
        &self.status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_status_not_started() {
        let mgr = LifecycleManager::new(
            "rust-analyzer".to_string(),
            vec![],
            "file:///test".to_string(),
        );
        assert_eq!(mgr.status(), &ServerStatus::NotStarted);
    }

    #[test]
    fn test_track_untrack_document() {
        let mut mgr = LifecycleManager::new(
            "rust-analyzer".to_string(),
            vec![],
            "file:///test".to_string(),
        );

        mgr.track_document("file:///main.rs".to_string(), "fn main() {}".to_string());
        assert_eq!(mgr.tracked_documents.len(), 1);
        assert_eq!(
            mgr.tracked_documents.get("file:///main.rs").unwrap(),
            "fn main() {}"
        );

        mgr.untrack_document("file:///main.rs");
        assert!(mgr.tracked_documents.is_empty());
    }

    #[test]
    fn test_untrack_nonexistent() {
        let mut mgr = LifecycleManager::new(
            "rust-analyzer".to_string(),
            vec![],
            "file:///test".to_string(),
        );
        // Should not panic
        mgr.untrack_document("file:///nonexistent.rs");
    }

    #[test]
    fn test_track_overwrites() {
        let mut mgr = LifecycleManager::new(
            "rust-analyzer".to_string(),
            vec![],
            "file:///test".to_string(),
        );
        mgr.track_document("file:///main.rs".to_string(), "v1".to_string());
        mgr.track_document("file:///main.rs".to_string(), "v2".to_string());
        assert_eq!(mgr.tracked_documents.len(), 1);
        assert_eq!(mgr.tracked_documents.get("file:///main.rs").unwrap(), "v2");
    }
}
