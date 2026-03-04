// Selector filtering for symbol resolution

use crate::lsp::types::{SymbolInformation, SymbolKind};

#[derive(Debug, Clone, PartialEq)]
pub enum SelectorType {
    File,
    Struct,
    Trait,
    Kind,
    Module,
    Line,
    Lines,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSelector {
    pub selector_type: SelectorType,
    pub value: String,
}

/// Parse a selector string like `@file:main.rs` or `@kind:function`.
pub fn parse_selector(selector: &str) -> Option<ParsedSelector> {
    let s = selector.strip_prefix('@')?;
    let (type_str, value) = s.split_once(':')?;

    let selector_type = match type_str {
        "file" => SelectorType::File,
        "struct" => SelectorType::Struct,
        "trait" => SelectorType::Trait,
        "kind" => SelectorType::Kind,
        "module" | "mod" => SelectorType::Module,
        "line" => SelectorType::Line,
        "lines" => SelectorType::Lines,
        _ => return None,
    };

    Some(ParsedSelector {
        selector_type,
        value: value.to_string(),
    })
}

/// Filter symbols by a list of parsed selectors. All selectors must match (AND logic).
pub fn filter_by_selectors<'a>(
    symbols: &'a [SymbolInformation],
    selectors: &[ParsedSelector],
) -> Vec<&'a SymbolInformation> {
    symbols
        .iter()
        .filter(|sym| selectors.iter().all(|sel| matches_selector(sym, sel)))
        .collect()
}

fn matches_selector(sym: &SymbolInformation, sel: &ParsedSelector) -> bool {
    match sel.selector_type {
        SelectorType::File => sym.location.uri.contains(&sel.value),
        SelectorType::Struct => {
            sym.container_name.as_deref() == Some(&sel.value)
                || (sym.name == sel.value && sym.kind == SymbolKind::Struct)
        }
        SelectorType::Trait => {
            sym.container_name.as_deref() == Some(&sel.value)
                || (sym.name == sel.value && sym.kind == SymbolKind::Interface)
        }
        SelectorType::Kind => {
            symbol_kind_from_string(&sel.value)
                .map(|k| sym.kind == k)
                .unwrap_or(false)
        }
        SelectorType::Module => {
            sym.container_name
                .as_deref()
                .map(|c| c.contains(&sel.value))
                .unwrap_or(false)
                || sym.location.uri.contains(&sel.value)
        }
        SelectorType::Line => {
            if let Ok(line) = sel.value.parse::<u32>() {
                sym.location.range.start.line <= line && line <= sym.location.range.end.line
            } else {
                false
            }
        }
        SelectorType::Lines => {
            // @lines is consumed by mutation handlers, not symbol filtering
            true
        }
    }
}

/// Parse a line range value like "15-30" into (start, end).
pub fn parse_line_range(value: &str) -> Option<(u32, u32)> {
    let (start_str, end_str) = value.split_once('-')?;
    let start = start_str.parse::<u32>().ok()?;
    let end = end_str.parse::<u32>().ok()?;
    if start <= end {
        Some((start, end))
    } else {
        None
    }
}

