// FCP core parsed operation — ported from Go (fcp-terraform)

use std::collections::HashMap;

use super::tokenizer::{is_key_value, is_selector, parse_key_value_with_meta, tokenize};

/// A successfully parsed FCP operation.
#[derive(Debug, Clone)]
pub struct ParsedOp {
    pub verb: String,
    pub positionals: Vec<String>,
    pub params: HashMap<String, String>,
    pub selectors: Vec<String>,
    #[allow(dead_code)]
    pub quoted_params: HashMap<String, bool>,
    #[allow(dead_code)]
    pub raw: String,
}

/// An error from parsing an FCP operation.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub error: String,
    #[allow(dead_code)]
    pub raw: String,
}

/// The result of parsing an FCP operation string.
pub type ParseResult = Result<ParsedOp, ParseError>;

/// Parses an FCP operation string into a structured ParsedOp.
pub fn parse_op(input: &str) -> ParseResult {
    let raw = input.trim().to_string();
    let tokens = tokenize(&raw);

    if tokens.is_empty() {
        return Err(ParseError {
            error: "empty operation".to_string(),
            raw,
        });
    }

    let verb = tokens[0].to_lowercase();
    let mut positionals = Vec::new();
    let mut params = HashMap::new();
    let mut selectors = Vec::new();
    let mut quoted_params = HashMap::new();

    for token in &tokens[1..] {
        if is_selector(token) {
            selectors.push(token.clone());
        } else if is_key_value(token) {
            let (key, value, was_quoted) = parse_key_value_with_meta(token);
            params.insert(key.clone(), value);
            if was_quoted {
                quoted_params.insert(key, true);
            }
        } else {
            positionals.push(token.clone());
        }
    }

    Ok(ParsedOp {
        verb,
        positionals,
        params,
        selectors,
        quoted_params,
        raw,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_op_simple_verb() {
        let r = parse_op("add svc AuthService").unwrap();
        assert_eq!(r.verb, "add");
        assert_eq!(r.positionals, vec!["svc", "AuthService"]);
        assert!(r.params.is_empty());
        assert!(r.selectors.is_empty());
    }

    #[test]
    fn test_parse_op_key_value_params() {
        let r = parse_op("add svc AuthService theme:blue near:Gateway").unwrap();
        assert_eq!(r.verb, "add");
        assert_eq!(r.positionals, vec!["svc", "AuthService"]);
        assert_eq!(r.params["theme"], "blue");
        assert_eq!(r.params["near"], "Gateway");
    }

    #[test]
    fn test_parse_op_selectors() {
        let r = parse_op("remove @type:db @recent:3").unwrap();
        assert_eq!(r.verb, "remove");
        assert_eq!(r.selectors, vec!["@type:db", "@recent:3"]);
        assert!(r.positionals.is_empty());
    }

    #[test]
    fn test_parse_op_mixed_types() {
        let r = parse_op("style @type:svc fill:#ff0000 bold").unwrap();
        assert_eq!(r.verb, "style");
        assert_eq!(r.selectors, vec!["@type:svc"]);
        assert_eq!(r.params["fill"], "#ff0000");
        assert_eq!(r.positionals, vec!["bold"]);
    }

    #[test]
    fn test_parse_op_lowercases_verb() {
        let r = parse_op("ADD svc Test").unwrap();
        assert_eq!(r.verb, "add");
    }

    #[test]
    fn test_parse_op_preserves_raw() {
        let r = parse_op("  add svc Test  ").unwrap();
        assert_eq!(r.raw, "add svc Test");
    }

    #[test]
    fn test_parse_op_quoted_positionals() {
        let r = parse_op(r#"label Gateway "API Gateway v2""#).unwrap();
        assert_eq!(r.positionals, vec!["Gateway", "API Gateway v2"]);
    }

    #[test]
    fn test_parse_op_empty_input() {
        let r = parse_op("");
        assert!(r.is_err());
        let e = r.unwrap_err();
        assert_eq!(e.error, "empty operation");
    }

    #[test]
    fn test_parse_op_whitespace_only() {
        let r = parse_op("   ");
        assert!(r.is_err());
    }

    #[test]
    fn test_parse_op_arrows_as_positionals() {
        let r = parse_op("connect A -> B").unwrap();
        assert_eq!(r.positionals, vec!["A", "->", "B"]);
    }

    #[test]
    fn test_parse_op_verb_only() {
        let r = parse_op("undo").unwrap();
        assert_eq!(r.verb, "undo");
        assert!(r.positionals.is_empty());
        assert!(r.params.is_empty());
        assert!(r.selectors.is_empty());
    }

    #[test]
    fn test_parse_op_terraform_style() {
        let r = parse_op("add resource aws_instance web ami:ami-123 instance_type:t2.micro")
            .unwrap();
        assert_eq!(r.verb, "add");
        assert_eq!(r.positionals, vec!["resource", "aws_instance", "web"]);
        assert_eq!(r.params["ami"], "ami-123");
        assert_eq!(r.params["instance_type"], "t2.micro");
    }
}
