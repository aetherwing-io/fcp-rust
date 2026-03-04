// LSP 3.17 type definitions — hand-rolled subset
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentIdentifier {
    pub uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentPositionParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VersionedTextDocumentIdentifier {
    pub uri: String,
    pub version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentItem {
    pub uri: String,
    pub language_id: String,
    pub version: i32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub process_id: Option<i64>,
    pub root_uri: Option<String>,
    pub capabilities: ClientCapabilities,
    pub initialization_options: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    pub general: Option<GeneralCapabilities>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GeneralCapabilities {
    pub position_encodings: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub capabilities: ServerCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    pub text_document_sync: Option<serde_json::Value>,
    pub definition_provider: Option<bool>,
    pub references_provider: Option<bool>,
    pub document_symbol_provider: Option<bool>,
    pub workspace_symbol_provider: Option<bool>,
    pub hover_provider: Option<bool>,
    pub implementation_provider: Option<bool>,
    pub call_hierarchy_provider: Option<bool>,
    pub rename_provider: Option<serde_json::Value>,
    pub code_action_provider: Option<serde_json::Value>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    File,
    Module,
    Namespace,
    Package,
    Class,
    Method,
    Property,
    Field,
    Constructor,
    Enum,
    Interface,
    Function,
    Variable,
    Constant,
    String,
    Number,
    Boolean,
    Array,
    Object,
    Key,
    Null,
    EnumMember,
    Struct,
    Event,
    Operator,
    TypeParameter,
    Other(u32),
}

impl std::fmt::Debug for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Other(v) => write!(f, "SymbolKind({})", v),
            _ => {
                let name = match self {
                    Self::File => "File",
                    Self::Module => "Module",
                    Self::Namespace => "Namespace",
                    Self::Package => "Package",
                    Self::Class => "Class",
                    Self::Method => "Method",
                    Self::Property => "Property",
                    Self::Field => "Field",
                    Self::Constructor => "Constructor",
                    Self::Enum => "Enum",
                    Self::Interface => "Interface",
                    Self::Function => "Function",
                    Self::Variable => "Variable",
                    Self::Constant => "Constant",
                    Self::String => "String",
                    Self::Number => "Number",
                    Self::Boolean => "Boolean",
                    Self::Array => "Array",
                    Self::Object => "Object",
                    Self::Key => "Key",
                    Self::Null => "Null",
                    Self::EnumMember => "EnumMember",
                    Self::Struct => "Struct",
                    Self::Event => "Event",
                    Self::Operator => "Operator",
                    Self::TypeParameter => "TypeParameter",
                    Self::Other(_) => unreachable!(),
                };
                write!(f, "{}", name)
            }
        }
    }
}

impl SymbolKind {
    fn to_u32(self) -> u32 {
        match self {
            Self::File => 1,
            Self::Module => 2,
            Self::Namespace => 3,
            Self::Package => 4,
            Self::Class => 5,
            Self::Method => 6,
            Self::Property => 7,
            Self::Field => 8,
            Self::Constructor => 9,
            Self::Enum => 10,
            Self::Interface => 11,
            Self::Function => 12,
            Self::Variable => 13,
            Self::Constant => 14,
            Self::String => 15,
            Self::Number => 16,
            Self::Boolean => 17,
            Self::Array => 18,
            Self::Object => 19,
            Self::Key => 20,
            Self::Null => 21,
            Self::EnumMember => 22,
            Self::Struct => 23,
            Self::Event => 24,
            Self::Operator => 25,
            Self::TypeParameter => 26,
            Self::Other(v) => v,
        }
    }

    fn from_u32(value: u32) -> Self {
        match value {
            1 => Self::File,
            2 => Self::Module,
            3 => Self::Namespace,
            4 => Self::Package,
            5 => Self::Class,
            6 => Self::Method,
            7 => Self::Property,
            8 => Self::Field,
            9 => Self::Constructor,
            10 => Self::Enum,
            11 => Self::Interface,
            12 => Self::Function,
            13 => Self::Variable,
            14 => Self::Constant,
            15 => Self::String,
            16 => Self::Number,
            17 => Self::Boolean,
            18 => Self::Array,
            19 => Self::Object,
            20 => Self::Key,
            21 => Self::Null,
            22 => Self::EnumMember,
            23 => Self::Struct,
            24 => Self::Event,
            25 => Self::Operator,
            26 => Self::TypeParameter,
            v => Self::Other(v),
        }
    }
}

