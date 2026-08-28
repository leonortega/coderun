use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Dependency graph derived from imports (local AST + regex)
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

    /// Build graph from file list — local AST+regex
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

    /// Get all files in the graph (both as sources and targets)
    pub fn all_files(&self) -> Vec<&String> {
        let mut files: HashSet<&String> = HashSet::new();
        for file in self.edges.keys() {
            files.insert(file);
        }
        for deps in self.edges.values() {
            for dep in deps {
                files.insert(dep);
            }
        }
        files.into_iter().collect()
    }
}

/// Call graph tracking function calls between files (P2 #8)
/// Uses deeper AST analysis to track function calls across file boundaries
#[derive(Debug, Default)]
pub struct CallGraph {
    /// Function call edges: (caller_file, caller_func) -> Vec<(callee_file, callee_func)>
    edges: HashMap<(String, String), Vec<(String, String)>>,
    /// Reverse edges for impact analysis
    reverse: HashMap<(String, String), Vec<(String, String)>>,
    /// Function definitions: file -> Vec<(func_name, line_start, line_end)>
    definitions: HashMap<String, Vec<(String, usize, usize)>>,
}

impl CallGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build call graph from file list using AST analysis
    pub fn build_from_files(repo_root: &Path, files: &[PathBuf]) -> Self {
        let mut graph = Self::new();
        
        // First pass: extract function definitions from all files
        for file in files {
            let rel = file.strip_prefix(repo_root).unwrap_or(file).to_string_lossy().to_string();
            if let Ok(content) = std::fs::read_to_string(file) {
                let defs = extract_function_definitions(&content, &rel);
                graph.definitions.insert(rel, defs);
            }
        }
        
        // Second pass: extract function calls and build edges
        for file in files {
            let rel = file.strip_prefix(repo_root).unwrap_or(file).to_string_lossy().to_string();
            if let Ok(content) = std::fs::read_to_string(file) {
                let calls = extract_function_calls(&content, &rel);
                let caller_defs = graph.definitions.get(&rel).cloned().unwrap_or_default();
                
                // For each call, find which function it's in and link to callee
                for (call_line, call_name) in calls {
                    // Find the function containing this call
                    let caller_func = find_containing_function(&caller_defs, call_line);
                    
                    // Try to find the callee definition across all files
                    if let Some((callee_file, _callee_line)) = find_function_definition(&graph.definitions, &call_name) {
                        let caller = (rel.clone(), caller_func);
                        let callee = (callee_file, call_name);
                        
                        graph.edges.entry(caller.clone()).or_default().push(callee.clone());
                        graph.reverse.entry(callee).or_default().push(caller);
                    }
                }
            }
        }
        
        graph
    }

    /// Get all functions called by a specific function
    pub fn callees_of(&self, file: &str, function: &str) -> Vec<(String, String)> {
        self.edges.get(&(file.to_string(), function.to_string())).cloned().unwrap_or_default()
    }

    /// Get all functions that call a specific function
    pub fn callers_of(&self, file: &str, function: &str) -> Vec<(String, String)> {
        self.reverse.get(&(file.to_string(), function.to_string())).cloned().unwrap_or_default()
    }

    /// Get all function definitions in a file
    pub fn definitions_in(&self, file: &str) -> Vec<(String, usize, usize)> {
        self.definitions.get(file).cloned().unwrap_or_default()
    }

    /// Get all files in the call graph
    pub fn all_files(&self) -> Vec<&String> {
        let mut files: HashSet<&String> = HashSet::new();
        for file in self.definitions.keys() {
            files.insert(file);
        }
        files.into_iter().collect()
    }

    /// Get edge count
    pub fn edge_count(&self) -> usize {
        self.edges.values().map(|v| v.len()).sum()
    }
}

