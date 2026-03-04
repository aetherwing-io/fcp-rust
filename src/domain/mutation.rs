// Mutation dispatcher and handlers

use std::sync::Arc;

use crate::fcpcore::formatter::suggest;
use crate::fcpcore::parsed_op::parse_op;
use crate::fcpcore::verb_registry::VerbRegistry;
use crate::lsp::types::*;
use crate::lsp::workspace_edit::{apply_workspace_edit, ApplyResult};
use crate::resolver::pipeline::ResolveResult;
use crate::resolver::selectors::{parse_line_range, parse_selector, SelectorType};

use super::format::*;
use super::model::RustModel;
use super::query::resolve_with_fallback;

/// Dispatch a mutation operation string to the appropriate handler.
pub async fn dispatch_mutation(
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

    // Require an active LSP session for all mutations
    if model.lsp_client.is_none() {
        return format_error("no workspace open. Use rust_session open PATH first.", None);
    }

    match op.verb.as_str() {
        "rename" => handle_rename(model, &op.positionals, &op.selectors).await,
        "extract" => handle_extract(model, &op.positionals, &op.selectors).await,
        "inline" => handle_inline(model, &op.positionals, &op.selectors).await,
        "generate" => handle_generate(model, &op.positionals, &op.selectors).await,
        "import" => handle_import(model, &op.positionals, &op.selectors).await,
        _ => format_error(&format!("verb '{}' is not a mutation.", op.verb), None),
    }
}

/// Open a file in LSP if not already open, or send didChange if it is.
/// This keeps the LSP server in sync with disk content after mutations.
async fn ensure_file_synced(
    _model: &RustModel,
    client: &crate::lsp::client::LspClient,
    uri: &str,
) -> Result<(), String> {
    let path = url::Url::parse(uri)
        .map_err(|e| format!("invalid URI: {}", e))?
        .to_file_path()
        .map_err(|_| "cannot convert URI to file path".to_string())?;
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read file: {}", e))?;

    // SAFETY: We need mutable access to open_documents but only hold an
    // immutable &RustModel. The model is behind Arc<Mutex> at the call site,
    // but the mutex is already held by the caller for the LspClient. We use
    // an UnsafeCell-free approach: check membership first, then update after.
    // Actually — open_documents is on the model which is behind its own
    // Arc<Mutex> at the server level, but here we only have &RustModel.
    // We'll use a simple interior-mutability workaround via the model pointer.
    //
    // For now, we always send didOpen (idempotent for our purposes —
    // rust-analyzer accepts re-opening with latest content).
    // TODO: track versions properly once model is passed as &mut.
    client
        .did_open(uri, &text)
        .await
        .map_err(|e| format!("didOpen failed: {}", e))
}

/// After applying a WorkspaceEdit, sync all changed files with the LSP server.
async fn sync_after_edit(
    model: &RustModel,
    client: &crate::lsp::client::LspClient,
    result: &ApplyResult,
) -> Result<(), String> {
    for (uri, _) in &result.files_changed {
        ensure_file_synced(model, client, uri).await?;
    }
    for uri in &result.files_created {
        ensure_file_synced(model, client, uri).await?;
    }
    Ok(())
}

/// Build a file URI from a selector value, using model root as base.
fn file_uri(model: &RustModel, file_value: &str) -> String {
    if file_value.starts_with("file://") {
        file_value.to_string()
    } else {
        format!(
            "{}/{}",
            model.root_uri.as_str().trim_end_matches('/'),
            file_value
        )
    }
}

// ── rename ──────────────────────────────────────────────────────────────────