impl Serialize for SymbolKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.to_u32())
    }
}

impl<'de> Deserialize<'de> for SymbolKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u32::deserialize(deserializer)?;
        Ok(Self::from_u32(value))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInformation {
    pub name: String,
    pub kind: SymbolKind,
    pub location: Location,
    pub container_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range,
    pub selection_range: Range,
    pub children: Option<Vec<DocumentSymbol>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

impl Serialize for DiagnosticSeverity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(*self as u32)
    }
}

impl<'de> Deserialize<'de> for DiagnosticSeverity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u32::deserialize(deserializer)?;
        match value {
            1 => Ok(DiagnosticSeverity::Error),
            2 => Ok(DiagnosticSeverity::Warning),
            3 => Ok(DiagnosticSeverity::Information),
            4 => Ok(DiagnosticSeverity::Hint),
            _ => Err(serde::de::Error::custom(format!(
                "unknown DiagnosticSeverity value: {}",
                value
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub range: Range,
    pub severity: Option<DiagnosticSeverity>,
    pub code: Option<serde_json::Value>,
    pub source: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PublishDiagnosticsParams {
    pub uri: String,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hover {
    pub contents: HoverContents,
    pub range: Option<Range>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum HoverContents {
    MarkedString(String),
    MarkupContent(MarkupContent),
    MarkedStringArray(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MarkupContent {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallHierarchyItem {
    pub name: String,
    pub kind: SymbolKind,
    pub uri: String,
    pub range: Range,
    pub selection_range: Range,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallHierarchyIncomingCall {
    pub from: CallHierarchyItem,
    pub from_ranges: Vec<Range>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallHierarchyOutgoingCall {
    pub to: CallHierarchyItem,
    pub from_ranges: Vec<Range>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSymbolParams {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DidOpenTextDocumentParams {
    pub text_document: TextDocumentItem,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DidCloseTextDocumentParams {
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OptionalVersionedTextDocumentIdentifier {
    pub uri: String,
    pub version: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentEdit {
    pub text_document: OptionalVersionedTextDocumentIdentifier,
    pub edits: Vec<TextEdit>,
}

/// LSP ResourceOperation for file create/rename/delete
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum ResourceOperation {
    #[serde(rename = "create")]
    Create { uri: String },
    #[serde(rename = "rename")]
    Rename { old_uri: String, new_uri: String },
    #[serde(rename = "delete")]
    Delete { uri: String },
}

/// DocumentChange is either a TextDocumentEdit or a ResourceOperation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DocumentChange {
    Edit(TextDocumentEdit),
    Operation(ResourceOperation),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEdit {
    /// Simple form: uri → edits
    pub changes: Option<std::collections::HashMap<String, Vec<TextEdit>>>,
    /// Rich form (preferred by rust-analyzer): ordered document changes
    pub document_changes: Option<Vec<DocumentChange>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub edit: Option<WorkspaceEdit>,
    pub is_preferred: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_serialize_position() {
        let pos = Position { line: 10, character: 5 };
        let json = serde_json::to_value(&pos).unwrap();
        assert_eq!(json, json!({"line": 10, "character": 5}));
        let deser: Position = serde_json::from_value(json).unwrap();
        assert_eq!(deser, pos);
    }

    #[test]
    fn test_serialize_range() {
        let range = Range {
            start: Position { line: 1, character: 0 },
            end: Position { line: 1, character: 10 },
        };
        let json = serde_json::to_value(&range).unwrap();
        assert_eq!(json["start"]["line"], 1);
        assert_eq!(json["end"]["character"], 10);
        let deser: Range = serde_json::from_value(json).unwrap();
        assert_eq!(deser, range);
    }

    #[test]
    fn test_serialize_location() {
        let loc = Location {
            uri: "file:///test.rs".to_string(),
            range: Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 5 },
            },
        };
        let json = serde_json::to_value(&loc).unwrap();
        assert_eq!(json["uri"], "file:///test.rs");
        let deser: Location = serde_json::from_value(json).unwrap();
        assert_eq!(deser, loc);
    }

    #[test]
    fn test_symbol_kind_as_u32() {
        assert_eq!(serde_json::to_value(SymbolKind::Function).unwrap(), json!(12));
        assert_eq!(serde_json::to_value(SymbolKind::Method).unwrap(), json!(6));
        assert_eq!(serde_json::to_value(SymbolKind::Struct).unwrap(), json!(23));
        assert_eq!(serde_json::to_value(SymbolKind::Enum).unwrap(), json!(10));

        let deser: SymbolKind = serde_json::from_value(json!(12)).unwrap();
        assert_eq!(deser, SymbolKind::Function);
        let deser: SymbolKind = serde_json::from_value(json!(23)).unwrap();
        assert_eq!(deser, SymbolKind::Struct);
    }

    #[test]
    fn test_diagnostic_severity_as_u32() {
        assert_eq!(serde_json::to_value(DiagnosticSeverity::Error).unwrap(), json!(1));
        assert_eq!(serde_json::to_value(DiagnosticSeverity::Warning).unwrap(), json!(2));
        assert_eq!(serde_json::to_value(DiagnosticSeverity::Information).unwrap(), json!(3));
        assert_eq!(serde_json::to_value(DiagnosticSeverity::Hint).unwrap(), json!(4));

        let deser: DiagnosticSeverity = serde_json::from_value(json!(1)).unwrap();
        assert_eq!(deser, DiagnosticSeverity::Error);
    }

    #[test]
    fn test_serialize_initialize_params() {
        let params = InitializeParams {
            process_id: Some(1234),
            root_uri: Some("file:///project".to_string()),
            capabilities: ClientCapabilities {
                general: Some(GeneralCapabilities {
                    position_encodings: Some(vec!["utf-32".to_string()]),
                }),
            },
            initialization_options: None,
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["processId"], 1234);
        assert_eq!(json["rootUri"], "file:///project");
        let deser: InitializeParams = serde_json::from_value(json).unwrap();
        assert_eq!(deser, params);
    }

    #[test]
    fn test_serialize_initialize_result() {
        let result = InitializeResult {
            capabilities: ServerCapabilities {
                definition_provider: Some(true),
                hover_provider: Some(true),
                ..Default::default()
            },
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["capabilities"]["definitionProvider"], true);
        assert_eq!(json["capabilities"]["hoverProvider"], true);
        let deser: InitializeResult = serde_json::from_value(json).unwrap();
        assert_eq!(deser, result);
    }

    #[test]
    fn test_serialize_symbol_information() {
        let sym = SymbolInformation {
            name: "main".to_string(),
            kind: SymbolKind::Function,
            location: Location {
                uri: "file:///main.rs".to_string(),
                range: Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 5, character: 1 },
                },
            },
            container_name: None,
        };
        let json = serde_json::to_value(&sym).unwrap();
        assert_eq!(json["name"], "main");
        assert_eq!(json["kind"], 12);
        let deser: SymbolInformation = serde_json::from_value(json).unwrap();
        assert_eq!(deser, sym);
    }

    #[test]
    fn test_serialize_document_symbol() {
        let ds = DocumentSymbol {
            name: "MyStruct".to_string(),
            kind: SymbolKind::Struct,
            range: Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 10, character: 1 },
            },
            selection_range: Range {
                start: Position { line: 0, character: 11 },
                end: Position { line: 0, character: 19 },
            },
            children: Some(vec![DocumentSymbol {
                name: "field".to_string(),
                kind: SymbolKind::Field,
                range: Range {
                    start: Position { line: 1, character: 4 },
                    end: Position { line: 1, character: 20 },
                },
                selection_range: Range {
                    start: Position { line: 1, character: 4 },
                    end: Position { line: 1, character: 9 },
                },
                children: None,
            }]),
        };
        let json = serde_json::to_value(&ds).unwrap();
        assert_eq!(json["name"], "MyStruct");
        assert_eq!(json["kind"], 23);
        assert_eq!(json["children"][0]["name"], "field");
        let deser: DocumentSymbol = serde_json::from_value(json).unwrap();
        assert_eq!(deser, ds);
    }

    #[test]
    fn test_serialize_diagnostic() {
        let diag = Diagnostic {
            range: Range {
                start: Position { line: 5, character: 0 },
                end: Position { line: 5, character: 10 },
            },
            severity: Some(DiagnosticSeverity::Error),
            code: Some(json!("E0308")),
            source: Some("rustc".to_string()),
            message: "mismatched types".to_string(),
        };
        let json = serde_json::to_value(&diag).unwrap();
        assert_eq!(json["severity"], 1);
        assert_eq!(json["message"], "mismatched types");
        let deser: Diagnostic = serde_json::from_value(json).unwrap();
        assert_eq!(deser, diag);
    }

    #[test]
    fn test_serialize_hover_marked_string() {
        let hover = Hover {
            contents: HoverContents::MarkedString("fn main()".to_string()),
            range: None,
        };
        let json = serde_json::to_value(&hover).unwrap();
        assert_eq!(json["contents"], "fn main()");
        let deser: Hover = serde_json::from_value(json).unwrap();
        assert_eq!(deser, hover);
    }

    #[test]
    fn test_serialize_hover_markup_content() {
        let hover = Hover {
            contents: HoverContents::MarkupContent(MarkupContent {
                kind: "markdown".to_string(),
                value: "```rust\nfn main()\n```".to_string(),
            }),
            range: Some(Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 4 },
            }),
        };
        let json = serde_json::to_value(&hover).unwrap();
        assert_eq!(json["contents"]["kind"], "markdown");
        let deser: Hover = serde_json::from_value(json).unwrap();
        assert_eq!(deser, hover);
    }

    #[test]
    fn test_serialize_call_hierarchy_item() {
        let item = CallHierarchyItem {
            name: "process".to_string(),
            kind: SymbolKind::Function,
            uri: "file:///lib.rs".to_string(),
            range: Range {
                start: Position { line: 10, character: 0 },
                end: Position { line: 20, character: 1 },
            },
            selection_range: Range {
                start: Position { line: 10, character: 3 },
                end: Position { line: 10, character: 10 },
            },
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["name"], "process");
        assert_eq!(json["kind"], 12);
        let deser: CallHierarchyItem = serde_json::from_value(json).unwrap();
        assert_eq!(deser, item);
    }

    #[test]
    fn test_serialize_jsonrpc_request() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(1),
            method: "textDocument/definition".to_string(),
            params: Some(json!({"textDocument": {"uri": "file:///test.rs"}, "position": {"line": 0, "character": 5}})),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "textDocument/definition");
        let deser: JsonRpcRequest = serde_json::from_value(json).unwrap();
        assert_eq!(deser, req);
    }

    #[test]
    fn test_serialize_jsonrpc_response_success() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            result: Some(json!({"line": 10, "character": 5})),
            error: None,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["id"], 1);
        assert!(json["result"].is_object());
        assert!(json["error"].is_null());
        let deser: JsonRpcResponse = serde_json::from_value(json).unwrap();
        assert_eq!(deser, resp);
    }

    #[test]
    fn test_serialize_jsonrpc_response_error() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(2)),
            result: None,
            error: Some(JsonRpcError {
                code: -32600,
                message: "Invalid Request".to_string(),
                data: None,
            }),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["error"]["code"], -32600);
        let deser: JsonRpcResponse = serde_json::from_value(json).unwrap();
        assert_eq!(deser, resp);
    }

    #[test]
    fn test_serialize_jsonrpc_notification() {
        let notif = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "textDocument/publishDiagnostics".to_string(),
            params: Some(json!({"uri": "file:///test.rs", "diagnostics": []})),
        };
        let json = serde_json::to_value(&notif).unwrap();
        assert_eq!(json["method"], "textDocument/publishDiagnostics");
        let deser: JsonRpcNotification = serde_json::from_value(json).unwrap();
        assert_eq!(deser, notif);
    }

    #[test]
    fn test_serialize_text_edit() {
        let edit = TextEdit {
            range: Range {
                start: Position { line: 5, character: 0 },
                end: Position { line: 5, character: 6 },
            },
            new_text: "Settings".to_string(),
        };
        let json = serde_json::to_value(&edit).unwrap();
        assert_eq!(json["newText"], "Settings");
        let deser: TextEdit = serde_json::from_value(json).unwrap();
        assert_eq!(deser, edit);
    }

    #[test]
    fn test_serialize_workspace_edit_changes_form() {
        let mut changes = std::collections::HashMap::new();
        changes.insert(
            "file:///src/main.rs".to_string(),
            vec![TextEdit {
                range: Range {
                    start: Position { line: 1, character: 4 },
                    end: Position { line: 1, character: 10 },
                },
                new_text: "Settings".to_string(),
            }],
        );
        let edit = WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
        };
        let json = serde_json::to_value(&edit).unwrap();
        assert!(json["changes"]["file:///src/main.rs"].is_array());
        let deser: WorkspaceEdit = serde_json::from_value(json).unwrap();
        assert_eq!(deser, edit);
    }

    #[test]
    fn test_serialize_workspace_edit_document_changes_form() {
        let edit = WorkspaceEdit {
            changes: None,
            document_changes: Some(vec![
                DocumentChange::Edit(TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier {
                        uri: "file:///src/main.rs".to_string(),
                        version: Some(1),
                    },
                    edits: vec![TextEdit {
                        range: Range {
                            start: Position { line: 0, character: 0 },
                            end: Position { line: 0, character: 6 },
                        },
                        new_text: "Settings".to_string(),
                    }],
                }),
                DocumentChange::Operation(ResourceOperation::Create {
                    uri: "file:///src/new.rs".to_string(),
                }),
            ]),
        };
        let json = serde_json::to_value(&edit).unwrap();
        assert!(json["documentChanges"].is_array());
        assert_eq!(json["documentChanges"].as_array().unwrap().len(), 2);
        let deser: WorkspaceEdit = serde_json::from_value(json).unwrap();
        assert_eq!(deser, edit);
    }

    #[test]
    fn test_serialize_resource_operation() {
        let create = ResourceOperation::Create { uri: "file:///new.rs".to_string() };
        let json = serde_json::to_value(&create).unwrap();
        assert_eq!(json["kind"], "create");
        assert_eq!(json["uri"], "file:///new.rs");
        let deser: ResourceOperation = serde_json::from_value(json).unwrap();
        assert_eq!(deser, create);

        let rename = ResourceOperation::Rename {
            old_uri: "file:///old.rs".to_string(),
            new_uri: "file:///new.rs".to_string(),
        };
        let json = serde_json::to_value(&rename).unwrap();
        assert_eq!(json["kind"], "rename");
        let deser: ResourceOperation = serde_json::from_value(json).unwrap();
        assert_eq!(deser, rename);

        let delete = ResourceOperation::Delete { uri: "file:///old.rs".to_string() };
        let json = serde_json::to_value(&delete).unwrap();
        assert_eq!(json["kind"], "delete");
        let deser: ResourceOperation = serde_json::from_value(json).unwrap();
        assert_eq!(deser, delete);
    }

    #[test]
    fn test_serialize_code_action() {
        let action = CodeAction {
            title: "Extract function".to_string(),
            kind: Some("refactor.extract.function".to_string()),
            edit: Some(WorkspaceEdit::default()),
            is_preferred: Some(true),
        };
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(json["title"], "Extract function");
        assert_eq!(json["kind"], "refactor.extract.function");
        assert_eq!(json["isPreferred"], true);
        let deser: CodeAction = serde_json::from_value(json).unwrap();
        assert_eq!(deser, action);
    }

    #[test]
    fn test_server_capabilities_partial() {
        let caps = ServerCapabilities {
            definition_provider: Some(true),
            ..Default::default()
        };
        let json = serde_json::to_value(&caps).unwrap();
        assert_eq!(json["definitionProvider"], true);
        assert!(json["hoverProvider"].is_null());
        let deser: ServerCapabilities = serde_json::from_value(json).unwrap();
        assert_eq!(deser, caps);
    }
}
