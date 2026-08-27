use std::path::Path;
use tree_sitter::Language;

// ── Language Identity ────────────────────────────────────────────────────

/// Stable language identifier — single source of truth for all of Coderun.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LanguageId {
    Rust,
    TypeScript,
    TypeScriptReact,
    JavaScript,
    JavaScriptReact,
    Python,
    CSharp,
    Go,
    Java,
    C,
    Cpp,
    Ruby,
    Php,
    Swift,
    Kotlin,
    Scala,
    Haskell,
    Elixir,
    Erlang,
    Lua,
    Zig,
    Nim,
    R,
    Ocaml,
    Clojure,
    FSharp,
    Vb,
    Sql,
    Shell,
    Protobuf,
    GraphQL,
    Terraform,
    Vue,
    Svelte,
    // Non-code — metadata/search only, no parser
    Markdown,
    Yaml,
    Toml,
    Json,
    Xml,
    Html,
    Css,
    Scss,
    Text,
}

impl LanguageId {
    /// String name used in DB, tantivy, and logs
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::TypeScriptReact => "typescriptreact",
            Self::JavaScript => "javascript",
            Self::JavaScriptReact => "javascriptreact",
            Self::Python => "python",
            Self::CSharp => "csharp",
            Self::Go => "go",
            Self::Java => "java",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Ruby => "ruby",
            Self::Php => "php",
            Self::Swift => "swift",
            Self::Kotlin => "kotlin",
            Self::Scala => "scala",
            Self::Haskell => "haskell",
            Self::Elixir => "elixir",
            Self::Erlang => "erlang",
            Self::Lua => "lua",
            Self::Zig => "zig",
            Self::Nim => "nim",
            Self::R => "r",
            Self::Ocaml => "ocaml",
            Self::Clojure => "clojure",
            Self::FSharp => "fsharp",
            Self::Vb => "vb",
            Self::Sql => "sql",
            Self::Shell => "shell",
            Self::Protobuf => "protobuf",
            Self::GraphQL => "graphql",
            Self::Terraform => "terraform",
            Self::Vue => "vue",
            Self::Svelte => "svelte",
            Self::Markdown => "markdown",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Html => "html",
            Self::Css => "css",
            Self::Scss => "scss",
            Self::Text => "text",
        }
    }

    /// Does this language have a tree-sitter parser?
    pub fn has_parser(&self) -> bool {
        matches!(
            self,
            Self::Rust | Self::TypeScript | Self::TypeScriptReact |
            Self::JavaScript | Self::JavaScriptReact | Self::Python |
            Self::CSharp
        ) || cfg!(feature = "extended-languages") && matches!(
            self,
            Self::Go | Self::Java | Self::C | Self::Cpp
        )
    }

    /// Parse from string (case-insensitive)
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "rust" | "rs" => Some(Self::Rust),
            "typescript" | "ts" => Some(Self::TypeScript),
            "typescriptreact" | "tsx" => Some(Self::TypeScriptReact),
            "javascript" | "js" => Some(Self::JavaScript),
            "javascriptreact" | "jsx" => Some(Self::JavaScriptReact),
            "python" | "py" => Some(Self::Python),
            "csharp" | "c#" | "cs" => Some(Self::CSharp),
            "go" => Some(Self::Go),
            "java" => Some(Self::Java),
            "c" => Some(Self::C),
            "cpp" | "c++" => Some(Self::Cpp),
            "ruby" | "rb" => Some(Self::Ruby),
            "php" => Some(Self::Php),
            "swift" => Some(Self::Swift),
            "kotlin" | "kt" => Some(Self::Kotlin),
            "scala" => Some(Self::Scala),
            "haskell" | "hs" => Some(Self::Haskell),
            "elixir" | "ex" => Some(Self::Elixir),
            "erlang" | "erl" => Some(Self::Erlang),
            "lua" => Some(Self::Lua),
            "zig" => Some(Self::Zig),
            "nim" => Some(Self::Nim),
            "r" => Some(Self::R),
            "ocaml" | "ml" => Some(Self::Ocaml),
            "clojure" | "clj" => Some(Self::Clojure),
            "fsharp" | "f#" | "fs" => Some(Self::FSharp),
            "vb" | "vb.net" => Some(Self::Vb),
            "sql" => Some(Self::Sql),
            "shell" | "sh" | "bash" | "zsh" => Some(Self::Shell),
            "protobuf" | "proto" => Some(Self::Protobuf),
            "graphql" | "gql" => Some(Self::GraphQL),
            "terraform" | "tf" | "hcl" => Some(Self::Terraform),
            "vue" => Some(Self::Vue),
            "svelte" => Some(Self::Svelte),
            "markdown" | "md" => Some(Self::Markdown),
            "yaml" | "yml" => Some(Self::Yaml),
            "toml" => Some(Self::Toml),
            "json" => Some(Self::Json),
            "xml" => Some(Self::Xml),
            "html" => Some(Self::Html),
            "css" => Some(Self::Css),
            "scss" => Some(Self::Scss),
            "text" | "txt" => Some(Self::Text),
            _ => None,
        }
    }
}