async fn handle_rename(
    model: &RustModel,
    positionals: &[String],
    selectors: &[String],
) -> String {
    if positionals.len() < 2 {
        return format_error("rename requires SYMBOL and NEW_NAME.", None);
    }
    let old_name = &positionals[0];
    let new_name = &positionals[1];

    let parsed_selectors: Vec<_> = selectors.iter().filter_map(|s| parse_selector(s)).collect();
    let resolved = resolve_with_fallback(model, old_name, &parsed_selectors).await;

    let entry = match resolved {
        ResolveResult::Found(e) => e,
        ResolveResult::Ambiguous(entries) => {
            return format_disambiguation(old_name, &entries);
        }
        ResolveResult::NotFound => {
            return format_error(&format!("symbol '{}' not found.", old_name), None);
        }
    };

    let client_arc = Arc::clone(model.lsp_client.as_ref().unwrap());
    let client = client_arc.lock().await;

    let params = serde_json::json!({
        "textDocument": {"uri": &entry.uri},
        "position": {
            "line": entry.selection_range.start.line,
            "character": entry.selection_range.start.character,
        },
        "newName": new_name,
    });

    let workspace_edit: WorkspaceEdit = match client
        .request("textDocument/rename", params)
        .await
    {
        Ok(edit) => edit,
        Err(e) => return format_error(&format!("rename failed: {}", e), None),
    };
    drop(client);

    match apply_workspace_edit(&workspace_edit) {
        Ok(result) => format_mutation_result(
            "rename",
            &format!("{} → {}", old_name, new_name),
            &result,
            model.root_uri.as_str(),
        ),
        Err(e) => format_error(&format!("failed to apply rename: {}", e), None),
    }
}

// ── extract ─────────────────────────────────────────────────────────────────

async fn handle_extract(
    model: &RustModel,
    positionals: &[String],
    selectors: &[String],
) -> String {
    if positionals.is_empty() {
        return format_error("extract requires FUNC_NAME.", None);
    }
    let func_name = &positionals[0];

    let parsed_selectors: Vec<_> = selectors.iter().filter_map(|s| parse_selector(s)).collect();

    let file_sel = parsed_selectors
        .iter()
        .find(|s| s.selector_type == SelectorType::File);
    let lines_sel = parsed_selectors
        .iter()
        .find(|s| s.selector_type == SelectorType::Lines);

    let file_value = match file_sel {
        Some(s) => &s.value,
        None => return format_error("extract requires @file:PATH selector.", None),
    };
    let (start_line, end_line) = match lines_sel {
        Some(s) => match parse_line_range(&s.value) {
            Some(range) => range,
            None => {
                return format_error(
                    &format!("invalid line range '{}'. Use @lines:N-M.", s.value),
                    None,
                )
            }
        },
        None => return format_error("extract requires @lines:N-M selector.", None),
    };

    let uri = file_uri(model, file_value);
    // Convert 1-indexed user lines to 0-indexed LSP
    let lsp_start = if start_line > 0 {
        start_line - 1
    } else {
        start_line
    };
    let lsp_end = if end_line > 0 {
        end_line - 1
    } else {
        end_line
    };

    let client_arc = Arc::clone(model.lsp_client.as_ref().unwrap());
    let client = client_arc.lock().await;

    if let Err(e) = ensure_file_synced(model, &client, &uri).await {
        return format_error(&format!("extract: {}", e), None);
    }

    let params = serde_json::json!({
        "textDocument": {"uri": &uri},
        "range": {
            "start": {"line": lsp_start, "character": 0},
            "end": {"line": lsp_end, "character": 999},
        },
        "context": {
            "diagnostics": [],
            "only": ["refactor.extract.function", "refactor.extract"],
            "triggerKind": 1,
        },
    });

    let actions: Vec<CodeAction> = match client
        .request::<Option<Vec<CodeAction>>>("textDocument/codeAction", params)
        .await
    {
        Ok(Some(a)) => a,
        Ok(None) => vec![],
        Err(e) => return format_error(&format!("extract failed: {}", e), None),
    };

    let extract_actions: Vec<&CodeAction> = actions
        .iter()
        .filter(|a| {
            a.kind
                .as_deref()
                .map(|k| k.starts_with("refactor.extract"))
                .unwrap_or(false)
        })
        .collect();

    let action = match extract_actions.len() {
        0 => return format_error("no extract action available for the selected range.", None),
        1 => extract_actions[0],
        _ => {
            // Prefer "Extract into function" over "Extract into variable"
            if let Some(func_action) = extract_actions
                .iter()
                .find(|a| a.title.to_lowercase().contains("function"))
            {
                func_action
            } else if let Some(preferred) =
                extract_actions.iter().find(|a| a.is_preferred == Some(true))
            {
                preferred
            } else {
                return format_code_action_choices(
                    &extract_actions.iter().map(|a| (*a).clone()).collect::<Vec<_>>(),
                );
            }
        }
    };

    let edit = match &action.edit {
        Some(e) => e,
        None => return format_error("extract action has no edit.", None),
    };
    drop(client);

    let apply_result = match apply_workspace_edit(edit) {
        Ok(r) => r,
        Err(e) => return format_error(&format!("failed to apply extract: {}", e), None),
    };

    // Sync changed files with LSP before follow-up rename so the server
    // sees the extracted function, not stale pre-extract content.
    {
        let client = client_arc.lock().await;
        if let Err(e) = sync_after_edit(model, &client, &apply_result).await {
            return format_error(&format!("extract sync failed: {}", e), None);
        }
    }

    // Follow-up rename: rust-analyzer generates a placeholder name like "fun_name".
    // Rename it to the user's requested function name.
    let rename_result = follow_up_rename(model, &uri, func_name).await;

    match rename_result {
        Some(combined) => format_mutation_result(
            "extract",
            func_name,
            &combined,
            model.root_uri.as_str(),
        ),
        None => format_mutation_result(
            "extract",
            func_name,
            &apply_result,
            model.root_uri.as_str(),
        ),
    }
}

