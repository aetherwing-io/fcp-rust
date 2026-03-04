// Output formatting for query results

use crate::lsp::types::{
    CallHierarchyIncomingCall, CallHierarchyOutgoingCall, CodeAction, Diagnostic,
    DiagnosticSeverity, DocumentSymbol, Location, Range,
};
use crate::lsp::workspace_edit::ApplyResult;
use crate::resolver::index::SymbolEntry;

pub fn format_navigation_result(locations: &[Location], description: &str) -> String {
    if locations.is_empty() {
        return format!("No {} found.", description);
    }
    let mut lines = vec![format!("{} ({}):", description, locations.len())];
    for loc in locations {
        lines.push(format!(
            "  {} L{}:{}",
            short_uri(&loc.uri),
            loc.range.start.line + 1,
            loc.range.start.character + 1,
        ));
    }
    lines.join("\n")
}

pub fn format_definition(uri: &str, range: &Range, source_snippet: Option<&str>) -> String {
    let mut result = format!(
        "Definition: {} L{}:{}",
        short_uri(uri),
        range.start.line + 1,
        range.start.character + 1,
    );
    if let Some(snippet) = source_snippet {
        result.push_str(&format!("\n\n{}", snippet));
    }
    result
}

pub fn format_symbol_outline(file: &str, symbols: &[DocumentSymbol], indent: usize) -> String {
    let mut lines = Vec::new();
    if indent == 0 {
        lines.push(format!("Symbols in {}:", short_uri(file)));
    }
    let prefix = "  ".repeat(indent + 1);
    for sym in symbols {
        let kind_str = format!("{:?}", sym.kind).to_lowercase();
        lines.push(format!(
            "{}{} ({}) L{}",
            prefix,
            sym.name,
            kind_str,
            sym.range.start.line + 1,
        ));
        if let Some(ref children) = sym.children {
            lines.push(format_symbol_outline(file, children, indent + 1));
        }
    }
    lines.join("\n")
}

pub fn format_diagnostics(uri: &str, diagnostics: &[Diagnostic]) -> String {
    if diagnostics.is_empty() {
        return format!("{}: clean", short_uri(uri));
    }
    let mut lines = vec![format!("{} ({} issues):", short_uri(uri), diagnostics.len())];
    for d in diagnostics {
        let severity = match d.severity {
            Some(DiagnosticSeverity::Error) => "ERROR",
            Some(DiagnosticSeverity::Warning) => "WARN",
            Some(DiagnosticSeverity::Information) => "INFO",
            Some(DiagnosticSeverity::Hint) => "HINT",
            None => "???",
        };
        lines.push(format!(
            "  L{}: [{}] {}",
            d.range.start.line + 1,
            severity,
            summarize_diagnostic_message(&d.message),
        ));
    }
    lines.join("\n")
}

pub fn format_disambiguation(name: &str, entries: &[SymbolEntry]) -> String {
    let mut lines = vec![format!(
        "? Multiple matches for '{}'. Narrow with a selector:",
        name
    )];
    for (i, entry) in entries.iter().enumerate() {
        let container = entry
            .container_name
            .as_deref()
            .map(|c| format!(" in {}", c))
            .unwrap_or_default();
        lines.push(format!(
            "  {}. {} ({:?}){} — {}",
            i + 1,
            entry.name,
            entry.kind,
            container,
            short_uri(&entry.uri),
        ));
    }
    lines.join("\n")
}

pub fn format_hover(
    name: &str,
    kind: &str,
    uri: &str,
    range: &Range,
    contents: &str,
) -> String {
    let mut lines = vec![format!(
        "{} ({}) — {} L{}",
        name,
        kind,
        short_uri(uri),
        range.start.line + 1,
    )];
    if !contents.is_empty() {
        lines.push(String::new());
        lines.push(contents.to_string());
    }
    lines.join("\n")
}

