mod parser;
pub mod graph;
pub mod lsp;
pub mod watcher;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use coderun_core::{SearchResult, SearchResults};
use coderun_events::{EventBus, RuntimeEvent};
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::SearcherBuilder;
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

// ── Configuration ───────────────────────────────────────────────────────

// Note: Directory walking now uses the `ignore` crate which respects .gitignore patterns

/// Map of file extensions to language names
const EXTENSION_MAP: &[(&str, &str)] = &[
    ("rs", "rust"),
    ("ts", "typescript"),
    ("tsx", "typescript"),
    ("js", "javascript"),
    ("jsx", "javascript"),
    ("py", "python"),
    ("go", "go"),
    ("java", "java"),
    ("c", "c"),
    ("cpp", "cpp"),
    ("cc", "cpp"),
    ("cxx", "cpp"),
    ("h", "c"),
    ("hpp", "cpp"),
    ("cs", "csharp"),
    ("rb", "ruby"),
    ("php", "php"),
    ("swift", "swift"),
    ("kt", "kotlin"),
    ("scala", "scala"),
    ("r", "r"),
    ("lua", "lua"),
    ("zig", "zig"),
    ("nim", "nim"),
    ("ex", "elixir"),
    ("exs", "elixir"),
    ("erl", "erlang"),
    ("hs", "haskell"),
    ("ml", "ocaml"),
    ("clj", "clojure"),
    ("sql", "sql"),
    ("sh", "shell"),
    ("bash", "shell"),
    ("zsh", "shell"),
    ("yaml", "yaml"),
    ("yml", "yaml"),
    ("toml", "toml"),
    ("json", "json"),
    ("xml", "xml"),
    ("html", "html"),
    ("css", "css"),
    ("scss", "scss"),
    ("md", "markdown"),
    ("txt", "text"),
];

// ── Symbol Extraction Patterns ──────────────────────────────────────────

/// Regex patterns for extracting symbols from different languages
struct SymbolPatterns {
    function_pattern: regex::Regex,
    struct_pattern: regex::Regex,
    enum_pattern: regex::Regex,
    impl_pattern: regex::Regex,
    trait_pattern: regex::Regex,
    type_pattern: regex::Regex,
    #[allow(dead_code)]
    import_pattern: regex::Regex,
}

impl SymbolPatterns {
    fn new() -> Self {
        Self {
            // Rust: fn name, Python: def name, JS/TS: function name / const name = () =>
            function_pattern: regex::Regex::new(
                r"(?m)^(?:pub\s+)?(?:async\s+)?fn\s+(\w+)|^def\s+(\w+)|^(?:export\s+)?(?:async\s+)?function\s+(\w+)|^(?:pub\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s+)?\(|^(?:pub\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s+)?(?:function|\()"
            ).unwrap(),
            // struct ClassName, class ClassName
            struct_pattern: regex::Regex::new(
                r"(?m)^(?:pub\s+)?struct\s+(\w+)|^class\s+(\w+)|^(?:export\s+)?class\s+(\w+)"
            ).unwrap(),
            // enum EnumName
            enum_pattern: regex::Regex::new(
                r"(?m)^(?:pub\s+)?enum\s+(\w+)"
            ).unwrap(),
            // impl TypeName
            impl_pattern: regex::Regex::new(
                r"(?m)^impl(?:<[^>]*>)?\s+(\w+)"
            ).unwrap(),
            // trait TraitName
            trait_pattern: regex::Regex::new(
                r"(?m)^(?:pub\s+)?trait\s+(\w+)"
            ).unwrap(),
            // type Alias = Type
            type_pattern: regex::Regex::new(
                r"(?m)^(?:pub\s+)?type\s+(\w+)"
            ).unwrap(),
            // import/use/require statements
            import_pattern: regex::Regex::new(
                r"(?m)^(?:use|import|from|require|require_relative)\s+"
            ).unwrap(),
        }
    }
}

