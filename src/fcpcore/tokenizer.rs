// FCP core tokenizer — ported from Go (fcp-terraform)

/// Tokenize splits an FCP operation string into tokens, handling quoted strings,
/// escape sequences, and embedded newlines.
pub fn tokenize(input: &str) -> Vec<String> {
    let bytes = input.as_bytes();
    let n = bytes.len();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < n {
        // Skip spaces
        while i < n && bytes[i] == b' ' {
            i += 1;
        }
        if i >= n {
            break;
        }

        if bytes[i] == b'"' {
            // Fully quoted token
            i += 1;
            let mut token = String::new();
            while i < n && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < n {
                    let next = bytes[i + 1];
                    if next == b'n' {
                        token.push('\n');
                        i += 2;
                    } else {
                        i += 1;
                        token.push(bytes[i] as char);
                        i += 1;
                    }
                } else {
                    token.push(bytes[i] as char);
                    i += 1;
                }
            }
            if i < n {
                i += 1; // skip closing quote
            }
            tokens.push(token);
        } else {
            // Unquoted token (may contain embedded quoted sections)
            let mut token = String::new();
            while i < n && bytes[i] != b' ' {
                if bytes[i] == b'"' {
                    token.push('"');
                    i += 1;
                    while i < n && bytes[i] != b'"' {
                        if bytes[i] == b'\\' && i + 1 < n {
                            let next = bytes[i + 1];
                            if next == b'n' {
                                token.push('\n');
                                i += 2;
                            } else {
                                i += 1;
                                token.push(bytes[i] as char);
                                i += 1;
                            }
                        } else {
                            token.push(bytes[i] as char);
                            i += 1;
                        }
                    }
                    if i < n {
                        token.push('"');
                        i += 1;
                    }
                } else {
                    token.push(bytes[i] as char);
                    i += 1;
                }
            }
            tokens.push(token.replace("\\n", "\n"));
        }
    }

    tokens
}

/// Returns true if the token is a key:value pair (not a selector, not an arrow).
pub fn is_key_value(token: &str) -> bool {
    if token.starts_with('@') {
        return false;
    }
    if is_arrow(token) {
        return false;
    }
    let idx = match token.find(':') {
        Some(i) => i,
        None => return false,
    };
    if idx == 0 || idx >= token.len() - 1 {
        return false;
    }
    let key = &token[..idx];
    for ch in key.chars() {
        if !matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-') {
            return false;
        }
    }
    true
}

/// Parses a key:value token into its key and value parts.
/// If the value is wrapped in double quotes, they are stripped.
#[allow(dead_code)]
pub fn parse_key_value(token: &str) -> (String, String) {
    let idx = token.find(':').unwrap();
    let key = token[..idx].to_string();
    let mut value = token[idx + 1..].to_string();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value = value[1..value.len() - 1].to_string();
    }
    (key, value)
}

/// Parses a key:value token, also returning whether the value was quoted.
pub fn parse_key_value_with_meta(token: &str) -> (String, String, bool) {
    let idx = token.find(':').unwrap();
    let key = token[..idx].to_string();
    let raw_value = &token[idx + 1..];
    let mut was_quoted = false;
    let value;
    if raw_value.len() >= 2 && raw_value.starts_with('"') && raw_value.ends_with('"') {
        value = raw_value[1..raw_value.len() - 1].to_string();
        was_quoted = true;
    } else {
        value = raw_value.to_string();
    }
    (key, value, was_quoted)
}

/// Returns true if the token is an arrow operator: ->, <->, or --
pub fn is_arrow(token: &str) -> bool {
    token == "->" || token == "<->" || token == "--"
}