/// After extract, try to rename the generated function to the user's desired name.
/// Returns combined ApplyResult if successful, None if rename wasn't needed/possible.
async fn follow_up_rename(
    model: &RustModel,
    uri: &str,
    desired_name: &str,
) -> Option<ApplyResult> {
    let client_arc = Arc::clone(model.lsp_client.as_ref()?);
    let client = client_arc.lock().await;

    // Read the file to find the generated function (rust-analyzer uses "fun_name")
    let path = url::Url::parse(uri)
        .ok()
        .and_then(|u| u.to_file_path().ok())?;
    let content = std::fs::read_to_string(&path).ok()?;

    // Look for "fn fun_name" pattern — rust-analyzer's default extract name
    let generated_name = "fun_name";
    let fn_pattern = format!("fn {}", generated_name);
    let byte_offset = content.find(&fn_pattern)?;
    // Find the position of the function name (after "fn ")
    let name_offset = byte_offset + 3; // "fn ".len()

    // Convert byte offset to Position
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in content.chars().enumerate() {
        if i == name_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    let params = serde_json::json!({
        "textDocument": {"uri": uri},
        "position": {"line": line, "character": col},
        "newName": desired_name,
    });

    let workspace_edit: WorkspaceEdit = client
        .request("textDocument/rename", params)
        .await
        .ok()?;
    drop(client);

    apply_workspace_edit(&workspace_edit).ok()
}

// ── inline ──────────────────────────────────────────────────────────────────

async fn handle_inline(
    model: &RustModel,
    positionals: &[String],
    selectors: &[String],
) -> String {
    if positionals.is_empty() {
        return format_error("inline requires SYMBOL.", None);
    }
    let name = &positionals[0];

    let parsed_selectors: Vec<_> = selectors.iter().filter_map(|s| parse_selector(s)).collect();
    let resolved = resolve_with_fallback(model, name, &parsed_selectors).await;

    let entry = match resolved {
        ResolveResult::Found(e) => e,
        ResolveResult::Ambiguous(entries) => {
            return format_disambiguation(name, &entries);
        }
        ResolveResult::NotFound => {
            return format_error(&format!("symbol '{}' not found.", name), None);
        }
    };

    let client_arc = Arc::clone(model.lsp_client.as_ref().unwrap());
    let client = client_arc.lock().await;

    if let Err(e) = ensure_file_synced(model, &client, &entry.uri).await {
        return format_error(&format!("inline: {}", e), None);
    }

    let params = serde_json::json!({
        "textDocument": {"uri": &entry.uri},
        "range": {
            "start": {
                "line": entry.selection_range.start.line,
                "character": entry.selection_range.start.character,
            },
            "end": {
                "line": entry.selection_range.end.line,
                "character": entry.selection_range.end.character,
            },
        },
        "context": {
            "diagnostics": [],
            "only": ["refactor.inline"],
            "triggerKind": 1,
        },
    });

    let actions: Vec<CodeAction> = match client
        .request::<Option<Vec<CodeAction>>>("textDocument/codeAction", params)
        .await
    {
        Ok(Some(a)) => a,
        Ok(None) => vec![],
        Err(e) => return format_error(&format!("inline failed: {}", e), None),
    };

    let inline_actions: Vec<&CodeAction> = actions
        .iter()
        .filter(|a| {
            let title_lower = a.title.to_lowercase();
            let kind_match = a
                .kind
                .as_deref()
                .map(|k| k.contains("inline"))
                .unwrap_or(false);
            kind_match
                || title_lower.contains("inline")
        })
        .collect();

    let action = match inline_actions.len() {
        0 => return format_error(&format!("no inline action available for '{}'.", name), None),
        1 => inline_actions[0],
        _ => {
            if let Some(preferred) = inline_actions.iter().find(|a| a.is_preferred == Some(true)) {
                preferred
            } else {
                return format_code_action_choices(
                    &inline_actions
                        .iter()
                        .map(|a| (*a).clone())
                        .collect::<Vec<_>>(),
                );
            }
        }
    };

    let edit = match &action.edit {
        Some(e) => e,
        None => return format_error("inline action has no edit.", None),
    };
    drop(client);

    match apply_workspace_edit(edit) {
        Ok(result) => {
            format_mutation_result("inline", name, &result, model.root_uri.as_str())
        }
        Err(e) => format_error(&format!("failed to apply inline: {}", e), None),
    }
}

// ── generate ────────────────────────────────────────────────────────────────

const DERIVABLE_TRAITS: &[&str] = &[
    "Debug", "Clone", "Copy", "PartialEq", "Eq",
    "Hash", "PartialOrd", "Ord", "Default",
];

fn is_derivable(trait_name: &str) -> bool {
    DERIVABLE_TRAITS.iter().any(|t| t.eq_ignore_ascii_case(trait_name))
}

/// Canonical form of a derivable trait name (e.g. "debug" → "Debug").
fn canonical_trait_name(trait_name: &str) -> String {
    DERIVABLE_TRAITS
        .iter()
        .find(|t| t.eq_ignore_ascii_case(trait_name))
        .map(|t| t.to_string())
        .unwrap_or_else(|| trait_name.to_string())
}

async fn handle_generate(
    model: &RustModel,
    positionals: &[String],
    selectors: &[String],
) -> String {
    if positionals.is_empty() {
        return format_error("generate requires TRAIT.", None);
    }
    let trait_name = &positionals[0];

    let parsed_selectors: Vec<_> = selectors.iter().filter_map(|s| parse_selector(s)).collect();
    let struct_sel = parsed_selectors
        .iter()
        .find(|s| s.selector_type == SelectorType::Struct);

    let struct_name = match struct_sel {
        Some(s) => &s.value,
        None => return format_error("generate requires @struct:NAME selector.", None),
    };

    // Resolve the struct to get its position
    let struct_selectors = vec![crate::resolver::selectors::ParsedSelector {
        selector_type: SelectorType::Struct,
        value: struct_name.clone(),
    }];
    let resolved = resolve_with_fallback(model, struct_name, &struct_selectors).await;

    let entry = match resolved {
        ResolveResult::Found(e) => e,
        ResolveResult::Ambiguous(entries) => {
            return format_disambiguation(struct_name, &entries);
        }
        ResolveResult::NotFound => {
            return format_error(&format!("struct '{}' not found.", struct_name), None);
        }
    };

    if is_derivable(trait_name) {
        return handle_generate_derive(model, trait_name, struct_name, &entry).await;
    }

    // Non-derivable: fall back to code action approach
    handle_generate_code_action(model, trait_name, struct_name, &entry).await
}

/// Insert or extend `#[derive(...)]` for derivable traits.
async fn handle_generate_derive(
    model: &RustModel,
    trait_name: &str,
    struct_name: &str,
    entry: &crate::resolver::index::SymbolEntry,
) -> String {
    let canonical = canonical_trait_name(trait_name);

    let path = match url::Url::parse(&entry.uri)
        .ok()
        .and_then(|u| u.to_file_path().ok())
    {
        Some(p) => p,
        None => return format_error(&format!("invalid URI: {}", entry.uri), None),
    };

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return format_error(&format!("cannot read file: {}", e), None),
    };

    let lines: Vec<&str> = content.lines().collect();
    let struct_line = entry.range.start.line as usize;

    // Check if there's already a #[derive(...)] on the line(s) above the struct
    let (derive_line_idx, existing_derive) = find_derive_above(&lines, struct_line);

    let new_content = match existing_derive {
        Some(existing) => {
            // Check if trait is already derived
            if existing
                .split(',')
                .any(|t| t.trim().eq_ignore_ascii_case(&canonical))
            {
                return format_error(
                    &format!("{} is already derived for {}.", canonical, struct_name),
                    None,
                );
            }
            // Append trait to existing derive list
            let line = lines[derive_line_idx.unwrap()];
            let close_paren = line.rfind(')').unwrap();
            let mut new_line = String::new();
            new_line.push_str(&line[..close_paren]);
            new_line.push_str(", ");
            new_line.push_str(&canonical);
            new_line.push_str(&line[close_paren..]);

            let mut result_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            result_lines[derive_line_idx.unwrap()] = new_line;
            result_lines.join("\n")
        }
        None => {
            // Insert new #[derive(Trait)] above the struct line
            let indent = lines
                .get(struct_line)
                .map(|l| {
                    let trimmed = l.trim_start();
                    &l[..l.len() - trimmed.len()]
                })
                .unwrap_or("");
            let derive_attr = format!("{}#[derive({})]", indent, canonical);

            let mut result_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            result_lines.insert(struct_line, derive_attr);
            result_lines.join("\n")
        }
    };

    // Preserve trailing newline if original had one
    let new_content = if content.ends_with('\n') && !new_content.ends_with('\n') {
        new_content + "\n"
    } else {
        new_content
    };

    if let Err(e) = std::fs::write(&path, &new_content) {
        return format_error(&format!("failed to write file: {}", e), None);
    }

    // Sync with LSP
    if let Some(client_arc) = model.lsp_client.as_ref() {
        let client = client_arc.lock().await;
        let _ = ensure_file_synced(model, &client, &entry.uri).await;
    }

    let rel_path = entry
        .uri
        .strip_prefix(model.root_uri.as_str())
        .unwrap_or(&entry.uri)
        .trim_start_matches('/');

    let result = ApplyResult {
        files_changed: vec![(entry.uri.clone(), 1)],
        ..Default::default()
    };
    format_mutation_result(
        "generate",
        &format!("#[derive({})] for {} in {}", canonical, struct_name, rel_path),
        &result,
        model.root_uri.as_str(),
    )
}