// ── Data Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: String,
    pub size: i64,
    pub language: Option<String>,
    pub symbol_count: usize,
    pub last_indexed_at: String,
}

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub line_start: i64,
    pub line_end: i64,
}

#[derive(Debug, Clone)]
pub struct IndexStats {
    pub files_indexed: usize,
    pub symbols_extracted: usize,
    pub files_skipped: usize,
    pub files_deleted: usize,
    pub duration_ms: u64,
}

// ── Repository Intelligence ─────────────────────────────────────────────

pub struct RepositoryIntelligence {
    repo_path: PathBuf,
    db: coderun_storage::Database,
    event_bus: EventBus,
    patterns: SymbolPatterns,
    #[allow(dead_code)]
    /// Cache of file hashes for quick change detection
    file_hashes: HashMap<String, String>,
}

impl RepositoryIntelligence {
    /// Create a new Repository Intelligence instance
    pub fn new(repo_path: PathBuf, db: coderun_storage::Database, event_bus: EventBus) -> Self {
        let patterns = SymbolPatterns::new();
        let file_hashes = HashMap::new();

        Self {
            repo_path,
            db,
            event_bus,
            patterns,
            file_hashes,
        }
    }

    /// Index the repository (full or incremental) — wires tantivy BM25 in-process, incremental
    pub fn index_repository(&mut self) -> Result<IndexStats, String> {
        let start = Instant::now();
        let mut files_indexed = 0;
        let mut symbols_extracted = 0;
        let mut files_skipped = 0;
        let mut files_deleted = 0;

        // Open tantivy index (MmapDirectory, memory-mapped per spec §3) — optional, never fails indexing
        let tantivy_index = coderun_storage::tantivy_index::TantivyIndex::open(&default_index_path()).ok();
        let mut tantivy_writer = tantivy_index.as_ref().and_then(|idx| idx.writer().ok());

        // Load existing file hashes from database
        let existing_files = self.db.get_all_files()?;
        let existing_hashes: HashMap<String, (i64, String)> = existing_files
            .into_iter()
            .enumerate()
            .map(|(i, (path, hash))| (path, (i as i64, hash)))
            .collect();

        let mut seen_paths = std::collections::HashSet::new();

        // Walk the directory tree
        for entry in self.walk_directory(&self.repo_path)? {
            let path = entry;
            let path_str = path.strip_prefix(&self.repo_path)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();

            seen_paths.insert(path_str.clone());

            // Check if file should be indexed
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let language = detect_language(ext);

            if language.is_none() && !is_indexable_text_file(ext) {
                files_skipped += 1;
                continue;
            }

            // Check if binary file
            if is_likely_binary(&path) {
                debug!(path = %path_str, "Skipping binary file");
                files_skipped += 1;
                continue;
            }

            // Compute content hash
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    warn!(path = %path_str, error = %e, "Failed to read file");
                    files_skipped += 1;
                    continue;
                }
            };

            let hash = compute_hash(&content);

            // Check if file has changed (incremental)
            if let Some(&(id, ref existing_hash)) = existing_hashes.get(&path_str) {
                if *existing_hash == hash {
                    debug!(path = %path_str, "File unchanged, skipping");
                    files_skipped += 1;
                    continue;
                }
                // File changed — update
                let size = content.len() as i64;
                self.db.update_file(id, &hash, size)?;
            } else {
                // New file — insert
                let size = content.len() as i64;
                self.db.insert_file(&path_str, &hash, size, language.as_deref())?;
            }

            // Extract symbols
            let file_id = self.db.get_file(&path_str)?
                .map(|f| f.id)
                .unwrap_or(0);

            if file_id > 0 {
                let symbols = extract_symbols(&content, &self.patterns, language.as_deref());
                for symbol in &symbols {
                    self.db.insert_symbol(
                        file_id,
                        &symbol.name,
                        &symbol.kind,
                        symbol.line_start,
                        symbol.line_end,
                        None,
                    )?;
                    symbols_extracted += 1;
                }
            }