pub fn format_callers(name: &str, calls: &[CallHierarchyIncomingCall]) -> String {
    if calls.is_empty() {
        return format!("No callers of '{}'.", name);
    }
    let mut lines = vec![format!("Callers of '{}' ({}):", name, calls.len())];
    for call in calls {
        lines.push(format!(
            "  {} ({:?}) — {} L{}",
            call.from.name,
            call.from.kind,
            short_uri(&call.from.uri),
            call.from.range.start.line + 1,
        ));
    }
    lines.join("\n")
}

pub fn format_callees(name: &str, calls: &[CallHierarchyOutgoingCall]) -> String {
    if calls.is_empty() {
        return format!("No callees of '{}'.", name);
    }
    let mut lines = vec![format!("Callees of '{}' ({}):", name, calls.len())];
    for call in calls {
        lines.push(format!(
            "  {} ({:?}) — {} L{}",
            call.to.name,
            call.to.kind,
            short_uri(&call.to.uri),
            call.to.range.start.line + 1,
        ));
    }
    lines.join("\n")
}

pub fn format_implementations(name: &str, locations: &[Location]) -> String {
    if locations.is_empty() {
        return format!("No implementations of '{}'.", name);
    }
    let mut lines = vec![format!(
        "Implementations of '{}' ({}):",
        name,
        locations.len()
    )];
    for loc in locations {
        lines.push(format!(
            "  {} L{}:{}",
            short_uri(&loc.uri),
            loc.range.start.line + 1,
            loc.range.start.character + 1,
        ));
    }
    lines.join("\n")
}

pub fn format_workspace_map(
    root_uri: &str,
    file_count: usize,
    symbol_count: usize,
    errors: usize,
    warnings: usize,
) -> String {
    let mut lines = vec![
        format!("Workspace: {}", short_uri(root_uri)),
        format!("  Files: {}", file_count),
        format!("  Symbols: {}", symbol_count),
    ];
    if errors > 0 || warnings > 0 {
        lines.push(format!(
            "  Diagnostics: {} errors, {} warnings",
            errors, warnings
        ));
    } else {
        lines.push("  Diagnostics: clean".to_string());
    }
    lines.join("\n")
}

pub fn format_unused(items: &[(&str, &Diagnostic)]) -> String {
    if items.is_empty() {
        return "No unused symbols found.".to_string();
    }
    let mut lines = vec![format!("Unused symbols ({}):", items.len())];
    for (uri, diag) in items {
        let classification = classify_unused(&diag.message);
        lines.push(format!(
            "  {} L{}: [{}] {}",
            short_uri(uri),
            diag.range.start.line + 1,
            classification,
            summarize_diagnostic_message(&diag.message),
        ));
    }
    lines.join("\n")
}

pub fn format_mutation_result(
    verb: &str,
    description: &str,
    result: &ApplyResult,
    root_uri: &str,
) -> String {
    let total = result.total_edits();
    let file_count = result.files_changed.len();
    let mut lines = vec![format!(
        "{}: {} ({} {}, {} {})",
        verb,
        description,
        file_count,
        if file_count == 1 { "file" } else { "files" },
        total,
        if total == 1 { "edit" } else { "edits" },
    )];
    for (uri, count) in &result.files_changed {
        lines.push(format!("  {}: {} {}", relative_path(uri, root_uri), count, if *count == 1 { "edit" } else { "edits" }));
    }
    for uri in &result.files_created {
        lines.push(format!("  {} (created)", relative_path(uri, root_uri)));
    }
    for (old, new) in &result.files_renamed {
        lines.push(format!(
            "  {} → {} (renamed)",
            relative_path(old, root_uri),
            relative_path(new, root_uri),
        ));
    }
    lines.join("\n")
}

