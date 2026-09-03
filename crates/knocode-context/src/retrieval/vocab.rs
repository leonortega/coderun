//! Bounded deterministic vocabulary — fixes lexical mismatch without giant OR.
//! Example: `add package` ↔ `create package` for `README.md: #### Create a new package`.

use std::collections::HashSet;

/// Expand a single term into bounded synonyms (including itself).
/// Keep small — at most 3 per term to avoid BM25 dilution.
fn synonyms_for(term: &str) -> Vec<String> {
    match term.to_lowercase().as_str() {
        "add" => vec!["add".into(), "create".into(), "new".into()],
        "create" => vec!["create".into(), "add".into(), "new".into()],
        "new" => vec!["new".into(), "create".into(), "add".into()],
        "make" => vec!["make".into(), "create".into(), "add".into()],
        "package" => vec!["package".into(), "workspace".into()],
        "packages" => vec!["packages".into(), "package".into(), "workspace".into()],
        "workspace" => vec!["workspace".into(), "package".into()],
        "install" => vec!["install".into(), "add".into(), "setup".into()],
        "setup" | "set" => vec!["setup".into(), "install".into(), "configure".into()],
        "how" | "do" | "i" => vec![], // filtered anyway — no expansion
        _ => vec![],
    }
}

/// Expand query terms with bounded synonyms.
/// `tokens` are already lowercased, stop-word filtered.
/// Returns expanded set (original + synonyms) deduped.
pub fn expand_terms(tokens: &[String]) -> Vec<String> {
    let mut out: HashSet<String> = HashSet::new();
    for t in tokens {
        out.insert(t.clone());
        for syn in synonyms_for(t) {
            if syn.len() >= 2 {
                out.insert(syn);
            }
        }
    }
    // Keep bounded: at most original 2x + synonyms, but cap 20 terms
    let mut v: Vec<String> = out.into_iter().collect();
    v.sort();
    v.truncate(20);
    v
}

/// Build an OR-joined expanded query string for Tantivy.
/// Uses original query + synonym expansion, but keeps it bounded (not huge OR).
pub fn expanded_query_string(original: &str, tokens: &[String]) -> String {
    if tokens.is_empty() {
        return original.to_string();
    }
    let expanded = expand_terms(tokens);
    // Tantivy sanitized query is OR-joined in `tantivy_index.rs:295`
    // We return space-separated and let sanitizer OR-join, or directly OR-join here.
    // To stay deterministic and avoid double OR, return space-joined and reuse sanitizer.
    // But for ripgrep fallback we need OR via `|`, so we return OR-joined for that path.
    // For now return OR-joined string for both (Tantivy parser accepts OR).
    if expanded.len() <= tokens.len() {
        original.to_string()
    } else {
        expanded.join(" OR ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_package_expands_to_create_new() {
        let toks = vec!["add".to_string(), "package".to_string()];
        let exp = expand_terms(&toks);
        assert!(exp.contains(&"add".to_string()));
        assert!(exp.contains(&"create".to_string()));
        assert!(exp.contains(&"new".to_string()));
        assert!(exp.contains(&"package".to_string()));
        assert!(exp.contains(&"workspace".to_string()));
    }

    #[test]
    fn bounded_no_explosion() {
        let toks = vec!["how".to_string(), "do".to_string(), "i".to_string(), "add".to_string(), "package".to_string()];
        let exp = expand_terms(&toks);
        assert!(exp.len() <= 20);
    }

    #[test]
    fn non_synonym_unchanged() {
        let toks = vec!["authentication".to_string()];
        let exp = expand_terms(&toks);
        assert_eq!(exp, vec!["authentication".to_string()]);
    }
}
