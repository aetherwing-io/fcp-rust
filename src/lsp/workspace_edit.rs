// Apply WorkspaceEdit to disk — pure client-side logic, no LSP client dependency

use crate::error::Result;
use crate::lsp::types::{DocumentChange, Position, ResourceOperation, TextEdit, WorkspaceEdit};

/// Result of applying a WorkspaceEdit to the filesystem.
#[derive(Debug, Clone, Default)]
pub struct ApplyResult {
    /// (uri, edit_count) for each file with text edits
    pub files_changed: Vec<(String, usize)>,
    /// URIs of files created
    pub files_created: Vec<String>,
    /// (old_uri, new_uri) for renamed files
    pub files_renamed: Vec<(String, String)>,
}

impl ApplyResult {
    pub fn total_edits(&self) -> usize {
        self.files_changed.iter().map(|(_, n)| n).sum()
    }
}

/// Apply text edits to a string. Edits MUST be applied in reverse offset order
/// so earlier edits don't invalidate later positions.
pub fn apply_text_edits(content: &str, edits: &[TextEdit]) -> String {
    if edits.is_empty() {
        return content.to_string();
    }

    // Sort by start position descending (reverse order)
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by(|a, b| {
        let line_cmp = b.range.start.line.cmp(&a.range.start.line);
        if line_cmp != std::cmp::Ordering::Equal {
            line_cmp
        } else {
            b.range.start.character.cmp(&a.range.start.character)
        }
    });

    let mut result = content.to_string();
    for edit in sorted {
        let start = position_to_offset(&result, &edit.range.start);
        let end = position_to_offset(&result, &edit.range.end);
        if let (Some(start), Some(end)) = (start, end) {
            result.replace_range(start..end, &edit.new_text);
        }
    }
    result
}

/// Convert an LSP Position (line, character) to a byte offset in the content.
fn position_to_offset(content: &str, pos: &Position) -> Option<usize> {
    let mut offset = 0;
    for (i, line) in content.split('\n').enumerate() {
        if i == pos.line as usize {
            // character is UTF-16 offset, but for ASCII this is the same as byte offset
            let char_offset = pos.character as usize;
            // Clamp to line length
            let clamped = char_offset.min(line.len());
            return Some(offset + clamped);
        }
        offset += line.len() + 1; // +1 for the '\n'
    }
    // Position beyond end of file — return end
    Some(content.len())
}

/// Convert a file:// URI to a filesystem path.
fn uri_to_path(uri: &str) -> Option<std::path::PathBuf> {
    url::Url::parse(uri)
        .ok()
        .and_then(|u| u.to_file_path().ok())
}

