// Symbol resolution pipeline

use super::index::{SymbolEntry, SymbolIndex};
use super::selectors::ParsedSelector;
use crate::lsp::types::{Location, SymbolInformation};

#[derive(Debug)]
pub enum ResolveResult {
    Found(SymbolEntry),
    Ambiguous(Vec<SymbolEntry>),
    NotFound,
}

pub struct SymbolResolver<'a> {
    index: &'a SymbolIndex,
}

impl<'a> SymbolResolver<'a> {
    pub fn new(index: &'a SymbolIndex) -> Self {
        Self { index }
    }

    /// Tier 1: Resolve from the in-memory index.
    pub fn resolve_from_index(
        &self,
        name: &str,
        selectors: &[ParsedSelector],
    ) -> ResolveResult {
        let entries = self.index.lookup_by_name(name);

        if entries.is_empty() {
            return ResolveResult::NotFound;
        }

        // Apply selectors to filter
        let filtered: Vec<&SymbolEntry> = if selectors.is_empty() {
            entries
        } else {
            entries
                .into_iter()
                .filter(|entry| {
                    // Convert SymbolEntry to SymbolInformation for selector filtering
                    let sym_info = SymbolInformation {
                        name: entry.name.clone(),
                        kind: entry.kind,
                        location: Location {
                            uri: entry.uri.clone(),
                            range: entry.range.clone(),
                        },
                        container_name: entry.container_name.clone(),
                    };
                    super::selectors::filter_by_selectors(&[sym_info], selectors).len() == 1
                })
                .collect()
        };

        match filtered.len() {
            0 => ResolveResult::NotFound,
            1 => ResolveResult::Found(filtered[0].clone()),
            _ => ResolveResult::Ambiguous(filtered.into_iter().cloned().collect()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::types::{Position, Range, SymbolKind};
    use super::super::selectors::parse_selector;

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
    fn test_tier1_cache_hit() {
        let mut index = SymbolIndex::new();
        index.insert(make_entry("main", SymbolKind::Function, "file:///main.rs", None));

        let resolver = SymbolResolver::new(&index);
        match resolver.resolve_from_index("main", &[]) {
            ResolveResult::Found(entry) => assert_eq!(entry.name, "main"),
            _ => panic!("expected Found"),
        }
    }

    #[test]
    fn test_tier1_miss_returns_not_found() {
        let index = SymbolIndex::new();
        let resolver = SymbolResolver::new(&index);
        match resolver.resolve_from_index("nonexistent", &[]) {
            ResolveResult::NotFound => {}
            _ => panic!("expected NotFound"),
        }
    }

    #[test]
    fn test_tier1_ambiguous() {
        let mut index = SymbolIndex::new();
        index.insert(make_entry("new", SymbolKind::Function, "file:///a.rs", Some("A")));
        index.insert(make_entry("new", SymbolKind::Function, "file:///b.rs", Some("B")));

        let resolver = SymbolResolver::new(&index);
        match resolver.resolve_from_index("new", &[]) {
            ResolveResult::Ambiguous(entries) => assert_eq!(entries.len(), 2),
            _ => panic!("expected Ambiguous"),
        }
    }

    #[test]
    fn test_tier1_with_selectors() {
        let mut index = SymbolIndex::new();
        index.insert(make_entry("new", SymbolKind::Function, "file:///a.rs", Some("A")));
        index.insert(make_entry("new", SymbolKind::Function, "file:///b.rs", Some("B")));

        let resolver = SymbolResolver::new(&index);
        let selectors = vec![parse_selector("@file:a.rs").unwrap()];
        match resolver.resolve_from_index("new", &selectors) {
            ResolveResult::Found(entry) => {
                assert_eq!(entry.name, "new");
                assert!(entry.uri.contains("a.rs"));
            }
            _ => panic!("expected Found with selector filter"),
        }
    }
}
