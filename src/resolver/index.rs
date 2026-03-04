// Symbol index for resolution pipeline

use std::collections::HashMap;
use crate::lsp::types::{SymbolKind, Range};

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolEntry {
    pub name: String,
    pub kind: SymbolKind,
    pub container_name: Option<String>,
    pub uri: String,
    pub range: Range,
    pub selection_range: Range,
}

#[derive(Debug, Default)]
pub struct SymbolIndex {
    by_name: HashMap<String, Vec<SymbolEntry>>,
    by_file: HashMap<String, Vec<SymbolEntry>>,
    by_container: HashMap<String, Vec<SymbolEntry>>,
}

impl SymbolIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, entry: SymbolEntry) {
        self.by_name
            .entry(entry.name.clone())
            .or_default()
            .push(entry.clone());

        self.by_file
            .entry(entry.uri.clone())
            .or_default()
            .push(entry.clone());

        if let Some(ref container) = entry.container_name {
            self.by_container
                .entry(container.clone())
                .or_default()
                .push(entry.clone());
        }
    }

    pub fn lookup_by_name(&self, name: &str) -> Vec<&SymbolEntry> {
        self.by_name
            .get(name)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn lookup_by_file(&self, uri: &str) -> Vec<&SymbolEntry> {
        self.by_file
            .get(uri)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn lookup_by_container(&self, container: &str) -> Vec<&SymbolEntry> {
        self.by_container
            .get(container)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn invalidate_file(&mut self, uri: &str) {
        // Remove from by_file
        self.by_file.remove(uri);

        // Remove matching entries from by_name
        for entries in self.by_name.values_mut() {
            entries.retain(|e| e.uri != uri);
        }
        self.by_name.retain(|_, v| !v.is_empty());

        // Remove matching entries from by_container
        for entries in self.by_container.values_mut() {
            entries.retain(|e| e.uri != uri);
        }
        self.by_container.retain(|_, v| !v.is_empty());
    }

    pub fn size(&self) -> usize {
        self.by_file.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::types::Position;

    fn make_range(line: u32) -> Range {
        Range {
            start: Position { line, character: 0 },
            end: Position { line, character: 10 },
        }
    }

    fn make_entry(name: &str, kind: SymbolKind, uri: &str, container: Option<&str>) -> SymbolEntry {
        SymbolEntry {
            name: name.to_string(),
            kind,
            container_name: container.map(|s| s.to_string()),
            uri: uri.to_string(),
            range: make_range(0),
            selection_range: make_range(0),
        }
    }

    #[test]
    fn test_insert_and_lookup_by_name() {
        let mut idx = SymbolIndex::new();
        idx.insert(make_entry("main", SymbolKind::Function, "file:///main.rs", None));
        let results = idx.lookup_by_name("main");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "main");
    }

    #[test]
    fn test_lookup_by_file() {
        let mut idx = SymbolIndex::new();
        idx.insert(make_entry("foo", SymbolKind::Function, "file:///lib.rs", None));
        idx.insert(make_entry("bar", SymbolKind::Function, "file:///lib.rs", None));
        idx.insert(make_entry("baz", SymbolKind::Function, "file:///main.rs", None));

        let results = idx.lookup_by_file("file:///lib.rs");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_lookup_by_container() {
        let mut idx = SymbolIndex::new();
        idx.insert(make_entry("method_a", SymbolKind::Method, "file:///lib.rs", Some("MyStruct")));
        idx.insert(make_entry("method_b", SymbolKind::Method, "file:///lib.rs", Some("MyStruct")));

        let results = idx.lookup_by_container("MyStruct");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_lookup_missing() {
        let idx = SymbolIndex::new();
        assert!(idx.lookup_by_name("nonexistent").is_empty());
        assert!(idx.lookup_by_file("file:///none.rs").is_empty());
        assert!(idx.lookup_by_container("Nothing").is_empty());
    }

    #[test]
    fn test_multiple_entries_same_name() {
        let mut idx = SymbolIndex::new();
        idx.insert(make_entry("new", SymbolKind::Function, "file:///a.rs", Some("A")));
        idx.insert(make_entry("new", SymbolKind::Function, "file:///b.rs", Some("B")));

        let results = idx.lookup_by_name("new");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_invalidate_file() {
        let mut idx = SymbolIndex::new();
        idx.insert(make_entry("foo", SymbolKind::Function, "file:///lib.rs", Some("Mod")));
        idx.insert(make_entry("bar", SymbolKind::Function, "file:///main.rs", None));

        idx.invalidate_file("file:///lib.rs");

        assert!(idx.lookup_by_file("file:///lib.rs").is_empty());
        assert!(idx.lookup_by_name("foo").is_empty());
        assert!(idx.lookup_by_container("Mod").is_empty());
        assert_eq!(idx.lookup_by_name("bar").len(), 1);
    }

    #[test]
    fn test_size() {
        let mut idx = SymbolIndex::new();
        assert_eq!(idx.size(), 0);
        idx.insert(make_entry("a", SymbolKind::Function, "file:///a.rs", None));
        idx.insert(make_entry("b", SymbolKind::Function, "file:///a.rs", None));
        idx.insert(make_entry("c", SymbolKind::Function, "file:///b.rs", None));
        assert_eq!(idx.size(), 3);
    }

    #[test]
    fn test_invalidate_then_size() {
        let mut idx = SymbolIndex::new();
        idx.insert(make_entry("a", SymbolKind::Function, "file:///a.rs", None));
        idx.insert(make_entry("b", SymbolKind::Function, "file:///b.rs", None));
        assert_eq!(idx.size(), 2);
        idx.invalidate_file("file:///a.rs");
        assert_eq!(idx.size(), 1);
    }

    #[test]
    fn test_invalidate_nonexistent_file() {
        let mut idx = SymbolIndex::new();
        idx.insert(make_entry("a", SymbolKind::Function, "file:///a.rs", None));
        idx.invalidate_file("file:///nonexistent.rs");
        assert_eq!(idx.size(), 1);
    }
}