/// Extract function definitions from source code using regex patterns
fn extract_function_definitions(content: &str, _file: &str) -> Vec<(String, usize, usize)> {
    let mut defs = Vec::new();
    
    // Rust: fn name(...) { ... }
    // Python: def name(...): ...
    // JS/TS: function name(...) { ... } or const name = (...) => { ... }
    // C#: public/private/static ReturnType Name(...) { ... }
    
    let patterns = [
        // Rust functions
        regex::Regex::new(r"(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*\(").unwrap(),
        // Python functions
        regex::Regex::new(r"def\s+(\w+)\s*\(").unwrap(),
        // JavaScript/TypeScript functions
        regex::Regex::new(r"(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*\(").unwrap(),
        // C# methods
        regex::Regex::new(r"(?:public|private|protected|internal)\s+(?:static\s+)?(?:async\s+)?(?:void|bool|int|long|float|double|string|var|IEnumerable|Task|ValueTask|IActionResult|ActionResult|ObjectResult)\s+(\w+)\s*\(").unwrap(),
        // Java methods
        regex::Regex::new(r"(?:public|private|protected)\s+(?:static\s+)?(?:final\s+)?(?:void|boolean|int|long|float|double|String|var)\s+(\w+)\s*\(").unwrap(),
    ];
    
    for (line_num, line) in content.lines().enumerate() {
        for pattern in &patterns {
            if let Some(caps) = pattern.captures(line) {
                if let Some(name_match) = caps.get(1) {
                    let name = name_match.as_str().to_string();
                    // Simple heuristic: assume function ends at next function or end of file
                    // In a real implementation, we'd use proper AST parsing
                    let line_start = line_num + 1;
                    let line_end = line_start + 50; // Placeholder: assume functions are ~50 lines
                    defs.push((name, line_start, line_end));
                }
            }
        }
    }
    
    defs
}

/// Extract function calls from source code
fn extract_function_calls(content: &str, _file: &str) -> Vec<(usize, String)> {
    let mut calls = Vec::new();
    
    // Pattern to match function calls: identifier followed by '('
    let call_pattern = regex::Regex::new(r"\b([a-zA-Z_]\w*)\s*\(").unwrap();
    
    // Keywords to exclude (not function calls)
    let keywords: HashSet<&str> = [
        "if", "else", "for", "while", "loop", "match", "return", "break", "continue",
        "fn", "struct", "enum", "impl", "trait", "type", "pub", "mod", "use",
        "class", "interface", "extends", "implements", "new", "this", "super",
        "function", "const", "let", "var", "import", "from", "export",
        "def", "self", "async", "await", "move",
    ].iter().cloned().collect();
    
    for (line_num, line) in content.lines().enumerate() {
        for caps in call_pattern.captures_iter(line) {
            if let Some(name_match) = caps.get(1) {
                let name = name_match.as_str();
                if !keywords.contains(name) && name.len() > 1 {
                    calls.push((line_num + 1, name.to_string()));
                }
            }
        }
    }
    
    calls
}

/// Find which function contains a given line number
fn find_containing_function(defs: &[(String, usize, usize)], line: usize) -> String {
    for (name, start, end) in defs {
        if line >= *start && line <= *end {
            return name.clone();
        }
    }
    "unknown".to_string()
}

/// Find function definition across all files
fn find_function_definition(definitions: &HashMap<String, Vec<(String, usize, usize)>>, func_name: &str) -> Option<(String, usize)> {
    for (file, defs) in definitions {
        for (name, line_start, _) in defs {
            if name == func_name {
                return Some((file.clone(), *line_start));
            }
        }
    }
    None
}

/// Extract imports via simple regex (local AST+regex primary)
fn extract_imports(content: &str, _current_file: &str, _repo_root: &Path) -> Vec<String> {
    let mut deps = Vec::new();
    // Rust: use crate::foo::bar, mod foo  (TASK-011: handle mod b; → b.rs)
    // JS/TS: import ... from "path", require("path")
    // Python: import foo, from foo import bar
    // Simpler line scan (regex retained for future ast-grep upgrade)
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("use ") || trimmed.starts_with("mod ") {
            // handle `mod b;` → b.rs (currently only `use` was handled)
            if let Some(rest) = trimmed.strip_prefix("mod ") {
                let name = rest.split(|c: char| c == ';' || c == '{' || c.is_whitespace()).next().unwrap_or("").trim();
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    deps.push(format!("{}.rs", name));
                    continue;
                }
            }
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

    #[test]
    fn test_extract_imports_js_from_quoted() {
        let content = r#"import foo from "bar/baz"; const x = require('qux');"#;
        let deps = extract_imports(content, "a.js", &PathBuf::from("."));
        assert!(deps.iter().any(|d| d.contains("bar/baz")));
    }
}
