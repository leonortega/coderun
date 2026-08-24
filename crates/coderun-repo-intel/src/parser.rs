use tree_sitter::{Language, Parser};

// ── Tree-sitter Languages ────────────────────────────────────────────────

/// Get tree-sitter language for a given language name
pub fn get_language(language: &str) -> Option<Language> {
    match language {
        "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        "typescript" | "typescriptreact" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "javascript" | "javascriptreact" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "python" => Some(tree_sitter_python::LANGUAGE.into()),
        _ => None,
    }
}

// ── AST Symbol Extraction ────────────────────────────────────────────────

/// Symbol extracted from AST
#[derive(Debug, Clone)]
pub struct AstSymbol {
    pub name: String,
    pub kind: String,
    pub line_start: u32,
    pub line_end: u32,
}

/// Extract symbols from source code using tree-sitter
pub fn extract_symbols_ast(content: &str, language: &str) -> Vec<AstSymbol> {
    let lang = match get_language(language) {
        Some(l) => l,
        None => return Vec::new(),
    };

    let mut parser = Parser::new();
    if parser.set_language(&lang).is_err() {
        return Vec::new();
    }

    let tree = match parser.parse(content, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut symbols = Vec::new();
    extract_symbols_recursive(tree.root_node(), content, &mut symbols);
    symbols
}

/// Recursively extract symbols from AST nodes
fn extract_symbols_recursive(node: tree_sitter::Node, content: &str, symbols: &mut Vec<AstSymbol>) {
    let kind = node.kind();

    match kind {
        // Rust
        "function_item" | "function_signature_item" => {
            if let Some(name) = get_node_name(node, content) {
                symbols.push(AstSymbol {
                    name,
                    kind: "function".to_string(),
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                });
            }
        }
        "struct_item" => {
            if let Some(name) = get_node_name(node, content) {
                symbols.push(AstSymbol {
                    name,
                    kind: "struct".to_string(),
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                });
            }
        }
        "enum_item" => {
            if let Some(name) = get_node_name(node, content) {
                symbols.push(AstSymbol {
                    name,
                    kind: "enum".to_string(),
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                });
            }
        }
        "impl_item" => {
            // Get the type being implemented
            if let Some(type_node) = node.child_by_field_name("type") {
                let type_name = type_node
                    .utf8_text(content.as_bytes())
                    .unwrap_or("Unknown")
                    .to_string();
                symbols.push(AstSymbol {
                    name: type_name,
                    kind: "impl".to_string(),
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                });
            }
        }
        "trait_item" => {
            if let Some(name) = get_node_name(node, content) {
                symbols.push(AstSymbol {
                    name,
                    kind: "trait".to_string(),
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                });
            }
        }
        "type_item" => {
            if let Some(name) = get_node_name(node, content) {
                symbols.push(AstSymbol {
                    name,
                    kind: "type".to_string(),
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                });
            }
        }
        // Python
        "function_definition" => {
            if let Some(name) = get_node_name(node, content) {
                symbols.push(AstSymbol {
                    name,
                    kind: "function".to_string(),
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                });
            }
        }
        "class_definition" => {
            if let Some(name) = get_node_name(node, content) {
                symbols.push(AstSymbol {
                    name,
                    kind: "class".to_string(),
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                });
            }
        }
        // JavaScript/TypeScript
        "function_declaration" | "arrow_function" | "function" => {
            if let Some(name) = get_node_name(node, content) {
                symbols.push(AstSymbol {
                    name,
                    kind: "function".to_string(),
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                });
            }
        }
        "class_declaration" => {
            if let Some(name) = get_node_name(node, content) {
                symbols.push(AstSymbol {
                    name,
                    kind: "class".to_string(),
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                });
            }
        }
        "interface_declaration" => {
            if let Some(name) = get_node_name(node, content) {
                symbols.push(AstSymbol {
                    name,
                    kind: "interface".to_string(),
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                });
            }
        }
        "type_alias_declaration" => {
            if let Some(name) = get_node_name(node, content) {
                symbols.push(AstSymbol {
                    name,
                    kind: "type".to_string(),
                    line_start: node.start_position().row as u32 + 1,
                    line_end: node.end_position().row as u32 + 1,
                });
            }
        }
        _ => {}
    }

    // Recurse into child nodes
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        extract_symbols_recursive(child, content, symbols);
    }
}

/// Get the name of an AST node
fn get_node_name(node: tree_sitter::Node, content: &str) -> Option<String> {
    // Try to get the "name" field first
    if let Some(name_node) = node.child_by_field_name("name") {
        return name_node
            .utf8_text(content.as_bytes())
            .ok()
            .map(|s| s.to_string());
    }

    // For some nodes, the name is the first child
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let kind = child.kind();
        if kind == "identifier" || kind == "type_identifier" {
            return child
                .utf8_text(content.as_bytes())
                .ok()
                .map(|s| s.to_string());
        }
    }

    None
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_symbols() {
        let code = r#"
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

        let symbols = extract_symbols_ast(code, "rust");
        assert!(symbols.iter().any(|s| s.name == "main" && s.kind == "function"));
        assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == "struct"));
        assert!(symbols.iter().any(|s| s.name == "Color" && s.kind == "enum"));
        assert!(symbols.iter().any(|s| s.name == "Drawable" && s.kind == "trait"));
        assert!(symbols.iter().any(|s| s.name == "Result" && s.kind == "type"));
    }

    #[test]
    fn test_python_symbols() {
        let code = r#"
def hello():
    print("Hello")

class Config:
    def __init__(self):
        self.name = "test"

class MyEnum:
    pass
"#;

        let symbols = extract_symbols_ast(code, "python");
        assert!(symbols.iter().any(|s| s.name == "hello" && s.kind == "function"));
        assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == "class"));
        assert!(symbols.iter().any(|s| s.name == "MyEnum" && s.kind == "class"));
    }

    #[test]
    fn test_javascript_symbols() {
        let code = r#"
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

        let symbols = extract_symbols_ast(code, "javascript");
        assert!(symbols.iter().any(|s| s.name == "hello" && s.kind == "function"));
        assert!(symbols.iter().any(|s| s.name == "Config" && s.kind == "class"));
        assert!(symbols.iter().any(|s| s.name == "fetchData" && s.kind == "function"));
    }

    #[test]
    fn test_typescript_symbols() {
        let code = r#"
interface User {
    name: string;
    age: number;
}

type Result<T> = {
    data: T;
    error?: string;
}

function greet(user: User): string {
    return `Hello ${user.name}`;
}
"#;

        let symbols = extract_symbols_ast(code, "typescript");
        assert!(symbols.iter().any(|s| s.name == "User" && s.kind == "interface"));
        assert!(symbols.iter().any(|s| s.name == "Result" && s.kind == "type"));
        assert!(symbols.iter().any(|s| s.name == "greet" && s.kind == "function"));
    }

    #[test]
    fn test_unknown_language() {
        let code = "fn main() {}";
        let symbols = extract_symbols_ast(code, "unknown");
        assert!(symbols.is_empty());
    }
}
