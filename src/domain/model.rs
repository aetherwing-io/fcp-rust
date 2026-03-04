// Domain model for Rust workspace

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

use crate::lsp::client::LspClient;
use crate::lsp::lifecycle::ServerStatus;
use crate::lsp::types::{Diagnostic, DiagnosticSeverity};
use crate::resolver::index::SymbolIndex;

pub struct RustModel {
    pub root_uri: Url,
    pub lsp_client: Option<Arc<Mutex<LspClient>>>,
    pub symbol_index: SymbolIndex,
    pub diagnostics: HashMap<String, Vec<Diagnostic>>,
    /// uri → version counter for LSP document sync
    pub open_documents: HashMap<String, i32>,
    pub server_status: ServerStatus,
    pub rs_file_count: usize,
    pub last_reload: Option<std::time::SystemTime>,
}

impl RustModel {
    pub fn new(root_uri: Url) -> Self {
        Self {
            root_uri,
            lsp_client: None,
            symbol_index: SymbolIndex::new(),
            diagnostics: HashMap::new(),
            open_documents: HashMap::new(),
            server_status: ServerStatus::NotStarted,
            rs_file_count: 0,
            last_reload: None,
        }
    }

    pub fn update_diagnostics(&mut self, uri: &str, diagnostics: Vec<Diagnostic>) {
        if diagnostics.is_empty() {
            self.diagnostics.remove(uri);
        } else {
            self.diagnostics.insert(uri.to_string(), diagnostics);
        }
    }

    pub fn total_diagnostics(&self) -> (usize, usize) {
        let mut errors = 0;
        let mut warnings = 0;
        for diags in self.diagnostics.values() {
            for d in diags {
                match d.severity {
                    Some(DiagnosticSeverity::Error) => errors += 1,
                    Some(DiagnosticSeverity::Warning) => warnings += 1,
                    _ => {}
                }
            }
        }
        (errors, warnings)
    }

    pub fn diagnostic_count(&self) -> usize {
        self.diagnostics.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::types::{Position, Range};

    fn make_diag(severity: DiagnosticSeverity, message: &str) -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 10 },
            },
            severity: Some(severity),
            code: None,
            source: Some("rustc".to_string()),
            message: message.to_string(),
        }
    }

    #[test]
    fn test_new_model() {
        let model = RustModel::new(Url::parse("file:///project").unwrap());
        assert_eq!(model.root_uri.as_str(), "file:///project");
        assert!(model.lsp_client.is_none());
        assert_eq!(model.symbol_index.size(), 0);
        assert!(model.diagnostics.is_empty());
        assert!(model.open_documents.is_empty());
        assert_eq!(model.server_status, ServerStatus::NotStarted);
        assert_eq!(model.rs_file_count, 0);
        assert!(model.last_reload.is_none());
    }

    #[test]
    fn test_update_diagnostics() {
        let mut model = RustModel::new(Url::parse("file:///project").unwrap());
        let diags = vec![
            make_diag(DiagnosticSeverity::Error, "type mismatch"),
            make_diag(DiagnosticSeverity::Warning, "unused variable"),
        ];
        model.update_diagnostics("file:///main.rs", diags);
        assert_eq!(model.diagnostics.len(), 1);
        assert_eq!(model.diagnostics["file:///main.rs"].len(), 2);
    }

    #[test]
    fn test_update_diagnostics_empty_removes() {
        let mut model = RustModel::new(Url::parse("file:///project").unwrap());
        model.update_diagnostics(
            "file:///main.rs",
            vec![make_diag(DiagnosticSeverity::Error, "err")],
        );
        assert_eq!(model.diagnostics.len(), 1);
        model.update_diagnostics("file:///main.rs", vec![]);
        assert!(model.diagnostics.is_empty());
    }

    #[test]
    fn test_total_diagnostics() {
        let mut model = RustModel::new(Url::parse("file:///project").unwrap());
        model.update_diagnostics(
            "file:///a.rs",
            vec![
                make_diag(DiagnosticSeverity::Error, "e1"),
                make_diag(DiagnosticSeverity::Error, "e2"),
                make_diag(DiagnosticSeverity::Warning, "w1"),
            ],
        );
        model.update_diagnostics(
            "file:///b.rs",
            vec![make_diag(DiagnosticSeverity::Warning, "w2")],
        );
        let (errors, warnings) = model.total_diagnostics();
        assert_eq!(errors, 2);
        assert_eq!(warnings, 2);
    }

    #[test]
    fn test_diagnostic_count() {
        let mut model = RustModel::new(Url::parse("file:///project").unwrap());
        assert_eq!(model.diagnostic_count(), 0);
        model.update_diagnostics(
            "file:///a.rs",
            vec![
                make_diag(DiagnosticSeverity::Error, "e1"),
                make_diag(DiagnosticSeverity::Warning, "w1"),
            ],
        );
        model.update_diagnostics(
            "file:///b.rs",
            vec![make_diag(DiagnosticSeverity::Error, "e2")],
        );
        assert_eq!(model.diagnostic_count(), 3);
    }
}