/// Find a `#[derive(...)]` attribute on the line(s) immediately above `struct_line`.
/// Returns (line_index, inner_derive_content) if found.
fn find_derive_above(lines: &[&str], struct_line: usize) -> (Option<usize>, Option<String>) {
    if struct_line == 0 {
        return (None, None);
    }
    // Scan upward past attributes and doc comments
    let mut idx = struct_line - 1;
    loop {
        let trimmed = lines[idx].trim();
        if trimmed.starts_with("#[derive(") && trimmed.ends_with(")]") {
            let start = trimmed.find('(').unwrap() + 1;
            let end = trimmed.rfind(')').unwrap();
            let inner = trimmed[start..end].to_string();
            return (Some(idx), Some(inner));
        }
        if trimmed.starts_with("#[") || trimmed.starts_with("///") || trimmed.starts_with("//!") {
            if idx == 0 {
                break;
            }
            idx -= 1;
            continue;
        }
        break;
    }
    (None, None)
}

/// Fall back to LSP code actions for non-derivable traits.
async fn handle_generate_code_action(
    model: &RustModel,
    trait_name: &str,
    struct_name: &str,
    entry: &crate::resolver::index::SymbolEntry,
) -> String {
    let client_arc = Arc::clone(model.lsp_client.as_ref().unwrap());
    let client = client_arc.lock().await;

    if let Err(e) = ensure_file_synced(model, &client, &entry.uri).await {
        return format_error(&format!("generate: {}", e), None);
    }

    let params = serde_json::json!({
        "textDocument": {"uri": &entry.uri},
        "range": {
            "start": {
                "line": entry.selection_range.start.line,
                "character": entry.selection_range.start.character,
            },
            "end": {
                "line": entry.selection_range.end.line,
                "character": entry.selection_range.end.character,
            },
        },
        "context": {
            "diagnostics": [],
            "triggerKind": 1,
        },
    });

    let actions: Vec<CodeAction> = match client
        .request::<Option<Vec<CodeAction>>>("textDocument/codeAction", params)
        .await
    {
        Ok(Some(a)) => a,
        Ok(None) => vec![],
        Err(e) => return format_error(&format!("generate failed: {}", e), None),
    };

    // Filter for generate/impl actions that mention the trait
    let trait_lower = trait_name.to_lowercase();
    let generate_actions: Vec<&CodeAction> = actions
        .iter()
        .filter(|a| {
            let title_lower = a.title.to_lowercase();
            title_lower.contains(&trait_lower)
                || title_lower.contains("generate")
                || title_lower.contains("implement missing")
                || title_lower.contains("add impl")
        })
        .collect();

    let action = match generate_actions.len() {
        0 => {
            return format_error(
                &format!(
                    "no generate action for '{}' on struct '{}'.",
                    trait_name, struct_name
                ),
                None,
            )
        }
        1 => generate_actions[0],
        _ => {
            // Prefer actions whose title contains the trait name
            if let Some(best) = generate_actions
                .iter()
                .find(|a| a.title.to_lowercase().contains(&trait_lower))
            {
                best
            } else if let Some(preferred) =
                generate_actions.iter().find(|a| a.is_preferred == Some(true))
            {
                preferred
            } else {
                return format_code_action_choices(
                    &generate_actions
                        .iter()
                        .map(|a| (*a).clone())
                        .collect::<Vec<_>>(),
                );
            }
        }
    };

    let edit = match &action.edit {
        Some(e) => e,
        None => return format_error("generate action has no edit.", None),
    };
    drop(client);

    match apply_workspace_edit(edit) {
        Ok(result) => format_mutation_result(
            "generate",
            &format!("{} for {}", trait_name, struct_name),
            &result,
            model.root_uri.as_str(),
        ),
        Err(e) => format_error(&format!("failed to apply generate: {}", e), None),
    }
}