pub fn format_code_action_choices(actions: &[CodeAction]) -> String {
    let mut lines = vec![format!(
        "? Multiple code actions available ({}):",
        actions.len()
    )];
    for (i, action) in actions.iter().enumerate() {
        let kind = action
            .kind
            .as_deref()
            .unwrap_or("unknown");
        let preferred = if action.is_preferred == Some(true) {
            " (preferred)"
        } else {
            ""
        };
        lines.push(format!("  {}. [{}] {}{}", i + 1, kind, action.title, preferred));
    }
    lines.join("\n")
}

fn classify_unused(message: &str) -> &str {
    let lower = message.to_lowercase();
    if lower.contains("dead_code") || lower.contains("never constructed") {
        "dead_code"
    } else if lower.contains("never read") {
        "never_read"
    } else {
        "unused"
    }
}

pub fn format_error(message: &str, suggestion: Option<&str>) -> String {
    match suggestion {
        Some(s) => format!("! {} Did you mean '{}'?", message, s),
        None => format!("! {}", message),
    }
}

pub fn summarize_diagnostic_message(raw: &str) -> String {
    // Strip Rust error codes like "E0308: " prefix
    if let Some(rest) = raw.strip_prefix("E") {
        if rest.len() >= 4 {
            let code = &rest[..4];
            if code.chars().all(|c| c.is_ascii_digit()) {
                let after_code = &rest[4..];
                if let Some(stripped) = after_code.strip_prefix(": ") {
                    return stripped.to_string();
                }
            }
        }
    }
    raw.to_string()
}

fn short_uri(uri: &str) -> &str {
    // Strip file:// prefix for display
    uri.strip_prefix("file://").unwrap_or(uri)
}

