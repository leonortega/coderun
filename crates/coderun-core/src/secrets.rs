use regex::Regex;

/// Redact secrets before any outbound API call (spec §6 step 11, packaging & hardening)
/// Replaces patterns like `api_key: sk-...`, `secret=...`, `token ...` with `[REDACTED]`
pub fn redact_secrets(text: &str) -> String {
    // No backrefs — Rust regex crate doesn't support \2
    let patterns = [
        (Regex::new(r#"(?i)["']?api[_-]?key["']?\s*[:=]\s*["']?[A-Za-z0-9\-_\.]{8,}"#).unwrap(), "api_key=[REDACTED]"),
        (Regex::new(r#"(?i)["']?secret["']?\s*[:=]\s*["']?[A-Za-z0-9\-_\.]{8,}"#).unwrap(), "secret=[REDACTED]"),
        (Regex::new(r#"(?i)["']?token["']?\s*[:=]\s*["']?[A-Za-z0-9\-_\.]{8,}"#).unwrap(), "token=[REDACTED]"),
        (Regex::new(r#"sk-[A-Za-z0-9]{10,}"#).unwrap(), "[REDACTED]"),
        (Regex::new(r#"ghp_[A-Za-z0-9]{10,}"#).unwrap(), "[REDACTED]"),
    ];
    let mut out = text.to_string();
    for (re, rep) in &patterns {
        out = re.replace_all(&out, *rep).to_string();
    }
    out
}

/// Check whether text contains a secret (for logging WARN before redacting)
pub fn contains_secret(text: &str) -> bool {
    redact_secrets(text) != text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_api_key() {
        let input = r#"{"api_key": "sk-abc1234567890abcdef"}"#;
        let redacted = redact_secrets(input);
        assert!(redacted.contains("[REDACTED]"));
        assert!(!redacted.contains("sk-abc"));
    }

    #[test]
    fn test_redact_secret() {
        let input = "secret=supersecretvalue123";
        let redacted = redact_secrets(input);
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn test_no_false_positive() {
        let input = "hello world, implement auth feature";
        assert_eq!(redact_secrets(input), input);
        assert!(!contains_secret(input));
    }

    #[test]
    fn test_redact_token() {
        let input = "token: ghp_abc1234567890abcdef123456";
        let redacted = redact_secrets(input);
        assert!(redacted.contains("[REDACTED]"));
    }
}