// ── import ──────────────────────────────────────────────────────────────────

async fn handle_import(
    model: &RustModel,
    positionals: &[String],
    selectors: &[String],
) -> String {
    if positionals.is_empty() {
        return format_error("import requires SYMBOL.", None);
    }
    let symbol_name = &positionals[0];

    let parsed_selectors: Vec<_> = selectors.iter().filter_map(|s| parse_selector(s)).collect();

    let file_sel = parsed_selectors
        .iter()
        .find(|s| s.selector_type == SelectorType::File);
    let line_sel = parsed_selectors
        .iter()
        .find(|s| s.selector_type == SelectorType::Line);

    let file_value = match file_sel {
        Some(s) => &s.value,
        None => return format_error("import requires @file:PATH selector.", None),
    };
    let line_num = match line_sel {
        Some(s) => match s.value.parse::<u32>() {
            Ok(n) => n,
            Err(_) => return format_error("invalid line number.", None),
        },
        None => return format_error("import requires @line:N selector.", None),
    };

    let uri = file_uri(model, file_value);
    let lsp_line = if line_num > 0 { line_num - 1 } else { line_num };

    let client_arc = Arc::clone(model.lsp_client.as_ref().unwrap());
    let client = client_arc.lock().await;

    if let Err(e) = ensure_file_synced(model, &client, &uri).await {
        return format_error(&format!("import: {}", e), None);
    }

    let params = serde_json::json!({
        "textDocument": {"uri": &uri},
        "range": {
            "start": {"line": lsp_line, "character": 0},
            "end": {"line": lsp_line, "character": 999},
        },
        "context": {
            "diagnostics": [],
            "only": ["quickfix", "source", "source.organizeImports"],
            "triggerKind": 1,
        },
    });

    let actions: Vec<CodeAction> = match client
        .request::<Option<Vec<CodeAction>>>("textDocument/codeAction", params)
        .await
    {
        Ok(Some(a)) => a,
        Ok(None) => vec![],
        Err(e) => return format_error(&format!("import failed: {}", e), None),
    };

    // Filter for import-related actions that mention the symbol
    let symbol_lower = symbol_name.to_lowercase();
    let import_actions: Vec<&CodeAction> = actions
        .iter()
        .filter(|a| {
            let title_lower = a.title.to_lowercase();
            let kind_match = a
                .kind
                .as_deref()
                .map(|k| {
                    k.contains("import") || k == "quickfix" || k.starts_with("source")
                })
                .unwrap_or(false);
            (kind_match || title_lower.contains("import") || title_lower.contains("use "))
                && title_lower.contains(&symbol_lower)
        })
        .collect();

    let action = match import_actions.len() {
        0 => {
            return format_error(
                &format!("no import action for '{}' at {}:{}.", symbol_name, file_value, line_num),
                None,
            )
        }
        1 => import_actions[0],
        _ => {
            if let Some(preferred) = import_actions.iter().find(|a| a.is_preferred == Some(true)) {
                preferred
            } else {
                return format_code_action_choices(
                    &import_actions
                        .iter()
                        .map(|a| (*a).clone())
                        .collect::<Vec<_>>(),
                );
            }
        }
    };

    let edit = match &action.edit {
        Some(e) => e,
        None => return format_error("import action has no edit.", None),
    };
    drop(client);

    match apply_workspace_edit(edit) {
        Ok(result) => format_mutation_result(
            "import",
            symbol_name,
            &result,
            model.root_uri.as_str(),
        ),
        Err(e) => format_error(&format!("failed to apply import: {}", e), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::verbs::{register_mutation_verbs, register_query_verbs, register_session_verbs};
    use crate::fcpcore::verb_registry::VerbRegistry;

    fn make_registry() -> VerbRegistry {
        let mut reg = VerbRegistry::new();
        register_query_verbs(&mut reg);
        register_mutation_verbs(&mut reg);
        register_session_verbs(&mut reg);
        reg
    }

    fn make_model() -> RustModel {
        RustModel::new(url::Url::parse("file:///project").unwrap())
    }

    #[tokio::test]
    async fn test_dispatch_mutation_parse_error() {
        let model = make_model();
        let reg = make_registry();
        let result = dispatch_mutation(&model, &reg, "").await;
        assert!(result.contains("parse error"));
    }

    #[tokio::test]
    async fn test_dispatch_mutation_unknown_verb() {
        let model = make_model();
        let reg = make_registry();
        let result = dispatch_mutation(&model, &reg, "refactor Config").await;
        assert!(result.contains("unknown verb"));
    }

    #[tokio::test]
    async fn test_dispatch_mutation_no_workspace() {
        let model = make_model();
        let reg = make_registry();
        let result = dispatch_mutation(&model, &reg, "rename Config Settings").await;
        assert!(result.contains("no workspace open"));
    }

    #[tokio::test]
    async fn test_dispatch_rename_missing_new_name() {
        // This won't reach the handler because no workspace is open,
        // but we test the verb is recognized
        let model = make_model();
        let reg = make_registry();
        let result = dispatch_mutation(&model, &reg, "rename Config").await;
        // No workspace → caught before missing-arg check
        assert!(result.contains("no workspace open"));
    }

    #[tokio::test]
    async fn test_dispatch_extract_recognized() {
        let model = make_model();
        let reg = make_registry();
        let result = dispatch_mutation(&model, &reg, "extract validate @file:server.rs @lines:15-30").await;
        assert!(result.contains("no workspace open"));
    }

    #[tokio::test]
    async fn test_dispatch_inline_recognized() {
        let model = make_model();
        let reg = make_registry();
        let result = dispatch_mutation(&model, &reg, "inline helper_fn @file:lib.rs").await;
        assert!(result.contains("no workspace open"));
    }

    #[tokio::test]
    async fn test_dispatch_generate_recognized() {
        let model = make_model();
        let reg = make_registry();
        let result = dispatch_mutation(&model, &reg, "generate Display @struct:Config").await;
        assert!(result.contains("no workspace open"));
    }

    #[tokio::test]
    async fn test_dispatch_import_recognized() {
        let model = make_model();
        let reg = make_registry();
        let result = dispatch_mutation(&model, &reg, "import HashMap @file:main.rs @line:5").await;
        assert!(result.contains("no workspace open"));
    }

    #[test]
    fn test_file_uri_absolute() {
        let model = make_model();
        assert_eq!(
            file_uri(&model, "file:///other/path.rs"),
            "file:///other/path.rs"
        );
    }

    #[test]
    fn test_file_uri_relative() {
        let model = make_model();
        assert_eq!(
            file_uri(&model, "src/main.rs"),
            "file:///project/src/main.rs"
        );
    }

    #[test]
    fn test_is_derivable() {
        assert!(is_derivable("Debug"));
        assert!(is_derivable("debug"));
        assert!(is_derivable("Clone"));
        assert!(is_derivable("PartialEq"));
        assert!(!is_derivable("Display"));
        assert!(!is_derivable("Iterator"));
        assert!(!is_derivable("Serialize"));
    }

    #[test]
    fn test_canonical_trait_name() {
        assert_eq!(canonical_trait_name("debug"), "Debug");
        assert_eq!(canonical_trait_name("partialeq"), "PartialEq");
        assert_eq!(canonical_trait_name("Display"), "Display");
    }

    #[test]
    fn test_find_derive_above_present() {
        let lines = vec![
            "#[derive(Clone, PartialEq)]",
            "pub struct Config {",
            "    name: String,",
            "}",
        ];
        let (idx, inner) = find_derive_above(&lines, 1);
        assert_eq!(idx, Some(0));
        assert_eq!(inner.as_deref(), Some("Clone, PartialEq"));
    }

    #[test]
    fn test_find_derive_above_with_doc_comment() {
        let lines = vec![
            "#[derive(Clone)]",
            "/// Documentation",
            "pub struct Config {",
            "}",
        ];
        let (idx, inner) = find_derive_above(&lines, 2);
        assert_eq!(idx, Some(0));
        assert_eq!(inner.as_deref(), Some("Clone"));
    }

    #[test]
    fn test_find_derive_above_missing() {
        let lines = vec![
            "pub struct Config {",
            "    name: String,",
            "}",
        ];
        let (idx, inner) = find_derive_above(&lines, 0);
        assert_eq!(idx, None);
        assert_eq!(inner, None);
    }

    #[test]
    fn test_find_derive_above_with_other_attrs() {
        let lines = vec![
            "#[derive(Clone)]",
            "#[serde(rename_all = \"camelCase\")]",
            "pub struct Config {",
            "}",
        ];
        let (idx, inner) = find_derive_above(&lines, 2);
        assert_eq!(idx, Some(0));
        assert_eq!(inner.as_deref(), Some("Clone"));
    }
}