pub fn relative_path(uri: &str, root_uri: &str) -> String {
    let path = short_uri(uri);
    let root = short_uri(root_uri).trim_end_matches('/');
    path.strip_prefix(root)
        .map(|p| p.strip_prefix('/').unwrap_or(p))
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::types::{
        CallHierarchyItem, Position, SymbolKind,
    };

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    fn range(start_line: u32, end_line: u32) -> Range {
        Range {
            start: pos(start_line, 0),
            end: pos(end_line, 0),
        }
    }

    fn loc(uri: &str, start_line: u32) -> Location {
        Location {
            uri: uri.to_string(),
            range: range(start_line, start_line + 5),
        }
    }

    #[test]
    fn test_format_navigation_result_multiple() {
        let locs = vec![
            loc("file:///src/a.rs", 10),
            loc("file:///src/b.rs", 20),
            loc("file:///src/c.rs", 30),
        ];
        let result = format_navigation_result(&locs, "references to foo");
        assert!(result.contains("references to foo (3):"));
        assert!(result.contains("/src/a.rs L11:1"));
        assert!(result.contains("/src/b.rs L21:1"));
        assert!(result.contains("/src/c.rs L31:1"));
    }

    #[test]
    fn test_format_navigation_result_single() {
        let locs = vec![loc("file:///src/main.rs", 5)];
        let result = format_navigation_result(&locs, "definition");
        assert!(result.contains("definition (1):"));
        assert!(result.contains("/src/main.rs L6:1"));
    }

    #[test]
    fn test_format_navigation_result_empty() {
        let result = format_navigation_result(&[], "references");
        assert_eq!(result, "No references found.");
    }

    #[test]
    fn test_format_definition_with_snippet() {
        let result = format_definition(
            "file:///src/lib.rs",
            &range(10, 15),
            Some("pub fn add(a: i32, b: i32) -> i32 { a + b }"),
        );
        assert!(result.contains("Definition: /src/lib.rs L11:1"));
        assert!(result.contains("pub fn add"));
    }

    #[test]
    fn test_format_definition_without_snippet() {
        let result = format_definition("file:///src/main.rs", &range(0, 5), None);
        assert!(result.contains("Definition: /src/main.rs L1:1"));
        assert!(!result.contains("\n\n"));
    }

    #[test]
    fn test_format_symbol_outline_flat() {
        let symbols = vec![
            DocumentSymbol {
                name: "main".to_string(),
                kind: SymbolKind::Function,
                range: range(0, 5),
                selection_range: range(0, 0),
                children: None,
            },
            DocumentSymbol {
                name: "Config".to_string(),
                kind: SymbolKind::Struct,
                range: range(7, 12),
                selection_range: range(7, 7),
                children: None,
            },
            DocumentSymbol {
                name: "MAX".to_string(),
                kind: SymbolKind::Constant,
                range: range(14, 14),
                selection_range: range(14, 14),
                children: None,
            },
        ];
        let result = format_symbol_outline("file:///src/main.rs", &symbols, 0);
        assert!(result.contains("Symbols in /src/main.rs:"));
        assert!(result.contains("main (function) L1"));
        assert!(result.contains("Config (struct) L8"));
        assert!(result.contains("MAX (constant) L15"));
    }

    #[test]
    fn test_format_symbol_outline_nested() {
        let symbols = vec![DocumentSymbol {
            name: "Point".to_string(),
            kind: SymbolKind::Struct,
            range: range(0, 10),
            selection_range: range(0, 0),
            children: Some(vec![
                DocumentSymbol {
                    name: "x".to_string(),
                    kind: SymbolKind::Field,
                    range: range(1, 1),
                    selection_range: range(1, 1),
                    children: None,
                },
                DocumentSymbol {
                    name: "y".to_string(),
                    kind: SymbolKind::Field,
                    range: range(2, 2),
                    selection_range: range(2, 2),
                    children: None,
                },
            ]),
        }];
        let result = format_symbol_outline("file:///src/lib.rs", &symbols, 0);
        assert!(result.contains("Point (struct) L1"));
        assert!(result.contains("x (field) L2"));
        assert!(result.contains("y (field) L3"));
    }

    #[test]
    fn test_format_diagnostics_mixed() {
        let diags = vec![
            Diagnostic {
                range: range(5, 5),
                severity: Some(DiagnosticSeverity::Error),
                code: None,
                source: Some("rustc".to_string()),
                message: "type mismatch".to_string(),
            },
            Diagnostic {
                range: range(10, 10),
                severity: Some(DiagnosticSeverity::Error),
                code: None,
                source: Some("rustc".to_string()),
                message: "undefined variable".to_string(),
            },
            Diagnostic {
                range: range(15, 15),
                severity: Some(DiagnosticSeverity::Warning),
                code: None,
                source: Some("rustc".to_string()),
                message: "unused import".to_string(),
            },
        ];
        let result = format_diagnostics("file:///src/main.rs", &diags);
        assert!(result.contains("/src/main.rs (3 issues):"));
        assert!(result.contains("[ERROR] type mismatch"));
        assert!(result.contains("[ERROR] undefined variable"));
        assert!(result.contains("[WARN] unused import"));
    }

    #[test]
    fn test_format_diagnostics_clean() {
        let result = format_diagnostics("file:///src/main.rs", &[]);
        assert!(result.contains("clean"));
    }

    #[test]
    fn test_format_disambiguation() {
        let entries = vec![
            SymbolEntry {
                name: "new".to_string(),
                kind: SymbolKind::Function,
                container_name: Some("Vec".to_string()),
                uri: "file:///std/vec.rs".to_string(),
                range: range(0, 5),
                selection_range: range(0, 0),
            },
            SymbolEntry {
                name: "new".to_string(),
                kind: SymbolKind::Function,
                container_name: Some("HashMap".to_string()),
                uri: "file:///std/hashmap.rs".to_string(),
                range: range(0, 5),
                selection_range: range(0, 0),
            },
            SymbolEntry {
                name: "new".to_string(),
                kind: SymbolKind::Function,
                container_name: None,
                uri: "file:///src/lib.rs".to_string(),
                range: range(10, 15),
                selection_range: range(10, 10),
            },
        ];
        let result = format_disambiguation("new", &entries);
        assert!(result.contains("? Multiple matches for 'new'"));
        assert!(result.contains("1."));
        assert!(result.contains("2."));
        assert!(result.contains("3."));
        assert!(result.contains("in Vec"));
        assert!(result.contains("in HashMap"));
    }

    #[test]
    fn test_format_hover() {
        let result = format_hover(
            "add",
            "function",
            "file:///src/lib.rs",
            &range(10, 15),
            "pub fn add(a: i32, b: i32) -> i32",
        );
        assert!(result.contains("add (function)"));
        assert!(result.contains("/src/lib.rs L11"));
        assert!(result.contains("pub fn add"));
    }

    #[test]
    fn test_format_callers() {
        let calls = vec![
            CallHierarchyIncomingCall {
                from: CallHierarchyItem {
                    name: "main".to_string(),
                    kind: SymbolKind::Function,
                    uri: "file:///src/main.rs".to_string(),
                    range: range(0, 10),
                    selection_range: range(0, 0),
                },
                from_ranges: vec![range(5, 5)],
            },
            CallHierarchyIncomingCall {
                from: CallHierarchyItem {
                    name: "test_add".to_string(),
                    kind: SymbolKind::Function,
                    uri: "file:///tests/test.rs".to_string(),
                    range: range(20, 30),
                    selection_range: range(20, 20),
                },
                from_ranges: vec![range(25, 25)],
            },
        ];
        let result = format_callers("add", &calls);
        assert!(result.contains("Callers of 'add' (2):"));
        assert!(result.contains("main"));
        assert!(result.contains("test_add"));
    }

    #[test]
    fn test_format_callees() {
        let calls = vec![
            CallHierarchyOutgoingCall {
                to: CallHierarchyItem {
                    name: "validate".to_string(),
                    kind: SymbolKind::Function,
                    uri: "file:///src/lib.rs".to_string(),
                    range: range(50, 60),
                    selection_range: range(50, 50),
                },
                from_ranges: vec![range(5, 5)],
            },
            CallHierarchyOutgoingCall {
                to: CallHierarchyItem {
                    name: "process".to_string(),
                    kind: SymbolKind::Function,
                    uri: "file:///src/lib.rs".to_string(),
                    range: range(70, 80),
                    selection_range: range(70, 70),
                },
                from_ranges: vec![range(6, 6)],
            },
            CallHierarchyOutgoingCall {
                to: CallHierarchyItem {
                    name: "save".to_string(),
                    kind: SymbolKind::Function,
                    uri: "file:///src/db.rs".to_string(),
                    range: range(10, 20),
                    selection_range: range(10, 10),
                },
                from_ranges: vec![range(7, 7)],
            },
        ];
        let result = format_callees("handle_request", &calls);
        assert!(result.contains("Callees of 'handle_request' (3):"));
        assert!(result.contains("validate"));
        assert!(result.contains("process"));
        assert!(result.contains("save"));
    }

    #[test]
    fn test_format_implementations() {
        let locs = vec![
            loc("file:///src/echo.rs", 10),
            loc("file:///src/log.rs", 20),
        ];
        let result = format_implementations("Handler", &locs);
        assert!(result.contains("Implementations of 'Handler' (2):"));
        assert!(result.contains("/src/echo.rs"));
        assert!(result.contains("/src/log.rs"));
    }

    #[test]
    fn test_format_workspace_map() {
        let result =
            format_workspace_map("file:///projects/myapp", 15, 120, 2, 5);
        assert!(result.contains("Workspace: /projects/myapp"));
        assert!(result.contains("Files: 15"));
        assert!(result.contains("Symbols: 120"));
        assert!(result.contains("2 errors, 5 warnings"));
    }

    #[test]
    fn test_format_unused_empty() {
        let result = format_unused(&[]);
        assert_eq!(result, "No unused symbols found.");
    }

    #[test]
    fn test_format_unused_with_items() {
        let d1 = Diagnostic {
            range: range(5, 5),
            severity: Some(DiagnosticSeverity::Warning),
            code: None,
            source: Some("rustc".to_string()),
            message: "unused variable `x`".to_string(),
        };
        let d2 = Diagnostic {
            range: range(10, 10),
            severity: Some(DiagnosticSeverity::Warning),
            code: None,
            source: Some("rustc".to_string()),
            message: "value assigned to `y` is never read".to_string(),
        };
        let items: Vec<(&str, &Diagnostic)> = vec![
            ("file:///src/main.rs", &d1),
            ("file:///src/lib.rs", &d2),
        ];
        let result = format_unused(&items);
        assert!(result.contains("Unused symbols (2):"));
        assert!(result.contains("[unused]"));
        assert!(result.contains("[never_read]"));
    }

    #[test]
    fn test_format_mutation_result() {
        let result = ApplyResult {
            files_changed: vec![
                ("file:///projects/myapp/src/config.rs".to_string(), 4),
                ("file:///projects/myapp/src/main.rs".to_string(), 2),
                ("file:///projects/myapp/tests/test.rs".to_string(), 1),
            ],
            files_created: vec![],
            files_renamed: vec![],
        };
        let output = format_mutation_result(
            "rename",
            "Config → Settings",
            &result,
            "file:///projects/myapp",
        );
        assert!(output.contains("rename: Config → Settings (3 files, 7 edits)"));
        assert!(output.contains("src/config.rs: 4 edits"));
        assert!(output.contains("src/main.rs: 2 edits"));
        assert!(output.contains("tests/test.rs: 1 edit"));
    }

    #[test]
    fn test_format_mutation_result_empty() {
        let result = ApplyResult::default();
        let output = format_mutation_result("inline", "helper_fn", &result, "file:///proj");
        assert!(output.contains("inline: helper_fn (0 files, 0 edits)"));
    }

    #[test]
    fn test_format_code_action_choices() {
        let actions = vec![
            CodeAction {
                title: "Extract into function".to_string(),
                kind: Some("refactor.extract.function".to_string()),
                edit: None,
                is_preferred: Some(true),
            },
            CodeAction {
                title: "Extract into method".to_string(),
                kind: Some("refactor.extract.method".to_string()),
                edit: None,
                is_preferred: None,
            },
        ];
        let output = format_code_action_choices(&actions);
        assert!(output.contains("Multiple code actions available (2):"));
        assert!(output.contains("1. [refactor.extract.function] Extract into function (preferred)"));
        assert!(output.contains("2. [refactor.extract.method] Extract into method"));
    }

    #[test]
    fn test_format_error_with_suggestion() {
        let result = format_error("unknown verb 'fnd'.", Some("find"));
        assert!(result.contains("! unknown verb 'fnd'."));
        assert!(result.contains("Did you mean 'find'?"));
    }

    #[test]
    fn test_format_error_without_suggestion() {
        let result = format_error("no workspace open.", None);
        assert_eq!(result, "! no workspace open.");
    }

    #[test]
    fn test_summarize_diagnostic_message_rust_e0308() {
        let result = summarize_diagnostic_message("E0308: mismatched types");
        assert_eq!(result, "mismatched types");
    }

    #[test]
    fn test_summarize_diagnostic_message_plain() {
        let result = summarize_diagnostic_message("unused variable `x`");
        assert_eq!(result, "unused variable `x`");
    }

    #[test]
    fn test_relative_path() {
        let result = relative_path("file:///projects/myapp/src/main.rs", "file:///projects/myapp");
        assert_eq!(result, "src/main.rs");
    }

    #[test]
    fn test_relative_path_no_match() {
        let result = relative_path("file:///other/path.rs", "file:///projects/myapp");
        assert_eq!(result, "/other/path.rs");
    }
}
