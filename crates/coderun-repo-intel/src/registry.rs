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

/// Get tree-sitter language for a LanguageId
pub fn get_ts_language(id: LanguageId) -> Option<Language> {
    match id {
        LanguageId::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
        LanguageId::TypeScript | LanguageId::TypeScriptReact => {
            Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        }
        LanguageId::JavaScript | LanguageId::JavaScriptReact => {
            Some(tree_sitter_javascript::LANGUAGE.into())
        }
        LanguageId::Python => Some(tree_sitter_python::LANGUAGE.into()),
        LanguageId::CSharp => Some(tree_sitter_c_sharp::LANGUAGE.into()),
        #[cfg(feature = "extended-languages")]
        LanguageId::Go => Some(tree_sitter_go::LANGUAGE.into()),
        #[cfg(feature = "extended-languages")]
        LanguageId::Java => Some(tree_sitter_java::LANGUAGE.into()),
        #[cfg(feature = "extended-languages")]
        LanguageId::C => Some(tree_sitter_c::LANGUAGE.into()),
        #[cfg(feature = "extended-languages")]
        LanguageId::Cpp => Some(tree_sitter_cpp::LANGUAGE.into()),
        _ => {
            #[cfg(not(feature = "extended-languages"))]
            if matches!(id, LanguageId::Go | LanguageId::Java | LanguageId::C | LanguageId::Cpp) {
                tracing::warn!(language = id.as_str(), "requires --features extended-languages; fallback to regex");
            }
            None
        }
    }
}

/// Static registry of all known languages.
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
    Unknown,
}

/// Directories that indicate generated/vendor/dependency code
const VENDOR_DIRS: &[&str] = &["vendor", "node_modules", "third_party", "extern", "external"];
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
}
