// Query dispatcher and handlers

use std::sync::Arc;

use crate::fcpcore::formatter::suggest;
use crate::fcpcore::parsed_op::parse_op;
use crate::fcpcore::verb_registry::VerbRegistry;
use crate::lsp::types::*;
use crate::resolver::index::SymbolEntry;
use crate::resolver::pipeline::{ResolveResult, SymbolResolver};
use crate::resolver::selectors::{parse_selector, filter_by_selectors, ParsedSelector, SelectorType};

use super::format::*;
use super::model::RustModel;

/// Search a DocumentSymbol tree for a symbol matching by name + line range.
/// Propagates parent_name as container_name for field resolution.
fn find_in_doc_symbols(
    symbols: &[DocumentSymbol],
    name: &str,
    line: u32,
    parent_name: Option<&str>,
) -> Option<SymbolEntry> {
    for sym in symbols {
        if sym.name == name
            && sym.range.start.line <= line
            && line <= sym.range.end.line
        {
            return Some(SymbolEntry {
                name: sym.name.clone(),
                kind: sym.kind,
                container_name: parent_name.map(|s| s.to_string()),
                uri: String::new(), // caller fills in
                range: sym.range.clone(),
                selection_range: sym.selection_range.clone(),
            });
        }
        if let Some(ref children) = sym.children {
            if let Some(found) = find_in_doc_symbols(children, name, line, Some(&sym.name)) {
                return Some(found);
            }
        }
    }
    None
}

/// Search a DocumentSymbol tree by name only (for @struct: path resolution).
fn find_by_name_in_doc_symbols(
    symbols: &[DocumentSymbol],
    name: &str,
    parent_name: Option<&str>,
) -> Option<SymbolEntry> {
    for sym in symbols {
        if sym.name == name {
            return Some(SymbolEntry {
                name: sym.name.clone(),
                kind: sym.kind,
                container_name: parent_name.map(|s| s.to_string()),
                uri: String::new(), // caller fills in
                range: sym.range.clone(),
                selection_range: sym.selection_range.clone(),
            });
        }
        if let Some(ref children) = sym.children {
            if let Some(found) = find_by_name_in_doc_symbols(children, name, Some(&sym.name)) {
                return Some(found);
            }
        }
    }
    None
}