/// Convert a string to a SymbolKind.
pub fn symbol_kind_from_string(s: &str) -> Option<SymbolKind> {
    match s.to_lowercase().as_str() {
        "function" | "fn" => Some(SymbolKind::Function),
        "method" => Some(SymbolKind::Method),
        "struct" => Some(SymbolKind::Struct),
        "enum" => Some(SymbolKind::Enum),
        "interface" | "trait" => Some(SymbolKind::Interface),
        "variable" | "var" => Some(SymbolKind::Variable),
        "constant" | "const" => Some(SymbolKind::Constant),
        "property" => Some(SymbolKind::Property),
        "module" | "mod" => Some(SymbolKind::Module),
        "namespace" => Some(SymbolKind::Namespace),
        "class" => Some(SymbolKind::Class),
        "field" => Some(SymbolKind::Field),
        "constructor" => Some(SymbolKind::Constructor),
        "type_parameter" | "typeparameter" => Some(SymbolKind::TypeParameter),
        "file" => Some(SymbolKind::File),
        "package" => Some(SymbolKind::Package),
        "string" => Some(SymbolKind::String),
        "number" => Some(SymbolKind::Number),
        "boolean" | "bool" => Some(SymbolKind::Boolean),
        "array" => Some(SymbolKind::Array),
        "object" => Some(SymbolKind::Object),
        "key" => Some(SymbolKind::Key),
        "null" => Some(SymbolKind::Null),
        "enum_member" | "enummember" => Some(SymbolKind::EnumMember),
        "event" => Some(SymbolKind::Event),
        "operator" => Some(SymbolKind::Operator),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::types::{Location, Position, Range};

    fn make_sym(
        name: &str,
        kind: SymbolKind,
        uri: &str,
        container: Option<&str>,
        start_line: u32,
        end_line: u32,
    ) -> SymbolInformation {
        SymbolInformation {
            name: name.to_string(),
            kind,
            location: Location {
                uri: uri.to_string(),
                range: Range {
                    start: Position { line: start_line, character: 0 },
                    end: Position { line: end_line, character: 0 },
                },
            },
            container_name: container.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_parse_file_selector() {
        let sel = parse_selector("@file:main.rs").unwrap();
        assert_eq!(sel.selector_type, SelectorType::File);
        assert_eq!(sel.value, "main.rs");
    }

    #[test]
    fn test_parse_struct_selector() {
        let sel = parse_selector("@struct:MyStruct").unwrap();
        assert_eq!(sel.selector_type, SelectorType::Struct);
        assert_eq!(sel.value, "MyStruct");
    }

    #[test]
    fn test_parse_trait_selector() {
        let sel = parse_selector("@trait:Display").unwrap();
        assert_eq!(sel.selector_type, SelectorType::Trait);
        assert_eq!(sel.value, "Display");
    }

    #[test]
    fn test_parse_kind_selector() {
        let sel = parse_selector("@kind:function").unwrap();
        assert_eq!(sel.selector_type, SelectorType::Kind);
        assert_eq!(sel.value, "function");
    }

    #[test]
    fn test_parse_module_selector() {
        let sel = parse_selector("@module:utils").unwrap();
        assert_eq!(sel.selector_type, SelectorType::Module);
        assert_eq!(sel.value, "utils");
    }

    #[test]
    fn test_parse_mod_selector() {
        let sel = parse_selector("@mod:utils").unwrap();
        assert_eq!(sel.selector_type, SelectorType::Module);
        assert_eq!(sel.value, "utils");
    }

    #[test]
    fn test_parse_line_selector() {
        let sel = parse_selector("@line:42").unwrap();
        assert_eq!(sel.selector_type, SelectorType::Line);
        assert_eq!(sel.value, "42");
    }

    #[test]
    fn test_parse_lines_selector() {
        let sel = parse_selector("@lines:15-30").unwrap();
        assert_eq!(sel.selector_type, SelectorType::Lines);
        assert_eq!(sel.value, "15-30");
    }

    #[test]
    fn test_parse_line_range_valid() {
        assert_eq!(parse_line_range("15-30"), Some((15, 30)));
        assert_eq!(parse_line_range("1-1"), Some((1, 1)));
    }

    #[test]
    fn test_parse_line_range_invalid() {
        assert_eq!(parse_line_range("30-15"), None);
        assert_eq!(parse_line_range("abc"), None);
        assert_eq!(parse_line_range("15"), None);
    }

    #[test]
    fn test_parse_unknown_selector() {
        assert!(parse_selector("@unknown:value").is_none());
    }

    #[test]
    fn test_parse_no_at() {
        assert!(parse_selector("file:main.rs").is_none());
    }

    #[test]
    fn test_parse_no_colon() {
        assert!(parse_selector("@file").is_none());
    }

    #[test]
    fn test_filter_by_file() {
        let syms = vec![
            make_sym("foo", SymbolKind::Function, "file:///lib.rs", None, 0, 5),
            make_sym("bar", SymbolKind::Function, "file:///main.rs", None, 0, 5),
        ];
        let sel = vec![parse_selector("@file:main.rs").unwrap()];
        let result = filter_by_selectors(&syms, &sel);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "bar");
    }

    #[test]
    fn test_filter_by_kind() {
        let syms = vec![
            make_sym("MyStruct", SymbolKind::Struct, "file:///lib.rs", None, 0, 10),
            make_sym("foo", SymbolKind::Function, "file:///lib.rs", None, 12, 20),
        ];
        let sel = vec![parse_selector("@kind:function").unwrap()];
        let result = filter_by_selectors(&syms, &sel);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "foo");
    }

    #[test]
    fn test_filter_by_struct() {
        let syms = vec![
            make_sym("method_a", SymbolKind::Method, "file:///lib.rs", Some("MyStruct"), 5, 10),
            make_sym("method_b", SymbolKind::Method, "file:///lib.rs", Some("Other"), 15, 20),
        ];
        let sel = vec![parse_selector("@struct:MyStruct").unwrap()];
        let result = filter_by_selectors(&syms, &sel);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "method_a");
    }

    #[test]
    fn test_filter_by_multiple_selectors() {
        let syms = vec![
            make_sym("foo", SymbolKind::Function, "file:///lib.rs", None, 0, 5),
            make_sym("bar", SymbolKind::Function, "file:///main.rs", None, 0, 5),
            make_sym("baz", SymbolKind::Struct, "file:///main.rs", None, 10, 20),
        ];
        let sel = vec![
            parse_selector("@file:main.rs").unwrap(),
            parse_selector("@kind:function").unwrap(),
        ];
        let result = filter_by_selectors(&syms, &sel);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "bar");
    }

    #[test]
    fn test_filter_by_line() {
        let syms = vec![
            make_sym("foo", SymbolKind::Function, "file:///lib.rs", None, 0, 5),
            make_sym("bar", SymbolKind::Function, "file:///lib.rs", None, 10, 20),
        ];
        let sel = vec![parse_selector("@line:15").unwrap()];
        let result = filter_by_selectors(&syms, &sel);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "bar");
    }

    #[test]
    fn test_symbol_kind_from_string_function() {
        assert_eq!(symbol_kind_from_string("function"), Some(SymbolKind::Function));
        assert_eq!(symbol_kind_from_string("fn"), Some(SymbolKind::Function));
    }

    #[test]
    fn test_symbol_kind_from_string_various() {
        assert_eq!(symbol_kind_from_string("struct"), Some(SymbolKind::Struct));
        assert_eq!(symbol_kind_from_string("enum"), Some(SymbolKind::Enum));
        assert_eq!(symbol_kind_from_string("trait"), Some(SymbolKind::Interface));
        assert_eq!(symbol_kind_from_string("method"), Some(SymbolKind::Method));
        assert_eq!(symbol_kind_from_string("module"), Some(SymbolKind::Module));
        assert_eq!(symbol_kind_from_string("constant"), Some(SymbolKind::Constant));
        assert_eq!(symbol_kind_from_string("variable"), Some(SymbolKind::Variable));
        assert_eq!(symbol_kind_from_string("class"), Some(SymbolKind::Class));
        assert_eq!(symbol_kind_from_string("field"), Some(SymbolKind::Field));
        assert_eq!(symbol_kind_from_string("unknown"), None);
    }
}
