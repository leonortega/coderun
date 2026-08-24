use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Dependency graph derived from imports (codebase-memory-mcp style, AST + regex fallback)
/// Spec §3 Repository Intelligence — produce symbols, dependency graph, entry points.
#[derive(Debug, Default)]
pub struct DependencyGraph {
    /// adjacency: file -> Vec<dep_file>
    edges: HashMap<String, Vec<String>>,
    /// reverse edges for impact analysis
    reverse: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build graph from file list by scanning imports
    pub fn build_from_files(repo_root: &Path, files: &[PathBuf]) -> Self {
        let mut graph = Self::new();
        for file in files {
            let rel = file.strip_prefix(repo_root).unwrap_or(file).to_string_lossy().to_string();
            if let Ok(content) = std::fs::read_to_string(file) {
                let deps = extract_imports(&content, &rel, repo_root);
                for dep in deps {
                    graph.add_edge(rel.clone(), dep);
                }
            }
        }
        graph
    }

    pub fn add_edge(&mut self, from: String, to: String) {
        self.edges.entry(from.clone()).or_default().push(to.clone());
        self.reverse.entry(to).or_default().push(from);
    }

    pub fn dependencies_of(&self, file: &str) -> Vec<String> {
        self.edges.get(file).cloned().unwrap_or_default()
    }

    pub fn dependents_of(&self, file: &str) -> Vec<String> {
        self.reverse.get(file).cloned().unwrap_or_default()
    }

    /// Impact analysis: transitive dependents of changed file
    pub fn impact_analysis(&self, changed: &str) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut queue = vec![changed.to_string()];
        let mut result = Vec::new();
        while let Some(cur) = queue.pop() {
            for dep in self.dependents_of(&cur) {
                if visited.insert(dep.clone()) {
                    result.push(dep.clone());
                    queue.push(dep);
                }
            }
        }
        result
    }

    pub fn edge_count(&self) -> usize {
        self.edges.values().map(|v| v.len()).sum()
    }
}

/// Extract imports via simple regex (fallback when tree-sitter grammar not loaded)
fn extract_imports(content: &str, _current_file: &str, _repo_root: &Path) -> Vec<String> {
    let mut deps = Vec::new();
    // Rust: use crate::foo::bar, mod foo
    // JS/TS: import ... from "path", require("path")
    // Python: import foo, from foo import bar
    // Simpler line scan (regex retained for future ast-grep upgrade)
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("use ") || trimmed.starts_with("mod ") {
            if let Some(rest) = trimmed.strip_prefix("use ") {
                let clean = rest.split(';').next().unwrap_or("").trim().split(" as ").next().unwrap_or("").trim().to_string();
                let parts: Vec<&str> = clean.split("::").collect();
                // Pick first meaningful segment not crate/self/super/std/core
                let mut dep_part = "";
                for p in &parts {
                    if !["crate", "self", "super", "std", "core"].contains(p) && !p.is_empty() {
                        dep_part = p;
                        break;
                    }
                }
                if dep_part.is_empty() { dep_part = parts.first().copied().unwrap_or(""); }
                if !dep_part.is_empty() && !["std","core"].contains(&dep_part) {
                    deps.push(format!("{}.rs", dep_part));
                    // Also push full path as fallback for test matching graph
                    let full = parts.join("/");
                    if full.contains('/') && !deps.contains(&format!("{}.rs", full)) {
                        // add alternative
                        deps.push(full);
                    }
                }
            }
        } else if trimmed.starts_with("import ") || trimmed.contains(" from ") {
            // extract quoted path
            if let Some(start) = trimmed.find('"').or_else(|| trimmed.find('\'')) {
                let quote = trimmed.chars().nth(trimmed.find('"').unwrap_or(trimmed.find('\'').unwrap_or(0))).unwrap_or('"');
                let rest = &trimmed[start+1..];
                if let Some(end) = rest.find(quote) {
                    let path = &rest[..end];
                    if !path.is_empty() {
                        deps.push(path.to_string());
                    }
                }
            }
        }
    }
    // Deduplicate
    deps.sort();
    deps.dedup();
    deps
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_extract_imports_rust() {
        let content = "use crate::graph::DependencyGraph;\nuse std::collections::HashMap;\nmod foo;";
        let deps = extract_imports(content, "src/main.rs", &PathBuf::from("."));
        assert!(deps.iter().any(|d| d.contains("graph")));
    }

    #[test]
    fn test_graph_edges_and_impact() {
        let mut g = DependencyGraph::new();
        g.add_edge("a.rs".to_string(), "b.rs".to_string());
        g.add_edge("b.rs".to_string(), "c.rs".to_string());
        assert_eq!(g.edge_count(), 2);
        assert_eq!(g.dependencies_of("a.rs"), vec!["b.rs"]);
        let impact = g.impact_analysis("c.rs");
        // c's dependents are b -> a
        assert!(impact.contains(&"b.rs".to_string()));
        assert!(impact.contains(&"a.rs".to_string()));
    }

    #[test]
    fn test_build_from_files_tmp() {
        let dir = std::env::temp_dir().join(format!("coderun_graph_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_a = dir.join("a.rs");
        let file_b = dir.join("b.rs");
        std::fs::write(&file_a, "use crate::b::Foo;\nfn main() {}").unwrap();
        std::fs::write(&file_b, "pub struct Foo;").unwrap();
        let graph = DependencyGraph::build_from_files(&dir, &[file_a.clone(), file_b.clone()]);
        assert!(graph.edge_count() >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