/// Apply a WorkspaceEdit to the filesystem.
pub fn apply_workspace_edit(edit: &WorkspaceEdit) -> Result<ApplyResult> {
    let mut result = ApplyResult::default();

    if let Some(ref doc_changes) = edit.document_changes {
        // Rich form (preferred by rust-analyzer)
        for change in doc_changes {
            match change {
                DocumentChange::Edit(tde) => {
                    let path = uri_to_path(&tde.text_document.uri).ok_or_else(|| {
                        crate::error::FcpRustError::Parse(format!(
                            "invalid URI: {}",
                            tde.text_document.uri
                        ))
                    })?;
                    let content = std::fs::read_to_string(&path)?;
                    let new_content = apply_text_edits(&content, &tde.edits);
                    std::fs::write(&path, &new_content)?;
                    result
                        .files_changed
                        .push((tde.text_document.uri.clone(), tde.edits.len()));
                }
                DocumentChange::Operation(op) => match op {
                    ResourceOperation::Create { uri } => {
                        if let Some(path) = uri_to_path(uri) {
                            if let Some(parent) = path.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            std::fs::write(&path, "")?;
                        }
                        result.files_created.push(uri.clone());
                    }
                    ResourceOperation::Rename { old_uri, new_uri } => {
                        if let (Some(old_path), Some(new_path)) =
                            (uri_to_path(old_uri), uri_to_path(new_uri))
                        {
                            if let Some(parent) = new_path.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            std::fs::rename(&old_path, &new_path)?;
                        }
                        result
                            .files_renamed
                            .push((old_uri.clone(), new_uri.clone()));
                    }
                    ResourceOperation::Delete { uri } => {
                        if let Some(path) = uri_to_path(uri) {
                            if path.exists() {
                                std::fs::remove_file(&path)?;
                            }
                        }
                    }
                },
            }
        }
    } else if let Some(ref changes) = edit.changes {
        // Simple form: uri → edits
        for (uri, edits) in changes {
            let path = uri_to_path(uri).ok_or_else(|| {
                crate::error::FcpRustError::Parse(format!("invalid URI: {}", uri))
            })?;
            let content = std::fs::read_to_string(&path)?;
            let new_content = apply_text_edits(&content, edits);
            std::fs::write(&path, &new_content)?;
            result.files_changed.push((uri.clone(), edits.len()));
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::types::{
        OptionalVersionedTextDocumentIdentifier, Position, Range, TextDocumentEdit,
    };

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    fn range(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
        Range {
            start: pos(sl, sc),
            end: pos(el, ec),
        }
    }

    #[test]
    fn test_apply_text_edits_single() {
        let content = "fn config() {}";
        let edits = vec![TextEdit {
            range: range(0, 3, 0, 9),
            new_text: "settings".to_string(),
        }];
        let result = apply_text_edits(content, &edits);
        assert_eq!(result, "fn settings() {}");
    }

    #[test]
    fn test_apply_text_edits_multiple_non_overlapping() {
        let content = "let x = Config::new();\nlet y = Config::default();";
        let edits = vec![
            TextEdit {
                range: range(0, 8, 0, 14),
                new_text: "Settings".to_string(),
            },
            TextEdit {
                range: range(1, 8, 1, 14),
                new_text: "Settings".to_string(),
            },
        ];
        let result = apply_text_edits(content, &edits);
        assert_eq!(
            result,
            "let x = Settings::new();\nlet y = Settings::default();"
        );
    }

    #[test]
    fn test_apply_text_edits_empty() {
        let content = "hello world";
        let result = apply_text_edits(content, &[]);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_apply_text_edits_insert() {
        let content = "fn main() {}";
        let edits = vec![TextEdit {
            range: range(0, 0, 0, 0),
            new_text: "pub ".to_string(),
        }];
        let result = apply_text_edits(content, &edits);
        assert_eq!(result, "pub fn main() {}");
    }

    #[test]
    fn test_apply_text_edits_multiline() {
        let content = "line one\nline two\nline three";
        let edits = vec![TextEdit {
            range: range(1, 5, 1, 8),
            new_text: "2".to_string(),
        }];
        let result = apply_text_edits(content, &edits);
        assert_eq!(result, "line one\nline 2\nline three");
    }

    #[test]
    fn test_position_to_offset() {
        let content = "hello\nworld\nfoo";
        assert_eq!(position_to_offset(content, &pos(0, 0)), Some(0));
        assert_eq!(position_to_offset(content, &pos(0, 5)), Some(5));
        assert_eq!(position_to_offset(content, &pos(1, 0)), Some(6));
        assert_eq!(position_to_offset(content, &pos(1, 5)), Some(11));
        assert_eq!(position_to_offset(content, &pos(2, 0)), Some(12));
        assert_eq!(position_to_offset(content, &pos(2, 3)), Some(15));
    }

    #[test]
    fn test_apply_workspace_edit_with_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(&file_path, "fn Config() {}\nuse Config;").unwrap();

        let uri = url::Url::from_file_path(&file_path).unwrap().to_string();
        let edit = WorkspaceEdit {
            changes: None,
            document_changes: Some(vec![DocumentChange::Edit(TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: None,
                },
                edits: vec![
                    TextEdit {
                        range: range(0, 3, 0, 9),
                        new_text: "Settings".to_string(),
                    },
                    TextEdit {
                        range: range(1, 4, 1, 10),
                        new_text: "Settings".to_string(),
                    },
                ],
            })]),
        };

        let result = apply_workspace_edit(&edit).unwrap();
        assert_eq!(result.files_changed.len(), 1);
        assert_eq!(result.files_changed[0].1, 2);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "fn Settings() {}\nuse Settings;");
    }

    #[test]
    fn test_apply_workspace_edit_create_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("new_file.rs");
        let uri = url::Url::from_file_path(&file_path).unwrap().to_string();

        let edit = WorkspaceEdit {
            changes: None,
            document_changes: Some(vec![DocumentChange::Operation(
                ResourceOperation::Create { uri: uri.clone() },
            )]),
        };

        let result = apply_workspace_edit(&edit).unwrap();
        assert_eq!(result.files_created.len(), 1);
        assert!(file_path.exists());
    }

    #[test]
    fn test_apply_workspace_edit_rename_file() {
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("old.rs");
        let new_path = dir.path().join("new.rs");
        std::fs::write(&old_path, "content").unwrap();

        let old_uri = url::Url::from_file_path(&old_path).unwrap().to_string();
        let new_uri = url::Url::from_file_path(&new_path).unwrap().to_string();

        let edit = WorkspaceEdit {
            changes: None,
            document_changes: Some(vec![DocumentChange::Operation(
                ResourceOperation::Rename {
                    old_uri: old_uri.clone(),
                    new_uri: new_uri.clone(),
                },
            )]),
        };

        let result = apply_workspace_edit(&edit).unwrap();
        assert_eq!(result.files_renamed.len(), 1);
        assert!(!old_path.exists());
        assert!(new_path.exists());
        assert_eq!(std::fs::read_to_string(&new_path).unwrap(), "content");
    }

    #[test]
    fn test_apply_workspace_edit_simple_changes_form() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(&file_path, "fn old_name() {}").unwrap();

        let uri = url::Url::from_file_path(&file_path).unwrap().to_string();
        let mut changes = std::collections::HashMap::new();
        changes.insert(
            uri.clone(),
            vec![TextEdit {
                range: range(0, 3, 0, 11),
                new_text: "new_name".to_string(),
            }],
        );

        let edit = WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
        };

        let result = apply_workspace_edit(&edit).unwrap();
        assert_eq!(result.files_changed.len(), 1);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "fn new_name() {}");
    }
}