            // Upsert into tantivy BM25 index (in-process, memory-mapped)
            if let (Some(ref idx), Some(ref mut writer)) = (&tantivy_index, &mut tantivy_writer) {
                let _ = idx.delete_document(writer, &path_str);
                let lang_str = language.as_deref().unwrap_or("text");
                let sym_names: Vec<String> = extract_symbols(&content, &self.patterns, language.as_deref()).iter().map(|s| s.name.clone()).collect();
                let _ = idx.add_document(writer, &path_str, &content, lang_str, &sym_names);
            }

            files_indexed += 1;

            // Log progress every 100 files
            if files_indexed % 100 == 0 {
                info!(
                    files_indexed = files_indexed,
                    symbols = symbols_extracted,
                    "Indexing progress"
                );
            }
        }

        // Remove deleted files from database (and tantivy)
        for path in existing_hashes.keys() {
            if !seen_paths.contains(path) {
                self.db.delete_file(path)?;
                if let (Some(ref idx), Some(ref mut writer)) = (&tantivy_index, &mut tantivy_writer) {
                    let _ = idx.delete_document(writer, path);
                }
                files_deleted += 1;
            }
        }

        // Commit tantivy if writer present
        if let (Some(ref idx), Some(ref mut writer)) = (&tantivy_index, &mut tantivy_writer) {
            let _ = idx.commit(writer);
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        let stats = IndexStats {
            files_indexed,
            symbols_extracted,
            files_skipped,
            files_deleted,
            duration_ms,
        };

        // Emit event
        self.event_bus.emit(RuntimeEvent::RepositoryUpdated {
            files_indexed,
            symbols_extracted,
            duration_ms,
        });

        info!(
            files_indexed = files_indexed,
            symbols_extracted = symbols_extracted,
            files_skipped = files_skipped,
            files_deleted = files_deleted,
            duration_ms = duration_ms,
            "Repository indexing complete"
        );

        Ok(stats)
    }

    /// Search for text in the repository using regex (ripgrep, spec §3)
    pub fn search_text(
        &self,
        query: &str,
        language_filter: Option<&str>,
        max_results: usize,
    ) -> Result<SearchResults, String> {
        // Use ripgrep for fast searching
        self.search_text_ripgrep(query, language_filter, max_results)
    }

    /// Structural search via ast-grep semantics (spec §3 — ast-grep structural search)
    /// Pattern examples: `function $NAME($$$) { $$$ }`, `class $C { $$$ }`, `fn $FUNC($$$) { $$$ }`
    /// v0.3.0 implements via tree-sitter node-type + identifier matching with regex fallback.
    /// Future: embed `ast-grep-core` directly.
    pub fn search_structural(
        &self,
        pattern: &str,
        language_filter: Option<&str>,
        max_results: usize,
    ) -> Result<SearchResults, String> {
        // Heuristic: map pattern keywords to tree-sitter node kinds
        let pattern_lower = pattern.to_lowercase();
        let want_fn = pattern_lower.contains("function") || pattern_lower.contains("fn ") || pattern_lower.contains("def ");
        let want_class = pattern_lower.contains("class") || pattern_lower.contains("struct") || pattern_lower.contains("interface");
        let want_loop = pattern_lower.contains("for") || pattern_lower.contains("while") || pattern_lower.contains("loop");

        let mut results = Vec::new();
        let walker = WalkBuilder::new(&self.repo_path).hidden(false).git_ignore(true).build();
        for entry in walker {
            let entry = match entry { Ok(e) => e, Err(_) => continue };
            if !entry.file_type().is_some_and(|ft| ft.is_file()) { continue; }
            let path = entry.path();
            let path_str = path.strip_prefix(&self.repo_path).unwrap_or(path).to_string_lossy().to_string();
            if let Some(lang) = language_filter {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if detect_language(ext).as_deref() != Some(lang) { continue; }
            }
            if is_likely_binary(path) { continue; }
            let content = match std::fs::read_to_string(path) { Ok(c) => c, Err(_) => continue };
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let lang = detect_language(ext).unwrap_or_else(|| "text".to_string());
            // Try tree-sitter structural match
            let mut matched = false;
            if ["rust", "python", "javascript", "typescript"].contains(&lang.as_str()) {
                let symbols = parser::extract_symbols_ast(&content, &lang);
                for sym in &symbols {
                    let is_fn = sym.kind == "function" || sym.kind == "method";
                    let is_class = sym.kind == "class" || sym.kind == "struct" || sym.kind == "interface";
                    if (want_fn && is_fn) || (want_class && is_class) || (want_loop && sym.kind == "loop") || (!want_fn && !want_class && !want_loop) {
                        results.push(SearchResult { path: path_str.clone(), line: sym.line_start as usize, content: format!("{} {} @ {}:{}", sym.kind, sym.name, path_str, sym.line_start), score: 0.9 });
                        matched = true;
                        if results.len() >= max_results { break; }
                    }
                }
            }
            // Fallback: regex contains of pattern keywords (strip ast-grep metavariables)
            if !matched {
                let keywords: Vec<String> = pattern.split(|c: char| !c.is_alphanumeric()).filter(|s| s.len() > 2 && !["function","class","struct","interface","fn"].contains(s)).map(|s| s.to_lowercase()).collect();
                let lower_content = content.to_lowercase();
                let mut score = 0.0;
                for kw in &keywords { if lower_content.contains(kw) { score += 1.0; } }
                if score > 0.0 && keywords.len() > 0 {
                    let norm = score / keywords.len() as f64;
                    if norm > 0.3 {
                        // find first line containing a keyword
                        for (i, line) in content.lines().enumerate() {
                            let ll = line.to_lowercase();
                            if keywords.iter().any(|k| ll.contains(k)) {
                                results.push(SearchResult { path: path_str.clone(), line: i+1, content: line.trim().to_string(), score: norm });
                                break;
                            }
                        }
                    }
                } else if keywords.is_empty() && (want_fn || want_class) {
                    // pattern with only metavariables — already handled via symbols; no fallback
                }
            }
            if results.len() >= max_results { break; }
        }
        results.truncate(max_results);
        let total = results.len();
        Ok(SearchResults { results, total_count: total })
    }

    /// Full-text BM25 search via tantivy (spec §3 — tantivy/BM25, in-process, memory-mapped)
    pub fn search_fulltext(
        &self,
        query: &str,
        language_filter: Option<&str>,
        max_results: usize,
    ) -> Result<SearchResults, String> {
        // Try tantivy index at default location; fallback to ripgrep if index missing
        let index_path = default_index_path();
        match coderun_storage::tantivy_index::TantivyIndex::open(&index_path) {
            Ok(idx) => {
                let reader = idx.reader().map_err(|e| format!("tantivy reader: {e}"))?;
                match idx.search(&reader, query, language_filter, max_results) {
                    Ok(hits) => {
                        if hits.is_empty() {
                            debug!("tantivy returned 0 hits for '{}', falling back to ripgrep", query);
                            return self.search_text(query, language_filter, max_results);
                        }
                        let mut results = Vec::new();
                        for hit in hits {
                            // snippet: first 200 chars
                            let snippet = hit.content.chars().take(200).collect::<String>();
                            results.push(SearchResult { path: hit.path, line: 1, content: snippet, score: hit.score as f64 });
                        }
                        let total = results.len();
                        Ok(SearchResults { results, total_count: total })
                    }
                    Err(e) => {
                        warn!(error = %e, "tantivy search failed, falling back to ripgrep");
                        self.search_text(query, language_filter, max_results)
                    }
                }
            }
            Err(_) => {
                debug!("tantivy index not found at {}, fallback to ripgrep", index_path);
                self.search_text(query, language_filter, max_results)
            }
        }
    }

    /// Search using ripgrep (grep-searcher crate)
    fn search_text_ripgrep(
        &self,
        query: &str,
        language_filter: Option<&str>,
        max_results: usize,
    ) -> Result<SearchResults, String> {
        let matcher = RegexMatcherBuilder::new()
            .case_insensitive(false)
            .build(query)
            .map_err(|e| format!("Invalid search pattern: {}", e))?;

        let mut results = Vec::new();

        // Use ignore's WalkBuilder for respecting .gitignore
        let walker = WalkBuilder::new(&self.repo_path)
            .hidden(false) // Include hidden files (but not .git)
            .git_ignore(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            let path = entry.path();
            let path_str = path.strip_prefix(&self.repo_path)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            // Apply language filter
            if let Some(lang) = language_filter {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if detect_language(ext).as_deref() != Some(lang) {
                    continue;
                }
            }

            // Skip binary files
            if is_likely_binary(path) {
                continue;
            }

            let mut searcher = SearcherBuilder::new().line_number(true).build();
            let mut match_count = 0;

            let _ = searcher.search_path(
                &matcher,
                path,
                UTF8(|line_number, line_content| {
                    if results.len() >= max_results {
                        return Ok(false);
                    }

                    results.push(SearchResult {
                        path: path_str.clone(),
                        line: line_number as usize,
                        content: line_content.trim_end().to_string(),
                        score: 1.0,
                    });

                    match_count += 1;
                    Ok(true)
                }),
            );

            if results.len() >= max_results {
                break;
            }
        }

        let total_count = results.len();
        Ok(SearchResults {
            results,
            total_count,
        })
    }

    /// Get file content with optional line range
    pub fn get_file_content(
        &self,
        path: &str,
        line_range: Option<(usize, usize)>,
    ) -> Result<String, String> {
        let full_path = self.repo_path.join(path);
        let content = std::fs::read_to_string(&full_path)
            .map_err(|e| format!("Failed to read file '{}': {}", path, e))?;

        match line_range {
            Some((start, end)) => {
                let lines: Vec<&str> = content.lines().collect();
                let start_idx = start.saturating_sub(1);
                let end_idx = end.min(lines.len());
                Ok(lines[start_idx..end_idx].join("\n"))
            }
            None => Ok(content),
        }
    }

    /// Get file information
    pub fn get_file_info(&self, path: &str) -> Result<Option<FileInfo>, String> {
        match self.db.get_file(path)? {
            Some(record) => {
                // Count symbols by getting all symbols and filtering by file_id
                let symbol_count = self.db.get_symbols_for_file(record.id)
                    .map(|s| s.len())
                    .unwrap_or(0);

                Ok(Some(FileInfo {
                    path: record.path,
                    size: record.size,
                    language: record.language,
                    symbol_count,
                    last_indexed_at: record.last_indexed_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get symbol information by name
    pub fn get_symbol_info(&self, query: &str) -> Result<Vec<SymbolInfo>, String> {
        let symbols = self.db.find_symbol(query)?;
        let files = self.db.get_all_files()?;
        let mut results = Vec::new();

        for symbol in symbols {
            // Look up file path by finding the file with matching id
            // In the current schema, we find the file by iterating
            let file_path = files.iter()
                .find(|(path, _)| {
                    // Simplified: match by checking if the symbol's file exists
                    self.db.get_file(path)
                        .ok()
                        .flatten()
                        .map(|f| f.id == symbol.file_id)
                        .unwrap_or(false)
                })
                .map(|(path, _)| path.clone())
                .unwrap_or_else(|| "unknown".to_string());

            results.push(SymbolInfo {
                name: symbol.name,
                kind: symbol.kind,
                file_path,
                line_start: symbol.line_start,
                line_end: symbol.line_end,
            });
        }

        Ok(results)
    }

    /// Build dependency graph for the current repo (spec §3, ROADMAP.md:81)
    pub fn build_dependency_graph(&self) -> Result<graph::DependencyGraph, String> {
        let files = self.walk_directory(&self.repo_path)?;
        Ok(graph::DependencyGraph::build_from_files(&self.repo_path, &files))
    }

    /// LSP client accessor (optional enrichment, never hard dep)
    pub fn lsp_client(&self) -> lsp::LspClient {
        lsp::LspClient::default()
    }

    /// Spawn git-change-aware watcher that re-indexes incrementally
    pub fn spawn_watcher(&self) -> watcher::RepoWatcher {
        watcher::RepoWatcher::new(self.repo_path.clone())
    }

    /// Walk directory tree, yielding indexable files
    fn walk_directory(&self, dir: &Path) -> Result<Vec<PathBuf>, String> {
        let mut files = Vec::new();

        // Use ignore's WalkBuilder for respecting .gitignore
        let walker = WalkBuilder::new(dir)
            .hidden(false) // Include hidden files (but not .git)
            .git_ignore(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                files.push(entry.into_path());
            }
        }

        Ok(files)
    }
}

// ── Helper Functions ────────────────────────────────────────────────────

/// Detect programming language from file extension
fn detect_language(ext: &str) -> Option<String> {
    EXTENSION_MAP
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, lang)| lang.to_string())
}

/// Check if a file extension is an indexable text file
fn is_indexable_text_file(ext: &str) -> bool {
    // Common config and text files that should be indexed
    matches!(ext,
        "toml" | "yaml" | "yml" | "json" | "xml" | "md" | "txt" |
        "sql" | "sh" | "bash" | "zsh" | "env" | "gitignore" |
        "dockerfile" | "makefile" | "cmake" | "gradle" | "sbt"
    )
}



/// Check if a file is likely binary
fn is_likely_binary(path: &Path) -> bool {
    // Check extension first
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if matches!(ext,
        "exe" | "dll" | "so" | "dylib" | "o" | "a" | "lib" |
        "bin" | "dat" | "png" | "jpg" | "jpeg" | "gif" | "bmp" |
        "ico" | "svg" | "pdf" | "zip" | "tar" | "gz" | "bz2" |
        "xz" | "7z" | "rar" | "woff" | "woff2" | "ttf" | "otf" |
        "eot" | "mp3" | "mp4" | "avi" | "mov" | "wav" | "ogg"
    ) {
        return true;
    }

    // Read first 512 bytes and check for null bytes
    if let Ok(content) = std::fs::read(path) {
        let check_len = content.len().min(512);
        let null_count = content[..check_len]
            .iter()
            .filter(|&&b| b == 0)
            .count();
        // If more than 1% null bytes in first 512 bytes, likely binary
        null_count > check_len / 100
    } else {
        false
    }
}

/// Default tantivy index path (spec §3 — MmapDirectory)
fn default_index_path() -> String {
    if let Some(home) = dirs_home() {
        home.join(".coderun").join("index").to_string_lossy().to_string()
    } else {
        ".coderun/index".to_string()
    }
}

fn dirs_home() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    { std::env::var("USERPROFILE").ok().map(PathBuf::from) }
    #[cfg(not(target_os = "windows"))]
    { std::env::var("HOME").ok().map(PathBuf::from) }
}

/// Compute SHA-256 hash of content
fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Extract symbols from source code (tree-sitter AST + regex fallback)
fn extract_symbols(content: &str, patterns: &SymbolPatterns, language: Option<&str>) -> Vec<ExtractedSymbol> {
    // Try tree-sitter first if language is supported
    if let Some(lang) = language {
        if lang == "rust" || lang == "python" || lang == "javascript" || lang == "typescript" {
            let ast_symbols = parser::extract_symbols_ast(content, lang);
            return ast_symbols.into_iter().map(|s| ExtractedSymbol {
                name: s.name,
                kind: s.kind,
                line_start: s.line_start as i64,
                line_end: s.line_end as i64,
            }).collect();
        }
    }

    // Fallback to regex patterns
    let mut symbols = Vec::new();

    // Extract functions
    for cap in patterns.function_pattern.captures_iter(content) {
        if let Some(name) = cap.get(1).or(cap.get(2)).or(cap.get(3)).or(cap.get(4)).or(cap.get(5)) {
            let line_num = content[..cap.get(0).unwrap().start()]
                .lines()
                .count();
            symbols.push(ExtractedSymbol {
                name: name.as_str().to_string(),
                kind: "function".to_string(),
                line_start: line_num as i64,
                line_end: line_num as i64 + 5, // Approximate
            });
        }
    }

    // Extract structs/classes
    for cap in patterns.struct_pattern.captures_iter(content) {
        if let Some(name) = cap.get(1).or(cap.get(2)).or(cap.get(3)) {
            let line_num = content[..cap.get(0).unwrap().start()]
                .lines()
                .count();
            symbols.push(ExtractedSymbol {
                name: name.as_str().to_string(),
                kind: "struct".to_string(),
                line_start: line_num as i64,
                line_end: line_num as i64 + 10, // Approximate
            });
        }
    }

    // Extract enums
    for cap in patterns.enum_pattern.captures_iter(content) {
        if let Some(name) = cap.get(1) {
            let line_num = content[..cap.get(0).unwrap().start()]
                .lines()
                .count();
            symbols.push(ExtractedSymbol {
                name: name.as_str().to_string(),
                kind: "enum".to_string(),
                line_start: line_num as i64,
                line_end: line_num as i64 + 10,
            });
        }
    }

    // Extract impl blocks
    for cap in patterns.impl_pattern.captures_iter(content) {
        if let Some(name) = cap.get(1) {
            let line_num = content[..cap.get(0).unwrap().start()]
                .lines()
                .count();
            symbols.push(ExtractedSymbol {
                name: name.as_str().to_string(),
                kind: "impl".to_string(),
                line_start: line_num as i64,
                line_end: line_num as i64 + 20, // Approximate
            });
        }
    }

    // Extract traits
    for cap in patterns.trait_pattern.captures_iter(content) {
        if let Some(name) = cap.get(1) {
            let line_num = content[..cap.get(0).unwrap().start()]
                .lines()
                .count();
            symbols.push(ExtractedSymbol {
                name: name.as_str().to_string(),
                kind: "trait".to_string(),
                line_start: line_num as i64,
                line_end: line_num as i64 + 15,
            });
        }
    }

    // Extract type aliases
    for cap in patterns.type_pattern.captures_iter(content) {
        if let Some(name) = cap.get(1) {
            let line_num = content[..cap.get(0).unwrap().start()]
                .lines()
                .count();
            symbols.push(ExtractedSymbol {
                name: name.as_str().to_string(),
                kind: "type".to_string(),
                line_start: line_num as i64,
                line_end: line_num as i64 + 1,
            });
        }
    }

    symbols
}

#[derive(Debug)]
struct ExtractedSymbol {
    name: String,
    kind: String,
    line_start: i64,
    line_end: i64,
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language("rs"), Some("rust".to_string()));
        assert_eq!(detect_language("ts"), Some("typescript".to_string()));
        assert_eq!(detect_language("py"), Some("python".to_string()));
        assert_eq!(detect_language("go"), Some("go".to_string()));
        assert_eq!(detect_language("xyz"), None);
    }



    #[test]
    fn test_compute_hash() {
        let hash1 = compute_hash("hello world");
        let hash2 = compute_hash("hello world");
        let hash3 = compute_hash("hello world!");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 64); // SHA-256 hex string
    }

    #[test]
    fn test_extract_symbols_rust() {
        let content = r#"
pub fn main() {
    println!("Hello");
}

pub struct Config {
    pub name: String,
}

enum Color {
    Red,
    Green,
    Blue,
}

impl Config {
    fn new() -> Self {
        Config { name: "test".to_string() }
    }
}

trait Drawable {
    fn draw(&self);
}

type Result<T> = std::result::Result<T, Error>;
"#;

        let patterns = SymbolPatterns::new();
        let symbols = extract_symbols(content, &patterns, Some("rust"));

        assert!(symbols.iter().any(|s| s.name == "main" && s.kind == "function"));
        assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == "struct"));
        assert!(symbols.iter().any(|s| s.name == "Color" && s.kind == "enum"));
        // Note: tree-sitter may not extract impl blocks the same way as regex
        assert!(symbols.iter().any(|s| s.name == "Drawable" && s.kind == "trait"));
    }

    #[test]
    fn test_extract_symbols_python() {
        let content = r#"
def hello():
    print("Hello")

class Config:
    def __init__(self):
        self.name = "test"

class MyEnum:
    pass
"#;

        let patterns = SymbolPatterns::new();
        let symbols = extract_symbols(content, &patterns, Some("python"));

        assert!(symbols.iter().any(|s| s.name == "hello" && s.kind == "function"));
        assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == "class"));
        assert!(symbols.iter().any(|s| s.name == "MyEnum" && s.kind == "class"));
    }

    #[test]
    fn test_extract_symbols_javascript() {
        let content = r#"
function hello() {
    console.log("Hello");
}

class Config {
    constructor() {
        this.name = "test";
    }
}

const greet = () => {
    console.log("Hi");
};

export async function fetchData() {
    return {};
}
"#;

        let patterns = SymbolPatterns::new();
        let symbols = extract_symbols(content, &patterns, Some("javascript"));

        assert!(symbols.iter().any(|s| s.name == "hello" && s.kind == "function"));
        assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == "class"));
        assert!(symbols.iter().any(|s| s.name == "fetchData" && s.kind == "function"));
    }

    #[test]
    fn test_is_likely_binary() {
        // Can't easily test with actual files in unit test, but test the logic
        let text_path = Path::new("test.rs");
        assert!(!is_likely_binary(text_path));

        let exe_path = Path::new("test.exe");
        assert!(is_likely_binary(exe_path));
    }

    #[test]
    fn test_is_indexable_text_file() {
        assert!(is_indexable_text_file("toml"));
        assert!(is_indexable_text_file("yaml"));
        assert!(is_indexable_text_file("md"));
        assert!(is_indexable_text_file("sql"));
        assert!(!is_indexable_text_file("rs"));
        assert!(!is_indexable_text_file("py"));
    }

    #[test]
    fn test_search_results_structure() {
        let results = SearchResults {
            results: vec![
                SearchResult {
                    path: "src/main.rs".to_string(),
                    line: 10,
                    content: "fn main() {}".to_string(),
                    score: 1.0,
                },
            ],
            total_count: 1,
        };

        assert_eq!(results.total_count, 1);
        assert_eq!(results.results[0].path, "src/main.rs");
    }

    #[test]
    fn test_search_structural_finds_pattern() {
        // ast-grep structural search: pattern `fn $FUNC` should match rust functions via tree-sitter
        let dir = std::env::temp_dir().join(format!("coderun_struct_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sample.rs"), "pub fn hello() {}\nfn world() {}\nstruct Foo;").unwrap();
        let db = coderun_storage::Database::open(&PathBuf::from(":memory:")).unwrap();
        let ri = RepositoryIntelligence::new(dir.clone(), db, EventBus::new());
        let res = ri.search_structural("fn $FUNC", None, 10).unwrap();
        assert!(!res.results.is_empty(), "structural search should find fn patterns");
        assert!(res.results.iter().any(|r| r.content.contains("hello") || r.content.contains("world") || r.content.contains("function")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_search_fulltext_via_tantivy_fallback() {
        // Full-text BM25: should fallback to ripgrep when tantivy index missing, still return results
        let dir = std::env::temp_dir().join(format!("coderun_fulltext_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.md"), "authentication middleware handles token verification").unwrap();
        std::fs::write(dir.join("main.rs"), "fn authenticate() { /* token check */ }").unwrap();
        let db = coderun_storage::Database::open(&PathBuf::from(":memory:")).unwrap();
        let ri = RepositoryIntelligence::new(dir.clone(), db, EventBus::new());
        let res = ri.search_fulltext("authentication", None, 10).unwrap();
        assert!(res.total_count >= 1, "fulltext should find at least one hit");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
