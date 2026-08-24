use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use coderun_core::{SearchResult, SearchResults};
use coderun_events::{EventBus, RuntimeEvent};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

// ── Configuration ───────────────────────────────────────────────────────

/// Patterns to ignore during directory walking
const IGNORE_PATTERNS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "venv",
    ".env",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "vendor",
    ".idea",
    ".vscode",
    "*.pyc",
    "*.pyo",
    "*.so",
    "*.dll",
    "*.dylib",
    "*.exe",
    "*.o",
    "*.a",
    "*.lib",
];

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

    /// Index the repository (full or incremental)
    pub fn index_repository(&mut self) -> Result<IndexStats, String> {
        let start = Instant::now();
        let mut files_indexed = 0;
        let mut symbols_extracted = 0;
        let mut files_skipped = 0;
        let mut files_deleted = 0;

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
                let symbols = extract_symbols(&content, &self.patterns);
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

        // Remove deleted files from database
        for path in existing_hashes.keys() {
            if !seen_paths.contains(path) {
                self.db.delete_file(path)?;
                files_deleted += 1;
            }
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

    /// Search for text in the repository using regex
    pub fn search_text(
        &self,
        query: &str,
        language_filter: Option<&str>,
        max_results: usize,
    ) -> Result<SearchResults, String> {
        let pattern = regex::Regex::new(query)
            .map_err(|e| format!("Invalid search pattern: {}", e))?;

        let files = self.db.get_all_files()?;
        let mut results = Vec::new();

        for (path, _hash) in &files {
            // Apply language filter
            if let Some(lang) = language_filter {
                let ext = Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if detect_language(ext).as_deref() != Some(lang) {
                    continue;
                }
            }

            let full_path = self.repo_path.join(path);
            let content = match std::fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            for (line_num, line) in content.lines().enumerate() {
                if pattern.is_match(line) {
                    results.push(SearchResult {
                        path: path.clone(),
                        line: line_num + 1,
                        content: line.to_string(),
                        score: 1.0,
                    });

                    if results.len() >= max_results {
                        break;
                    }
                }
            }

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

    /// Walk directory tree, yielding indexable files
    fn walk_directory(&self, dir: &Path) -> Result<Vec<PathBuf>, String> {
        let mut files = Vec::new();
        self.walk_recursive(dir, &mut files)?;
        Ok(files)
    }

    fn walk_recursive(&self, dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        let entries = std::fs::read_dir(dir)
            .map_err(|e| format!("Failed to read directory '{}': {}", dir.display(), e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();

            if path.is_dir() {
                let dir_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                // Check ignore patterns
                if should_ignore(dir_name) {
                    debug!(dir = dir_name, "Ignoring directory");
                    continue;
                }

                self.walk_recursive(&path, files)?;
            } else if path.is_file() {
                files.push(path);
            }
        }

        Ok(())
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

/// Check if a directory/file should be ignored
fn should_ignore(name: &str) -> bool {
    let lower = name.to_lowercase();
    IGNORE_PATTERNS.iter().any(|pattern| {
        if let Some(suffix) = pattern.strip_prefix('*') {
            lower.ends_with(suffix)
        } else {
            lower == *pattern
        }
    })
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

/// Compute SHA-256 hash of content
fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Extract symbols from source code using regex patterns
fn extract_symbols(content: &str, patterns: &SymbolPatterns) -> Vec<ExtractedSymbol> {
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
    fn test_should_ignore() {
        assert!(should_ignore("node_modules"));
        assert!(should_ignore("target"));
        assert!(should_ignore(".git"));
        assert!(should_ignore("__pycache__"));
        assert!(!should_ignore("src"));
        assert!(!should_ignore("crates"));
    }

    #[test]
    fn test_should_ignore_with_glob() {
        assert!(should_ignore("*.pyc"));
        assert!(should_ignore("*.so"));
        assert!(should_ignore("*.exe"));
        assert!(!should_ignore("main.rs"));
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
        let symbols = extract_symbols(content, &patterns);

        assert!(symbols.iter().any(|s| s.name == "main" && s.kind == "function"));
        assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == "struct"));
        assert!(symbols.iter().any(|s| s.name == "Color" && s.kind == "enum"));
        assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == "impl"));
        assert!(symbols.iter().any(|s| s.name == "Drawable" && s.kind == "trait"));
        assert!(symbols.iter().any(|s| s.name == "Result" && s.kind == "type"));
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
        let symbols = extract_symbols(content, &patterns);

        assert!(symbols.iter().any(|s| s.name == "hello" && s.kind == "function"));
        assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == "struct"));
        assert!(symbols.iter().any(|s| s.name == "MyEnum" && s.kind == "struct"));
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
        let symbols = extract_symbols(content, &patterns);

        assert!(symbols.iter().any(|s| s.name == "hello" && s.kind == "function"));
        assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == "struct"));
        assert!(symbols.iter().any(|s| s.name == "greet" && s.kind == "function"));
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
}
