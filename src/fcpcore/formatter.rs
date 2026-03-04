// FCP core formatter — ported from Go (fcp-terraform)

/// Formats a result with prefix convention.
/// If success is false, returns "ERROR: message" (prefix is ignored).
/// If success is true and prefix is given, returns "prefix message".
/// Otherwise returns just the message.
///
/// Prefix conventions:
///   + created
///     ~ modified (edge/connection)
///   * changed (property)
///   - removed
///     ! meta/group operation
///     @ bulk/layout operation
#[allow(dead_code)]
pub fn format_result(success: bool, message: &str, prefix: Option<&str>) -> String {
    if !success {
        return format!("ERROR: {}", message);
    }
    match prefix {
        Some(p) if !p.is_empty() => format!("{} {}", p, message),
        _ => message.to_string(),
    }
}

/// Finds the closest candidate for a misspelled input using Levenshtein distance.
/// Returns None if no candidate is close enough (distance > 3).
pub fn suggest(input: &str, candidates: &[&str]) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }

    let input_lower = input.to_lowercase();
    let mut best: Option<&str> = None;
    let mut best_dist = 999;

    for &candidate in candidates {
        let dist = levenshtein(&input_lower, &candidate.to_lowercase());
        if dist < best_dist {
            best_dist = dist;
            best = Some(candidate);
        }
    }

    if best_dist <= 3 {
        best.map(|s| s.to_string())
    } else {
        None
    }
}

/// Computes the Levenshtein distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();

    let mut prev: Vec<usize> = (0..=n).collect();

    for i in 1..=m {
        let mut prev_diag = prev[0];
        prev[0] = i;
        #[allow(clippy::needless_range_loop)]
        for j in 1..=n {
            let temp = prev[j];
            if a_bytes[i - 1] == b_bytes[j - 1] {
                prev[j] = prev_diag;
            } else {
                let min_val = prev_diag.min(prev[j - 1]).min(prev[j]);
                prev[j] = 1 + min_val;
            }
            prev_diag = temp;
        }
    }

    prev[n]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_result_success_with_prefix() {
        let got = format_result(true, "svc AuthService", Some("+"));
        assert_eq!(got, "+ svc AuthService");
    }

    #[test]
    fn test_format_result_success_without_prefix() {
        let got = format_result(true, "done", None);
        assert_eq!(got, "done");
    }

    #[test]
    fn test_format_result_error() {
        let got = format_result(false, "something broke", None);
        assert_eq!(got, "ERROR: something broke");
    }

    #[test]
    fn test_format_result_error_ignores_prefix() {
        let got = format_result(false, "bad input", Some("+"));
        assert_eq!(got, "ERROR: bad input");
    }

    #[test]
    fn test_format_result_prefix_conventions() {
        let tests = vec![
            ("+", "AuthService", "+ AuthService"),
            ("~", "edge A->B", "~ edge A->B"),
            ("*", "styled A", "* styled A"),
            ("-", "A", "- A"),
            ("!", "group Backend", "! group Backend"),
            ("@", "layout", "@ layout"),
        ];
        for (prefix, message, want) in tests {
            let got = format_result(true, message, Some(prefix));
            assert_eq!(got, want, "format_result({:?}, {:?})", prefix, message);
        }
    }

    #[test]
    fn test_suggest() {
        let candidates = vec!["add", "remove", "connect", "style", "label", "badge"];

        let tests = vec![
            ("add", Some("add")),
            ("ad", Some("add")),
            ("styel", Some("style")),
            ("labek", Some("label")),
            ("zzzzzzz", None),
            ("bade", Some("badge")),
        ];

        for (input, want) in tests {
            let got = suggest(input, &candidates);
            assert_eq!(
                got,
                want.map(|s| s.to_string()),
                "suggest({:?})",
                input
            );
        }
    }

    #[test]
    fn test_suggest_empty_candidates() {
        let got = suggest("test", &[]);
        assert_eq!(got, None);
    }
}