/// Returns true if the token is a selector (starts with @).
pub fn is_selector(token: &str) -> bool {
    token.starts_with('@')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple_tokens() {
        let got = tokenize("add svc AuthService");
        assert_eq!(got, vec!["add", "svc", "AuthService"]);
    }

    #[test]
    fn test_tokenize_quoted_strings() {
        let got = tokenize(r#"add svc "Auth Service" theme:blue"#);
        assert_eq!(got, vec!["add", "svc", "Auth Service", "theme:blue"]);
    }

    #[test]
    fn test_tokenize_escaped_quotes() {
        let got = tokenize(r#"label A "say \"hello\"""#);
        assert_eq!(got, vec!["label", "A", r#"say "hello""#]);
    }

    #[test]
    fn test_tokenize_empty_input() {
        let got = tokenize("");
        assert!(got.is_empty());
    }

    #[test]
    fn test_tokenize_whitespace_only() {
        let got = tokenize("   ");
        assert!(got.is_empty());
    }

    #[test]
    fn test_tokenize_multiple_spaces() {
        let got = tokenize("add   svc   A");
        assert_eq!(got, vec!["add", "svc", "A"]);
    }

    #[test]
    fn test_tokenize_newline_in_unquoted() {
        let got = tokenize(r"add svc Container\nRegistry");
        assert_eq!(got, vec!["add", "svc", "Container\nRegistry"]);
    }

    #[test]
    fn test_tokenize_newline_in_quoted() {
        let got = tokenize(r#"add svc "Container\nRegistry""#);
        assert_eq!(got, vec!["add", "svc", "Container\nRegistry"]);
    }

    #[test]
    fn test_tokenize_embedded_quoted_value() {
        let got = tokenize(r#"label:"Line1\nLine2""#);
        assert_eq!(got, vec!["label:\"Line1\nLine2\""]);
    }

    #[test]
    fn test_tokenize_multiple_newlines() {
        let got = tokenize(r"add svc A\nB\nC");
        assert_eq!(got, vec!["add", "svc", "A\nB\nC"]);
    }

    #[test]
    fn test_tokenize_single_token() {
        let got = tokenize("add");
        assert_eq!(got, vec!["add"]);
    }

    #[test]
    fn test_tokenize_empty_quoted_string() {
        let got = tokenize(r#""""#);
        assert_eq!(got, vec![""]);
    }

    #[test]
    fn test_tokenize_unclosed_quote() {
        let got = tokenize(r#""hello world"#);
        assert_eq!(got, vec!["hello world"]);
    }

    #[test]
    fn test_tokenize_escaped_backslash() {
        let got = tokenize(r#""path\\dir""#);
        assert_eq!(got, vec![r"path\dir"]);
    }

    #[test]
    fn test_tokenize_colons_in_value() {
        let got = tokenize("url:http://example.com");
        assert_eq!(got, vec!["url:http://example.com"]);
    }

    #[test]
    fn test_is_key_value() {
        let tests = vec![
            ("theme:blue", true),
            ("url:http://x", true),
            ("@type:db", false),
            ("->", false),
            ("hello", false),
            ("key:", false),
            (":value", false),
        ];
        for (input, want) in tests {
            assert_eq!(
                is_key_value(input),
                want,
                "is_key_value({:?}) = {}, want {}",
                input,
                is_key_value(input),
                want
            );
        }
    }

    #[test]
    fn test_parse_key_value() {
        let (key, value) = parse_key_value("theme:blue");
        assert_eq!(key, "theme");
        assert_eq!(value, "blue");

        let (key, value) = parse_key_value("url:http://x:8080");
        assert_eq!(key, "url");
        assert_eq!(value, "http://x:8080");
    }

    #[test]
    fn test_parse_key_value_strips_quotes() {
        let (key, value) = parse_key_value("label:\"Line1\nLine2\"");
        assert_eq!(key, "label");
        assert_eq!(value, "Line1\nLine2");
    }

    #[test]
    fn test_parse_key_value_with_meta() {
        let (key, value, was_quoted) = parse_key_value_with_meta(r#"engine_version:"15""#);
        assert_eq!(key, "engine_version");
        assert_eq!(value, "15");
        assert!(was_quoted);

        let (key, value, was_quoted) = parse_key_value_with_meta("port:80");
        assert_eq!(key, "port");
        assert_eq!(value, "80");
        assert!(!was_quoted);
    }

    #[test]
    fn test_is_arrow() {
        assert!(is_arrow("->"));
        assert!(is_arrow("<->"));
        assert!(is_arrow("--"));
        assert!(!is_arrow("=>"));
        assert!(!is_arrow("add"));
    }

    #[test]
    fn test_is_selector() {
        assert!(is_selector("@type:db"));
        assert!(is_selector("@all"));
        assert!(!is_selector("type:db"));
        assert!(!is_selector("add"));
    }
}