// ── Language Definition ──────────────────────────────────────────────────

/// Static definition of a supported language.
#[derive(Debug, Clone)]
pub struct LanguageDefinition {
    pub id: LanguageId,
    pub extensions: &'static [&'static str],
    pub filenames: &'static [&'static str],
}

impl LanguageDefinition {
    const fn new(id: LanguageId, extensions: &'static [&'static str], filenames: &'static [&'static str]) -> Self {
        Self { id, extensions, filenames }
    }
}

// ── Parser Registry ──────────────────────────────────────────────────────

/// Get tree-sitter language for a LanguageId (backward-compatible free function).
/// Prefer `ParserRegistry::get_language()` for new code.
pub fn get_ts_language(id: LanguageId) -> Option<Language> {
    ParserRegistry::default().get_language(id)
}

/// Extensible parser registry — manages language definitions and grammar loading.
/// Languages can be registered at runtime via `register_language()`.
#[derive(Debug, Clone)]
pub struct ParserRegistry {
    definitions: Vec<LanguageDefinition>,
    grammar_loaders: std::collections::HashMap<LanguageId, fn() -> Option<Language>>,
}

impl ParserRegistry {
    /// Create a new registry with all built-in languages pre-registered.
    pub fn new() -> Self {
        let mut registry = Self {
            definitions: Vec::new(),
            grammar_loaders: std::collections::HashMap::new(),
        };
        registry.register_builtins();
        registry
    }

    /// Register a built-in language with its grammar loader.
    fn register_builtin(&mut self, def: LanguageDefinition, loader: fn() -> Option<Language>) {
        self.grammar_loaders.insert(def.id, loader);
        self.definitions.push(def);
    }