/// Try the in-memory index first; on miss, fall back to workspace/symbol LSP,
/// then to documentSymbol tree walk for struct fields.
pub(crate) async fn resolve_with_fallback(
    model: &RustModel,
    name: &str,
    selectors: &[ParsedSelector],
) -> ResolveResult {
    // Tier 1: in-memory index
    let resolver = SymbolResolver::new(&model.symbol_index);
    let result = resolver.resolve_from_index(name, selectors);

    if !matches!(result, ResolveResult::NotFound) {
        return result;
    }

    let client_arc = match model.lsp_client {
        Some(ref c) => Arc::clone(c),
        None => return result,
    };

    // Tier 2: workspace/symbol LSP
    let tier2_result = {
        let client = client_arc.lock().await;
        let symbols: Vec<SymbolInformation> = client
            .request("workspace/symbol", serde_json::json!({"query": name}))
            .await
            .unwrap_or_default();

        if symbols.is_empty() {
            ResolveResult::NotFound
        } else {
            let filtered = if selectors.is_empty() {
                symbols.iter().collect::<Vec<_>>()
            } else {
                filter_by_selectors(&symbols, selectors)
            };

            match filtered.len() {
                0 => ResolveResult::NotFound,
                1 => {
                    let sym = &filtered[0];
                    ResolveResult::Found(SymbolEntry {
                        name: sym.name.clone(),
                        kind: sym.kind,
                        container_name: sym.container_name.clone(),
                        uri: sym.location.uri.clone(),
                        range: sym.location.range.clone(),
                        selection_range: sym.location.range.clone(),
                    })
                }
                _ => {
                    let entries: Vec<SymbolEntry> = filtered
                        .iter()
                        .map(|sym| SymbolEntry {
                            name: sym.name.clone(),
                            kind: sym.kind,
                            container_name: sym.container_name.clone(),
                            uri: sym.location.uri.clone(),
                            range: sym.location.range.clone(),
                            selection_range: sym.location.range.clone(),
                        })
                        .collect();
                    ResolveResult::Ambiguous(entries)
                }
            }
        }
    }; // client lock dropped

    if !matches!(tier2_result, ResolveResult::NotFound) {
        return tier2_result;
    }

    // Tier 3: documentSymbol fallback for struct fields
    // Extract file/line/struct selectors
    let file_sel = selectors.iter().find(|s| s.selector_type == SelectorType::File);
    let line_sel = selectors.iter().find(|s| s.selector_type == SelectorType::Line);
    let struct_sel = selectors.iter().find(|s| s.selector_type == SelectorType::Struct);

    // Tier 3a: @file + @line — request documentSymbol for the file, walk tree
    if let (Some(file), Some(line)) = (file_sel, line_sel) {
        if let Ok(line_num) = line.value.parse::<u32>() {
            let uri = if file.value.starts_with("file://") {
                file.value.clone()
            } else {
                format!("{}/{}", model.root_uri.as_str().trim_end_matches('/'), file.value)
            };
            let client = client_arc.lock().await;
            let params = serde_json::json!({"textDocument": {"uri": &uri}});
            if let Ok(symbols) = client
                .request::<Vec<DocumentSymbol>>("textDocument/documentSymbol", params)
                .await
            {
                // line selectors are 1-indexed from user, but LSP is 0-indexed
                let lsp_line = if line_num > 0 { line_num - 1 } else { line_num };
                if let Some(mut entry) = find_in_doc_symbols(&symbols, name, lsp_line, None) {
                    entry.uri = uri;
                    return ResolveResult::Found(entry);
                }
            }
        }
    }

    // Tier 3b: @struct:NAME — locate struct file via workspace/symbol, then documentSymbol
    if let Some(struct_sel) = struct_sel {
        let client = client_arc.lock().await;
        // Find the struct's file
        let struct_symbols: Vec<SymbolInformation> = client
            .request("workspace/symbol", serde_json::json!({"query": &struct_sel.value}))
            .await
            .unwrap_or_default();
        let struct_info = struct_symbols
            .iter()
            .find(|s| s.name == struct_sel.value && s.kind == SymbolKind::Struct);
        if let Some(struct_info) = struct_info {
            let uri = &struct_info.location.uri;
            let params = serde_json::json!({"textDocument": {"uri": uri}});
            if let Ok(doc_symbols) = client
                .request::<Vec<DocumentSymbol>>("textDocument/documentSymbol", params)
                .await
            {
                // Find the struct node, then search its children
                for sym in &doc_symbols {
                    if sym.name == struct_sel.value && sym.kind == SymbolKind::Struct {
                        if let Some(ref children) = sym.children {
                            if let Some(mut entry) = find_by_name_in_doc_symbols(children, name, Some(&sym.name)) {
                                entry.uri = uri.clone();
                                return ResolveResult::Found(entry);
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    ResolveResult::NotFound
}

/// Dispatch a query operation string to the appropriate handler.
pub async fn dispatch_query(
    model: &RustModel,
    registry: &VerbRegistry,
    input: &str,
) -> String {
    let op = match parse_op(input) {
        Ok(op) => op,
        Err(e) => return format_error(&format!("parse error: {}", e.error), None),
    };

    if registry.lookup(&op.verb).is_none() {
        let verb_names: Vec<&str> = registry.verbs().iter().map(|v| v.name.as_str()).collect();
        let suggestion = suggest(&op.verb, &verb_names);
        return format_error(
            &format!("unknown verb '{}'.", op.verb),
            suggestion.as_deref(),
        );
    }

    match op.verb.as_str() {
        "find" => handle_find(model, &op.positionals, &op.params).await,
        "def" => handle_def(model, &op.positionals, &op.selectors).await,
        "refs" => handle_refs(model, &op.positionals, &op.selectors).await,
        "symbols" => handle_symbols(model, &op.positionals).await,
        "diagnose" => handle_diagnose(model, &op.positionals),
        "inspect" => handle_inspect(model, &op.positionals, &op.selectors).await,
        "callers" => handle_callers(model, &op.positionals, &op.selectors).await,
        "callees" => handle_callees(model, &op.positionals, &op.selectors).await,
        "impl" => handle_impl(model, &op.positionals, &op.selectors).await,
        "map" => handle_map(model),
        "unused" => handle_unused(model, &op.selectors),
        _ => format_error(&format!("unhandled verb '{}'.", op.verb), None),
    }
}

async fn handle_find(
    model: &RustModel,
    positionals: &[String],
    params: &std::collections::HashMap<String, String>,
) -> String {
    let query = match positionals.first() {
        Some(q) => q.as_str(),
        None => return format_error("find requires a search query.", None),
    };

    let kind_filter = params.get("kind").map(|s| s.as_str());

    // Search the symbol index
    let entries = model.symbol_index.lookup_by_name(query);

    // Apply kind filter
    let filtered: Vec<&SymbolEntry> = if let Some(kind_str) = kind_filter {
        let target_kind = crate::resolver::selectors::symbol_kind_from_string(kind_str);
        match target_kind {
            Some(k) => entries.into_iter().filter(|e| e.kind == k).collect(),
            None => return format_error(&format!("unknown kind '{}'.", kind_str), None),
        }
    } else {
        entries
    };

    if filtered.is_empty() {
        // Try LSP workspace/symbol as fallback
        if let Some(ref client) = model.lsp_client {
            let client = client.lock().await;
            if let Ok(symbols) = client
                .request::<Vec<SymbolInformation>>(
                    "workspace/symbol",
                    serde_json::json!({"query": query}),
                )
                .await
            {
                let locs: Vec<Location> = symbols.iter().map(|s| s.location.clone()).collect();
                return format_navigation_result(&locs, &format!("matches for '{}'", query));
            }
        }
        return format_error(&format!("no symbols matching '{}'.", query), None);
    }

    let locs: Vec<Location> = filtered
        .iter()
        .map(|e| Location {
            uri: e.uri.clone(),
            range: e.range.clone(),
        })
        .collect();
    format_navigation_result(&locs, &format!("matches for '{}'", query))
}

async fn handle_def(
    model: &RustModel,
    positionals: &[String],
    selectors: &[String],
) -> String {
    let name = match positionals.first() {
        Some(n) => n.as_str(),
        None => return format_error("def requires a symbol name.", None),
    };

    let parsed_selectors: Vec<_> = selectors.iter().filter_map(|s| parse_selector(s)).collect();
    match resolve_with_fallback(model, name, &parsed_selectors).await {
        ResolveResult::Found(entry) => {
            format_definition(&entry.uri, &entry.range, None)
        }
        ResolveResult::Ambiguous(entries) => format_disambiguation(name, &entries),
        ResolveResult::NotFound => {
            format_error(&format!("symbol '{}' not found.", name), None)
        }
    }
}

async fn handle_refs(
    model: &RustModel,
    positionals: &[String],
    selectors: &[String],
) -> String {
    let name = match positionals.first() {
        Some(n) => n.as_str(),
        None => return format_error("refs requires a symbol name.", None),
    };

    let parsed_selectors: Vec<_> = selectors.iter().filter_map(|s| parse_selector(s)).collect();
    let entry = match resolve_with_fallback(model, name, &parsed_selectors).await {
        ResolveResult::Found(e) => e,
        ResolveResult::Ambiguous(entries) => return format_disambiguation(name, &entries),
        ResolveResult::NotFound => {
            return format_error(&format!("symbol '{}' not found.", name), None);
        }
    };

    let client = match &model.lsp_client {
        Some(c) => c,
        None => return format_error("no workspace open.", None),
    };
    let client = client.lock().await;

    let params = serde_json::json!({
        "textDocument": {"uri": entry.uri},
        "position": {"line": entry.range.start.line, "character": entry.range.start.character},
        "context": {"includeDeclaration": true}
    });

    match client.request::<Vec<Location>>("textDocument/references", params).await {
        Ok(locations) => {
            format_navigation_result(&locations, &format!("references to '{}'", name))
        }
        Err(e) => format_error(&format!("LSP error: {}", e), None),
    }
}

async fn handle_symbols(
    model: &RustModel,
    positionals: &[String],
) -> String {
    let path = match positionals.first() {
        Some(p) => p.as_str(),
        None => return format_error("symbols requires a file path.", None),
    };

    let client = match &model.lsp_client {
        Some(c) => c,
        None => return format_error("no workspace open.", None),
    };
    let client = client.lock().await;

    // Convert path to URI if needed
    let uri = if path.starts_with("file://") {
        path.to_string()
    } else {
        format!("{}/{}", model.root_uri.as_str().trim_end_matches('/'), path)
    };

    // Nudge rust-analyzer to load this file's symbol table. `documentSymbol`
    // returns null (not an empty array) when the file hasn't been opened
    // in LSP yet — that null then fails deserialization into `Vec<..>`.
    // did_open is idempotent in rust-analyzer, so re-opening is safe.
    if let Ok(file_path) = url::Url::parse(&uri).and_then(|u| u.to_file_path().map_err(|_| url::ParseError::RelativeUrlWithoutBase)) {
        if let Ok(text) = std::fs::read_to_string(&file_path) {
            let _ = client.did_open(&uri, &text).await;
        }
    }

    let params = serde_json::json!({
        "textDocument": {"uri": uri}
    });

    // rust-analyzer may return null for `documentSymbol` when its per-file
    // symbol table isn't populated yet (common right after a cold start
    // even though `Status: ready`). We deserialize into `Option<Vec<..>>`
    // so null → None → empty list, instead of the "invalid type: null,
    // expected a sequence" deserialization failure. Same for the
    // SymbolInformation fallback.
    match client
        .request::<Option<Vec<DocumentSymbol>>>("textDocument/documentSymbol", params.clone())
        .await
    {
        Ok(symbols) => format_symbol_outline(&uri, &symbols.unwrap_or_default(), 0),
        Err(_) => {
            match client
                .request::<Option<Vec<SymbolInformation>>>("textDocument/documentSymbol", params)
                .await
            {
                Ok(symbols) => {
                    let doc_symbols: Vec<DocumentSymbol> = symbols
                        .unwrap_or_default()
                        .iter()
                        .map(|s| DocumentSymbol {
                            name: s.name.clone(),
                            kind: s.kind,
                            range: s.location.range.clone(),
                            selection_range: s.location.range.clone(),
                            children: None,
                        })
                        .collect();
                    format_symbol_outline(&uri, &doc_symbols, 0)
                }
                Err(e) => format_error(&format!("LSP error: {}", e), None),
            }
        }
    }
}

fn handle_diagnose(model: &RustModel, positionals: &[String]) -> String {
    if let Some(path) = positionals.first() {
        // Find diagnostics for this file
        let uri = if path.starts_with("file://") {
            path.to_string()
        } else {
            format!("{}/{}", model.root_uri.as_str().trim_end_matches('/'), path)
        };

        // Search for matching file in diagnostics
        for (diag_uri, diags) in &model.diagnostics {
            if diag_uri == &uri || diag_uri.ends_with(path.as_str()) {
                return format_diagnostics(diag_uri, diags);
            }
        }
        format_diagnostics(&uri, &[])
    } else {
        // All diagnostics
        if model.diagnostics.is_empty() {
            return "Workspace: clean — no diagnostics.".to_string();
        }
        let mut lines = Vec::new();
        let (errors, warnings) = model.total_diagnostics();
        lines.push(format!(
            "Workspace diagnostics: {} errors, {} warnings",
            errors, warnings
        ));
        for (uri, diags) in &model.diagnostics {
            lines.push(format_diagnostics(uri, diags));
        }
        lines.join("\n\n")
    }
}

async fn handle_inspect(
    model: &RustModel,
    positionals: &[String],
    selectors: &[String],
) -> String {
    let name = match positionals.first() {
        Some(n) => n.as_str(),
        None => return format_error("inspect requires a symbol name.", None),
    };

    let parsed_selectors: Vec<_> = selectors.iter().filter_map(|s| parse_selector(s)).collect();
    let entry = match resolve_with_fallback(model, name, &parsed_selectors).await {
        ResolveResult::Found(e) => e,
        ResolveResult::Ambiguous(entries) => return format_disambiguation(name, &entries),
        ResolveResult::NotFound => {
            return format_error(&format!("symbol '{}' not found.", name), None);
        }
    };

    let client = match &model.lsp_client {
        Some(c) => c,
        None => {
            // Return what we know from the index
            let kind_str = format!("{:?}", entry.kind).to_lowercase();
            return format_hover(name, &kind_str, &entry.uri, &entry.range, "");
        }
    };
    let client = client.lock().await;

    let params = serde_json::json!({
        "textDocument": {"uri": entry.uri},
        "position": {"line": entry.selection_range.start.line, "character": entry.selection_range.start.character}
    });

    match client.request::<Hover>("textDocument/hover", params).await {
        Ok(hover) => {
            let contents = match &hover.contents {
                HoverContents::MarkedString(s) => s.clone(),
                HoverContents::MarkupContent(mc) => mc.value.clone(),
                HoverContents::MarkedStringArray(arr) => arr.join("\n"),
            };
            let kind_str = format!("{:?}", entry.kind).to_lowercase();
            format_hover(name, &kind_str, &entry.uri, &entry.range, &contents)
        }
        Err(e) => format_error(&format!("LSP error: {}", e), None),
    }
}

async fn handle_callers(
    model: &RustModel,
    positionals: &[String],
    selectors: &[String],
) -> String {
    let name = match positionals.first() {
        Some(n) => n.as_str(),
        None => return format_error("callers requires a symbol name.", None),
    };

    let parsed_selectors: Vec<_> = selectors.iter().filter_map(|s| parse_selector(s)).collect();
    let entry = match resolve_with_fallback(model, name, &parsed_selectors).await {
        ResolveResult::Found(e) => e,
        ResolveResult::Ambiguous(entries) => return format_disambiguation(name, &entries),
        ResolveResult::NotFound => {
            return format_error(&format!("symbol '{}' not found.", name), None);
        }
    };

    let client = match &model.lsp_client {
        Some(c) => c,
        None => return format_error("no workspace open.", None),
    };
    let client = client.lock().await;

    // Prepare call hierarchy
    let prepare_params = serde_json::json!({
        "textDocument": {"uri": entry.uri},
        "position": {"line": entry.selection_range.start.line, "character": entry.selection_range.start.character}
    });

    let items: Vec<CallHierarchyItem> = match client
        .request("textDocument/prepareCallHierarchy", prepare_params)
        .await
    {
        Ok(items) => items,
        Err(e) => return format_error(&format!("LSP error: {}", e), None),
    };

    if items.is_empty() {
        return format_callers(name, &[]);
    }

    let incoming_params = serde_json::json!({"item": serde_json::to_value(&items[0]).unwrap()});
    match client
        .request::<Vec<CallHierarchyIncomingCall>>("callHierarchy/incomingCalls", incoming_params)
        .await
    {
        Ok(calls) => format_callers(name, &calls),
        Err(e) => format_error(&format!("LSP error: {}", e), None),
    }
}

async fn handle_callees(
    model: &RustModel,
    positionals: &[String],
    selectors: &[String],
) -> String {
    let name = match positionals.first() {
        Some(n) => n.as_str(),
        None => return format_error("callees requires a symbol name.", None),
    };

    let parsed_selectors: Vec<_> = selectors.iter().filter_map(|s| parse_selector(s)).collect();
    let entry = match resolve_with_fallback(model, name, &parsed_selectors).await {
        ResolveResult::Found(e) => e,
        ResolveResult::Ambiguous(entries) => return format_disambiguation(name, &entries),
        ResolveResult::NotFound => {
            return format_error(&format!("symbol '{}' not found.", name), None);
        }
    };

    let client = match &model.lsp_client {
        Some(c) => c,
        None => return format_error("no workspace open.", None),
    };
    let client = client.lock().await;

    let prepare_params = serde_json::json!({
        "textDocument": {"uri": entry.uri},
        "position": {"line": entry.selection_range.start.line, "character": entry.selection_range.start.character}
    });

    let items: Vec<CallHierarchyItem> = match client
        .request("textDocument/prepareCallHierarchy", prepare_params)
        .await
    {
        Ok(items) => items,
        Err(e) => return format_error(&format!("LSP error: {}", e), None),
    };

    if items.is_empty() {
        return format_callees(name, &[]);
    }

    let outgoing_params = serde_json::json!({"item": serde_json::to_value(&items[0]).unwrap()});
    match client
        .request::<Vec<CallHierarchyOutgoingCall>>("callHierarchy/outgoingCalls", outgoing_params)
        .await
    {
        Ok(calls) => format_callees(name, &calls),
        Err(e) => format_error(&format!("LSP error: {}", e), None),
    }
}

async fn handle_impl(
    model: &RustModel,
    positionals: &[String],
    selectors: &[String],
) -> String {
    let name = match positionals.first() {
        Some(n) => n.as_str(),
        None => return format_error("impl requires a symbol name.", None),
    };

    let parsed_selectors: Vec<_> = selectors.iter().filter_map(|s| parse_selector(s)).collect();
    let entry = match resolve_with_fallback(model, name, &parsed_selectors).await {
        ResolveResult::Found(e) => e,
        ResolveResult::Ambiguous(entries) => return format_disambiguation(name, &entries),
        ResolveResult::NotFound => {
            return format_error(&format!("symbol '{}' not found.", name), None);
        }
    };

    let client = match &model.lsp_client {
        Some(c) => c,
        None => return format_error("no workspace open.", None),
    };
    let client = client.lock().await;

    let params = serde_json::json!({
        "textDocument": {"uri": entry.uri},
        "position": {"line": entry.selection_range.start.line, "character": entry.selection_range.start.character}
    });

    match client
        .request::<Vec<Location>>("textDocument/implementation", params)
        .await
    {
        Ok(locations) => format_implementations(name, &locations),
        Err(e) => format_error(&format!("LSP error: {}", e), None),
    }
}

fn handle_map(model: &RustModel) -> String {
    let (errors, warnings) = model.total_diagnostics();
    format_workspace_map(
        model.root_uri.as_str(),
        model.rs_file_count,
        model.symbol_index.size(),
        errors,
        warnings,
    )
}

fn handle_unused(model: &RustModel, selectors: &[String]) -> String {
    let parsed_selectors: Vec<_> = selectors.iter().filter_map(|s| parse_selector(s)).collect();
    let file_filter = parsed_selectors
        .iter()
        .find(|s| s.selector_type == SelectorType::File)
        .map(|s| s.value.as_str());

    let unused_patterns = [
        "unused", "never read", "never constructed", "never used", "dead_code",
    ];

    let mut items: Vec<(&str, &Diagnostic)> = Vec::new();
    for (uri, diags) in &model.diagnostics {
        // Apply file filter
        if let Some(filter) = file_filter {
            if !uri.contains(filter) {
                continue;
            }
        }
        for diag in diags {
            let msg_lower = diag.message.to_lowercase();
            if unused_patterns.iter().any(|p| msg_lower.contains(p)) {
                items.push((uri.as_str(), diag));
            }
        }
    }

    // Sort by file then line for stable output
    items.sort_by(|a, b| {
        a.0.cmp(b.0)
            .then(a.1.range.start.line.cmp(&b.1.range.start.line))
    });

    format_unused(&items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fcpcore::verb_registry::VerbRegistry;
    use crate::lsp::types::{Position, Range, SymbolKind};
    use crate::resolver::index::SymbolEntry;
    use crate::domain::verbs::{register_query_verbs, register_session_verbs};
    use url::Url;

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    fn range(start_line: u32, end_line: u32) -> Range {
        Range {
            start: pos(start_line, 0),
            end: pos(end_line, 0),
        }
    }

    fn make_entry(
        name: &str,
        kind: SymbolKind,
        uri: &str,
        container: Option<&str>,
    ) -> SymbolEntry {
        SymbolEntry {
            name: name.to_string(),
            kind,
            container_name: container.map(|s| s.to_string()),
            uri: uri.to_string(),
            range: range(0, 5),
            selection_range: range(0, 0),
        }
    }

    fn make_model_with_index(entries: Vec<SymbolEntry>) -> RustModel {
        let mut model = RustModel::new(Url::parse("file:///project").unwrap());
        for entry in entries {
            model.symbol_index.insert(entry);
        }
        model
    }

    fn make_registry() -> VerbRegistry {
        let mut reg = VerbRegistry::new();
        register_query_verbs(&mut reg);
        register_session_verbs(&mut reg);
        reg
    }

    // find tests

    #[tokio::test]
    async fn test_handle_find_with_results() {
        let model = make_model_with_index(vec![
            make_entry("Config", SymbolKind::Struct, "file:///src/config.rs", None),
            make_entry("Config", SymbolKind::Struct, "file:///src/other.rs", None),
        ]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "find Config").await;
        assert!(result.contains("matches for 'Config' (2):"));
        assert!(result.contains("config.rs"));
        assert!(result.contains("other.rs"));
    }

    #[tokio::test]
    async fn test_handle_find_no_results() {
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "find NonExistent").await;
        assert!(result.contains("no symbols matching 'NonExistent'"));
    }

    #[tokio::test]
    async fn test_handle_find_with_kind_filter() {
        let model = make_model_with_index(vec![
            make_entry("process", SymbolKind::Function, "file:///src/lib.rs", None),
            make_entry("process", SymbolKind::Module, "file:///src/process.rs", None),
        ]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "find process kind:function").await;
        assert!(result.contains("(1):"));
        assert!(result.contains("lib.rs"));
    }

    // def tests

    #[tokio::test]
    async fn test_handle_def_found() {
        let model = make_model_with_index(vec![make_entry(
            "main",
            SymbolKind::Function,
            "file:///src/main.rs",
            None,
        )]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "def main").await;
        assert!(result.contains("Definition:"));
        assert!(result.contains("main.rs"));
    }

    #[tokio::test]
    async fn test_handle_def_not_found() {
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "def nonexistent").await;
        assert!(result.contains("not found"));
    }

    #[tokio::test]
    async fn test_handle_def_ambiguous() {
        let model = make_model_with_index(vec![
            make_entry("new", SymbolKind::Function, "file:///a.rs", Some("A")),
            make_entry("new", SymbolKind::Function, "file:///b.rs", Some("B")),
        ]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "def new").await;
        assert!(result.contains("Multiple matches"));
        assert!(result.contains("in A"));
        assert!(result.contains("in B"));
    }

    // refs tests

    #[tokio::test]
    async fn test_handle_refs_not_found() {
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "refs unknown").await;
        assert!(result.contains("not found"));
    }

    #[tokio::test]
    async fn test_handle_refs_no_client() {
        let model = make_model_with_index(vec![make_entry(
            "foo",
            SymbolKind::Function,
            "file:///lib.rs",
            None,
        )]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "refs foo").await;
        assert!(result.contains("no workspace open"));
    }

    // symbols tests

    #[tokio::test]
    async fn test_handle_symbols_no_client() {
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "symbols src/main.rs").await;
        assert!(result.contains("no workspace open"));
    }

    // diagnose tests

    #[tokio::test]
    async fn test_handle_diagnose_file() {
        let mut model = make_model_with_index(vec![]);
        model.update_diagnostics(
            "file:///project/src/main.rs",
            vec![Diagnostic {
                range: range(5, 5),
                severity: Some(DiagnosticSeverity::Error),
                code: None,
                source: Some("rustc".to_string()),
                message: "type mismatch".to_string(),
            }],
        );
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "diagnose src/main.rs").await;
        assert!(result.contains("1 issues"));
        assert!(result.contains("type mismatch"));
    }

    #[tokio::test]
    async fn test_handle_diagnose_workspace() {
        let mut model = make_model_with_index(vec![]);
        model.update_diagnostics(
            "file:///project/src/a.rs",
            vec![
                Diagnostic {
                    range: range(1, 1),
                    severity: Some(DiagnosticSeverity::Error),
                    code: None,
                    source: None,
                    message: "err".to_string(),
                },
            ],
        );
        model.update_diagnostics(
            "file:///project/src/b.rs",
            vec![
                Diagnostic {
                    range: range(2, 2),
                    severity: Some(DiagnosticSeverity::Warning),
                    code: None,
                    source: None,
                    message: "warn".to_string(),
                },
            ],
        );
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "diagnose").await;
        assert!(result.contains("1 errors, 1 warnings"));
    }

    #[tokio::test]
    async fn test_handle_diagnose_clean() {
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "diagnose").await;
        assert!(result.contains("clean"));
    }

    // inspect tests

    #[tokio::test]
    async fn test_handle_inspect_no_client_uses_index() {
        let model = make_model_with_index(vec![make_entry(
            "Config",
            SymbolKind::Struct,
            "file:///src/config.rs",
            None,
        )]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "inspect Config").await;
        assert!(result.contains("Config (struct)"));
        assert!(result.contains("config.rs"));
    }

    #[tokio::test]
    async fn test_handle_inspect_not_found() {
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "inspect NonExistent").await;
        assert!(result.contains("not found"));
    }

    // callers/callees tests

    #[tokio::test]
    async fn test_handle_callers_not_found() {
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "callers unknown").await;
        assert!(result.contains("not found"));
    }

    #[tokio::test]
    async fn test_handle_callers_no_client() {
        let model = make_model_with_index(vec![make_entry(
            "foo",
            SymbolKind::Function,
            "file:///lib.rs",
            None,
        )]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "callers foo").await;
        assert!(result.contains("no workspace open"));
    }

    #[tokio::test]
    async fn test_handle_callees_not_found() {
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "callees unknown").await;
        assert!(result.contains("not found"));
    }

    // impl tests

    #[tokio::test]
    async fn test_handle_impl_not_found() {
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "impl NonExistent").await;
        assert!(result.contains("not found"));
    }

    #[tokio::test]
    async fn test_handle_impl_no_client() {
        let model = make_model_with_index(vec![make_entry(
            "Handler",
            SymbolKind::Interface,
            "file:///lib.rs",
            None,
        )]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "impl Handler").await;
        assert!(result.contains("no workspace open"));
    }

    // map tests

    #[tokio::test]
    async fn test_handle_map() {
        let mut model = make_model_with_index(vec![
            make_entry("main", SymbolKind::Function, "file:///src/main.rs", None),
            make_entry("Config", SymbolKind::Struct, "file:///src/config.rs", None),
        ]);
        model.rs_file_count = 5;
        model.update_diagnostics(
            "file:///src/main.rs",
            vec![Diagnostic {
                range: range(0, 0),
                severity: Some(DiagnosticSeverity::Warning),
                code: None,
                source: None,
                message: "unused".to_string(),
            }],
        );
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "map").await;
        assert!(result.contains("Workspace:"));
        assert!(result.contains("Files: 5"));
        assert!(result.contains("Symbols: 2"));
        assert!(result.contains("0 errors, 1 warnings"));
    }

    // dispatcher tests

    #[tokio::test]
    async fn test_dispatch_query_known_verb() {
        let model = make_model_with_index(vec![
            make_entry("Config", SymbolKind::Struct, "file:///src/config.rs", None),
        ]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "find Config").await;
        assert!(result.contains("Config"));
    }

    #[tokio::test]
    async fn test_dispatch_query_unknown_verb_with_suggestion() {
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "fnd Config").await;
        assert!(result.contains("unknown verb"));
        assert!(result.contains("Did you mean 'find'?"));
    }

    #[tokio::test]
    async fn test_dispatch_query_unknown_verb_no_suggestion() {
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "zzzzzzz Config").await;
        assert!(result.contains("unknown verb"));
    }

    #[tokio::test]
    async fn test_dispatch_query_empty_input() {
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "").await;
        assert!(result.contains("parse error"));
    }

    // unused tests

    #[tokio::test]
    async fn test_handle_unused_empty() {
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "unused").await;
        assert!(result.contains("No unused symbols found"));
    }

    #[tokio::test]
    async fn test_handle_unused_with_matches() {
        let mut model = make_model_with_index(vec![]);
        model.update_diagnostics(
            "file:///project/src/main.rs",
            vec![
                Diagnostic {
                    range: range(5, 5),
                    severity: Some(DiagnosticSeverity::Warning),
                    code: None,
                    source: Some("rustc".to_string()),
                    message: "unused variable `x`".to_string(),
                },
                Diagnostic {
                    range: range(10, 10),
                    severity: Some(DiagnosticSeverity::Error),
                    code: None,
                    source: Some("rustc".to_string()),
                    message: "type mismatch".to_string(),
                },
            ],
        );
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "unused").await;
        assert!(result.contains("Unused symbols (1):"));
        assert!(result.contains("unused variable"));
        assert!(!result.contains("type mismatch"));
    }

    #[tokio::test]
    async fn test_handle_unused_file_filter() {
        let mut model = make_model_with_index(vec![]);
        model.update_diagnostics(
            "file:///project/src/a.rs",
            vec![Diagnostic {
                range: range(1, 1),
                severity: Some(DiagnosticSeverity::Warning),
                code: None,
                source: None,
                message: "unused import".to_string(),
            }],
        );
        model.update_diagnostics(
            "file:///project/src/b.rs",
            vec![Diagnostic {
                range: range(2, 2),
                severity: Some(DiagnosticSeverity::Warning),
                code: None,
                source: None,
                message: "unused variable `y`".to_string(),
            }],
        );
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "unused @file:a.rs").await;
        assert!(result.contains("Unused symbols (1):"));
        assert!(result.contains("a.rs"));
        assert!(!result.contains("b.rs"));
    }

    // documentSymbol fallback tests

    #[tokio::test]
    async fn test_resolve_field_no_client_graceful() {
        // Without a client, resolve should return NotFound gracefully
        let model = make_model_with_index(vec![]);
        let reg = make_registry();
        let result = dispatch_query(&model, &reg, "inspect config @file:server.rs @line:71").await;
        assert!(result.contains("not found"));
    }

    #[test]
    fn test_find_in_doc_symbols_basic() {
        let symbols = vec![DocumentSymbol {
            name: "Config".to_string(),
            kind: SymbolKind::Struct,
            range: range(10, 20),
            selection_range: range(10, 10),
            children: Some(vec![DocumentSymbol {
                name: "port".to_string(),
                kind: SymbolKind::Field,
                range: range(12, 12),
                selection_range: range(12, 12),
                children: None,
            }]),
        }];
        // Line 12 (0-indexed) should find "port" inside "Config"
        let entry = find_in_doc_symbols(&symbols, "port", 12, None).unwrap();
        assert_eq!(entry.name, "port");
        assert_eq!(entry.container_name, Some("Config".to_string()));
        assert_eq!(entry.kind, SymbolKind::Field);
    }

    #[test]
    fn test_find_in_doc_symbols_not_found() {
        let symbols = vec![DocumentSymbol {
            name: "Config".to_string(),
            kind: SymbolKind::Struct,
            range: range(10, 20),
            selection_range: range(10, 10),
            children: None,
        }];
        assert!(find_in_doc_symbols(&symbols, "port", 12, None).is_none());
    }

    #[test]
    fn test_find_by_name_in_doc_symbols_basic() {
        let symbols = vec![
            DocumentSymbol {
                name: "host".to_string(),
                kind: SymbolKind::Field,
                range: range(11, 11),
                selection_range: range(11, 11),
                children: None,
            },
            DocumentSymbol {
                name: "port".to_string(),
                kind: SymbolKind::Field,
                range: range(12, 12),
                selection_range: range(12, 12),
                children: None,
            },
        ];
        let entry = find_by_name_in_doc_symbols(&symbols, "port", Some("Config")).unwrap();
        assert_eq!(entry.name, "port");
        assert_eq!(entry.container_name, Some("Config".to_string()));
    }
}