    /// Register all built-in languages.
    fn register_builtins(&mut self) {
        // Source languages with parsers
        self.register_builtin(LanguageDefinition::new(LanguageId::Rust, &["rs"], &["Cargo.toml"]), || Some(tree_sitter_rust::LANGUAGE.into()));
        self.register_builtin(LanguageDefinition::new(LanguageId::TypeScript, &["ts", "mts", "cts"], &[]), || Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()));
        self.register_builtin(LanguageDefinition::new(LanguageId::TypeScriptReact, &["tsx"], &[]), || Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()));
        self.register_builtin(LanguageDefinition::new(LanguageId::JavaScript, &["js", "mjs", "cjs"], &["package.json"]), || Some(tree_sitter_javascript::LANGUAGE.into()));
        self.register_builtin(LanguageDefinition::new(LanguageId::JavaScriptReact, &["jsx"], &[]), || Some(tree_sitter_javascript::LANGUAGE.into()));
        self.register_builtin(LanguageDefinition::new(LanguageId::Python, &["py", "pyi"], &["pyproject.toml", "setup.py", "requirements.txt"]), || Some(tree_sitter_python::LANGUAGE.into()));
        self.register_builtin(LanguageDefinition::new(LanguageId::CSharp, &["cs"], &["*.csproj", "*.sln"]), || Some(tree_sitter_c_sharp::LANGUAGE.into()));
        #[cfg(feature = "extended-languages")]
        {
            self.register_builtin(LanguageDefinition::new(LanguageId::Go, &["go"], &["go.mod"]), || Some(tree_sitter_go::LANGUAGE.into()));
            self.register_builtin(LanguageDefinition::new(LanguageId::Java, &["java"], &["pom.xml", "build.gradle"]), || Some(tree_sitter_java::LANGUAGE.into()));
            self.register_builtin(LanguageDefinition::new(LanguageId::C, &["c", "h"], &[]), || Some(tree_sitter_c::LANGUAGE.into()));
            self.register_builtin(LanguageDefinition::new(LanguageId::Cpp, &["cpp", "cc", "cxx", "hpp"], &[]), || Some(tree_sitter_cpp::LANGUAGE.into()));
        }
        // Source languages without parsers (regex fallback)
        self.register_builtin(LanguageDefinition::new(LanguageId::Ruby, &["rb"], &["Gemfile"]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Php, &["php"], &["composer.json"]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Swift, &["swift"], &["Package.swift"]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Kotlin, &["kt", "kts"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Scala, &["scala"], &["build.sbt"]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Haskell, &["hs"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Elixir, &["ex", "exs"], &["mix.exs"]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Erlang, &["erl"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Lua, &["lua"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Zig, &["zig"], &["build.zig"]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Nim, &["nim"], &["*.nimble"]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::R, &["r", "R"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Ocaml, &["ml"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Clojure, &["clj"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::FSharp, &["fs", "fsx"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Vb, &["vb"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Sql, &["sql"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Shell, &["sh", "bash", "zsh"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Protobuf, &["proto"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::GraphQL, &["graphql", "gql"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Terraform, &["tf", "hcl"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Vue, &["vue"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Svelte, &["svelte"], &[]), || None);
        // Non-code (metadata/search only)
        self.register_builtin(LanguageDefinition::new(LanguageId::Markdown, &["md"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Yaml, &["yaml", "yml"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Toml, &["toml"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Json, &["json"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Xml, &["xml"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Html, &["html"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Css, &["css"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Scss, &["scss"], &[]), || None);
        self.register_builtin(LanguageDefinition::new(LanguageId::Text, &["txt"], &[]), || None);
    }

    /// Register a new language at runtime.
    /// Returns `true` if the language was registered, `false` if already present.
    pub fn register_language(&mut self, def: LanguageDefinition, loader: Option<fn() -> Option<Language>>) -> bool {
        if self.definitions.iter().any(|d| d.id == def.id) {
            return false;
        }
        if let Some(f) = loader {
            self.grammar_loaders.insert(def.id, f);
        }
        self.definitions.push(def);
        true
    }

    /// Get tree-sitter language for a LanguageId using this registry's loaders.
    pub fn get_language(&self, id: LanguageId) -> Option<Language> {
        if let Some(loader) = self.grammar_loaders.get(&id) {
            return loader();
        }
        None
    }

    /// List all available languages (with parser support status).
    pub fn list_available_languages(&self) -> Vec<(LanguageId, bool)> {
        self.definitions.iter().map(|d| (d.id, d.id.has_parser())).collect()
    }

    /// List languages that have tree-sitter parsers loaded.
    pub fn list_with_parsers(&self) -> Vec<LanguageId> {
        self.definitions.iter()
            .filter(|d| self.get_language(d.id).is_some())
            .map(|d| d.id)
            .collect()
    }

    /// Look up language definition by file extension.
    pub fn language_by_extension(&self, ext: &str) -> Option<&LanguageDefinition> {
        self.definitions.iter().find(|def| def.extensions.contains(&ext))
    }

    /// Look up language definition by manifest filename.
    pub fn language_by_manifest(&self, filename: &str) -> Option<&LanguageDefinition> {
        self.definitions.iter().find(|def| def.filenames.contains(&filename))
    }

    /// Detect language from a file path using this registry.
    pub fn detect_language(&self, path: &Path) -> Option<&LanguageDefinition> {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(def) = self.language_by_manifest(name) {
                return Some(def);
            }
        }
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if let Some(def) = self.language_by_extension(ext) {
                return Some(def);
            }
        }
        None
    }
}

impl Default for ParserRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Static registry of all known languages (backward-compatible constant).
pub const LANGUAGE_REGISTRY: &[LanguageDefinition] = &[
    // ── Source languages with parsers ──
    LanguageDefinition::new(LanguageId::Rust, &["rs"], &["Cargo.toml"]),
    LanguageDefinition::new(LanguageId::TypeScript, &["ts", "mts", "cts"], &[]),
    LanguageDefinition::new(LanguageId::TypeScriptReact, &["tsx"], &[]),
    LanguageDefinition::new(LanguageId::JavaScript, &["js", "mjs", "cjs"], &["package.json"]),
    LanguageDefinition::new(LanguageId::JavaScriptReact, &["jsx"], &[]),
    LanguageDefinition::new(LanguageId::Python, &["py", "pyi"], &["pyproject.toml", "setup.py", "requirements.txt"]),
    LanguageDefinition::new(LanguageId::CSharp, &["cs"], &["*.csproj", "*.sln"]),
    LanguageDefinition::new(LanguageId::Go, &["go"], &["go.mod"]),
    LanguageDefinition::new(LanguageId::Java, &["java"], &["pom.xml", "build.gradle"]),
    LanguageDefinition::new(LanguageId::C, &["c", "h"], &[]),
    LanguageDefinition::new(LanguageId::Cpp, &["cpp", "cc", "cxx", "hpp"], &[]),
    // ── Source languages without parsers (regex fallback) ──
    LanguageDefinition::new(LanguageId::Ruby, &["rb"], &["Gemfile"]),
    LanguageDefinition::new(LanguageId::Php, &["php"], &["composer.json"]),
    LanguageDefinition::new(LanguageId::Swift, &["swift"], &["Package.swift"]),
    LanguageDefinition::new(LanguageId::Kotlin, &["kt", "kts"], &[]),
    LanguageDefinition::new(LanguageId::Scala, &["scala"], &["build.sbt"]),
    LanguageDefinition::new(LanguageId::Haskell, &["hs"], &[]),
    LanguageDefinition::new(LanguageId::Elixir, &["ex", "exs"], &["mix.exs"]),
    LanguageDefinition::new(LanguageId::Erlang, &["erl"], &[]),
    LanguageDefinition::new(LanguageId::Lua, &["lua"], &[]),
    LanguageDefinition::new(LanguageId::Zig, &["zig"], &["build.zig"]),
    LanguageDefinition::new(LanguageId::Nim, &["nim"], &["*.nimble"]),
    LanguageDefinition::new(LanguageId::R, &["r", "R"], &[]),
    LanguageDefinition::new(LanguageId::Ocaml, &["ml"], &[]),
    LanguageDefinition::new(LanguageId::Clojure, &["clj"], &[]),
    LanguageDefinition::new(LanguageId::FSharp, &["fs", "fsx"], &[]),
    LanguageDefinition::new(LanguageId::Vb, &["vb"], &[]),
    LanguageDefinition::new(LanguageId::Sql, &["sql"], &[]),
    LanguageDefinition::new(LanguageId::Shell, &["sh", "bash", "zsh"], &[]),
    LanguageDefinition::new(LanguageId::Protobuf, &["proto"], &[]),
    LanguageDefinition::new(LanguageId::GraphQL, &["graphql", "gql"], &[]),
    LanguageDefinition::new(LanguageId::Terraform, &["tf", "hcl"], &[]),
    LanguageDefinition::new(LanguageId::Vue, &["vue"], &[]),
    LanguageDefinition::new(LanguageId::Svelte, &["svelte"], &[]),
    // ── Non-code (metadata/search only) ──
    LanguageDefinition::new(LanguageId::Markdown, &["md"], &[]),
    LanguageDefinition::new(LanguageId::Yaml, &["yaml", "yml"], &[]),
    LanguageDefinition::new(LanguageId::Toml, &["toml"], &[]),
    LanguageDefinition::new(LanguageId::Json, &["json"], &[]),
    LanguageDefinition::new(LanguageId::Xml, &["xml"], &[]),
    LanguageDefinition::new(LanguageId::Html, &["html"], &[]),
    LanguageDefinition::new(LanguageId::Css, &["css"], &[]),
    LanguageDefinition::new(LanguageId::Scss, &["scss"], &[]),
    LanguageDefinition::new(LanguageId::Text, &["txt"], &[]),
];

/// Look up language by file extension
pub fn language_by_extension(ext: &str) -> Option<&'static LanguageDefinition> {
    LANGUAGE_REGISTRY.iter().find(|def| def.extensions.contains(&ext))
}

/// Look up language by manifest filename (e.g. "Cargo.toml" -> Rust)
pub fn language_by_manifest(filename: &str) -> Option<&'static LanguageDefinition> {
    LANGUAGE_REGISTRY.iter().find(|def| def.filenames.contains(&filename))
}

/// Detect language from a file path
pub fn detect_language(path: &Path) -> Option<&'static LanguageDefinition> {
    // Try filename/manifest first (e.g. "Cargo.toml" -> Rust, "go.mod" -> Go)
    // This must come before extension check since "Cargo.toml" has ext "toml"
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if let Some(def) = language_by_manifest(name) {
            return Some(def);
        }
    }
    // Try extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if let Some(def) = language_by_extension(ext) {
            return Some(def);
        }
    }
    None
}

// ── File Classification ──────────────────────────────────────────────────

/// Classifies a file path into a category for indexing decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileClass {
    Source,
    Test,
    Generated,
    Vendor,
    Dependency,
    Config,
    Documentation,
    Binary,
    Stylesheet,
    Unknown,
}

/// Directories that indicate generated/vendor/dependency code
const VENDOR_DIRS: &[&str] = &["vendor", "node_modules", "third_party", "extern", "external", ".git", ".svn", ".hg", "__pycache__", ".cache", ".coderun", ".vscode", ".idea", ".vs", ".claude", ".devcontainer"];
const GENERATED_DIRS: &[&str] = &["generated", "gen", "auto-generated", "__generated__"];
const DEP_DIRS: &[&str] = &["target", "bin", "obj", "dist", "build", ".gradle", ".next", ".nuxt"];
const TEST_DIRS: &[&str] = &["test", "tests", "__tests__", "test_data", "testdata", "fixtures", "spec"];

/// Classify a file path
pub fn classify_file(path: &Path) -> FileClass {
    let components: Vec<_> = path.components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Check for binary extensions
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if matches!(ext,
            "exe" | "dll" | "so" | "dylib" | "o" | "a" | "lib" |
            "bin" | "dat" | "png" | "jpg" | "jpeg" | "gif" | "bmp" |
            "ico" | "pdf" | "zip" | "tar" | "gz" | "woff" | "woff2" | "ttf"
        ) {
            return FileClass::Binary;
        }
        // Stylesheet files — never useful for code search, exclude from index
        if matches!(ext, "css" | "scss" | "less" | "sass" | "styl") {
            return FileClass::Stylesheet;
        }
    }

    // Check for dotfiles (hidden config files like .editorconfig, .gitignore, etc.)
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.') && name.len() > 1 {
            return FileClass::Config;
        }
    }

    for comp in &components {
        let lower = comp.to_lowercase();
        if VENDOR_DIRS.contains(&lower.as_str()) {
            return FileClass::Vendor;
        }
        if GENERATED_DIRS.contains(&lower.as_str()) {
            return FileClass::Generated;
        }
        if DEP_DIRS.contains(&lower.as_str()) {
            return FileClass::Dependency;
        }
        if TEST_DIRS.contains(&lower.as_str()) {
            return FileClass::Test;
        }
    }

    // Check for config/doc files
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if matches!(ext, "md" | "txt" | "rst") {
            return FileClass::Documentation;
        }
        if matches!(ext, "toml" | "yaml" | "yml" | "json" | "xml" | "ini" | "cfg" | "env") {
            return FileClass::Config;
        }
    }

    FileClass::Source
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_id_roundtrip() {
        for id in [
            LanguageId::Rust, LanguageId::TypeScript, LanguageId::JavaScript,
            LanguageId::Python, LanguageId::CSharp, LanguageId::Go, LanguageId::Java,
        ] {
            let s = id.as_str();
            let parsed = LanguageId::from_str(s);
            assert_eq!(parsed, Some(id), "roundtrip failed for {}", s);
        }
    }

    #[test]
    fn test_language_by_extension() {
        assert_eq!(language_by_extension("rs").unwrap().id, LanguageId::Rust);
        assert_eq!(language_by_extension("cs").unwrap().id, LanguageId::CSharp);
        assert_eq!(language_by_extension("tsx").unwrap().id, LanguageId::TypeScriptReact);
        assert!(language_by_extension("xyz").is_none());
    }

    #[test]
    fn test_detect_language_by_path() {
        use std::path::PathBuf;
        assert_eq!(detect_language(&PathBuf::from("src/main.rs")).unwrap().id, LanguageId::Rust);
        assert_eq!(detect_language(&PathBuf::from("Program.cs")).unwrap().id, LanguageId::CSharp);
        assert_eq!(detect_language(&PathBuf::from("Cargo.toml")).unwrap().id, LanguageId::Rust);
        assert_eq!(detect_language(&PathBuf::from("go.mod")).unwrap().id, LanguageId::Go);
        assert_eq!(detect_language(&PathBuf::from("app.tsx")).unwrap().id, LanguageId::TypeScriptReact);
    }

    #[test]
    fn test_classify_file() {
        use std::path::PathBuf;
        assert_eq!(classify_file(&PathBuf::from("src/main.rs")), FileClass::Source);
        assert_eq!(classify_file(&PathBuf::from("tests/test_main.rs")), FileClass::Test);
        assert_eq!(classify_file(&PathBuf::from("vendor/lib/foo.rs")), FileClass::Vendor);
        assert_eq!(classify_file(&PathBuf::from("node_modules/pkg/index.js")), FileClass::Vendor);
        assert_eq!(classify_file(&PathBuf::from("target/debug/binary")), FileClass::Dependency);
        assert_eq!(classify_file(&PathBuf::from("README.md")), FileClass::Documentation);
        assert_eq!(classify_file(&PathBuf::from("image.png")), FileClass::Binary);
    }

    #[test]
    fn test_has_parser() {
        assert!(LanguageId::Rust.has_parser());
        assert!(LanguageId::CSharp.has_parser());
        assert!(LanguageId::Python.has_parser());
        assert!(!LanguageId::Ruby.has_parser());
        assert!(!LanguageId::Markdown.has_parser());
    }

    #[test]
    fn test_registry_consistency() {
        // Every extension in EXTENSION_MAP should map to a known LanguageId
        for def in LANGUAGE_REGISTRY {
            for ext in def.extensions {
                assert!(language_by_extension(ext).is_some(), "extension {} not found", ext);
            }
        }
    }

    #[test]
    fn test_parser_registry_new_has_all_builtins() {
        let registry = ParserRegistry::new();
        let langs = registry.list_available_languages();
        assert!(langs.len() >= 30, "should have all built-in languages");
        // Verify key languages are present
        assert!(langs.iter().any(|(id, _)| *id == LanguageId::Rust));
        assert!(langs.iter().any(|(id, _)| *id == LanguageId::Python));
        assert!(langs.iter().any(|(id, _)| *id == LanguageId::CSharp));
    }

    #[test]
    fn test_parser_registry_get_language() {
        let registry = ParserRegistry::new();
        assert!(registry.get_language(LanguageId::Rust).is_some());
        assert!(registry.get_language(LanguageId::Python).is_some());
        assert!(registry.get_language(LanguageId::Markdown).is_none());
    }

    #[test]
    fn test_parser_registry_register_custom_language() {
        let mut registry = ParserRegistry::new();
        let def = LanguageDefinition::new(LanguageId::Zig, &["zig"], &["build.zig"]);
        // Zig is already registered, so register_language should return false
        assert!(!registry.register_language(def, None));
        // Verify list_with_parsers returns languages with loaded grammars
        let with_parsers = registry.list_with_parsers();
        assert!(with_parsers.contains(&LanguageId::Rust));
    }

    #[test]
    fn test_parser_registry_detect_language() {
        use std::path::PathBuf;
        let registry = ParserRegistry::new();
        assert_eq!(registry.detect_language(&PathBuf::from("src/main.rs")).unwrap().id, LanguageId::Rust);
        assert_eq!(registry.detect_language(&PathBuf::from("app.py")).unwrap().id, LanguageId::Python);
        assert!(registry.detect_language(&PathBuf::from("unknown.xyz")).is_none());
    }

    #[test]
    fn test_parser_registry_language_by_extension() {
        let registry = ParserRegistry::new();
        assert_eq!(registry.language_by_extension("rs").unwrap().id, LanguageId::Rust);
        assert_eq!(registry.language_by_extension("cs").unwrap().id, LanguageId::CSharp);
        assert!(registry.language_by_extension("xyz").is_none());
    }
}
