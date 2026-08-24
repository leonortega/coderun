use coderun_core::OutputType;
use tracing::{debug, warn};

/// Execution Optimizer: compresses tool outputs via RTK or type-specific compressors
#[derive(Clone)]
pub struct ExecutionOptimizer {
    config: OptimizerConfig,
}

#[derive(Debug, Clone)]
pub struct OptimizerConfig {
    pub enabled: bool,
    pub max_output_tokens: usize,
    pub compression_level: String,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_output_tokens: 8000,
            compression_level: "balanced".to_string(),
        }
    }
}

/// Result of compression
#[derive(Debug, Clone)]
pub struct CompressedOutput {
    pub original: String,
    pub compressed: String,
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    pub output_type: OutputType,
}

impl ExecutionOptimizer {
    pub fn new(config: OptimizerConfig) -> Self {
        Self { config }
    }

    /// Compress tool output based on its type
    pub fn compress_output(
        &self,
        tool_name: &str,
        output_type: OutputType,
        content: String,
        _context: Option<&str>,
    ) -> CompressedOutput {
        if !self.config.enabled {
            let token_count = estimate_tokens(&content);
            return CompressedOutput {
                original: content.clone(),
                compressed: content,
                original_tokens: token_count,
                compressed_tokens: token_count,
                output_type,
            };
        }

        let original_tokens = estimate_tokens(&content);

        let compressed = match &output_type {
            OutputType::FileRead => compress_file_read(&content, &self.config.compression_level),
            OutputType::SearchResult => compress_search_result(&content),
            OutputType::ShellOutput => compress_shell_output(&content),
            OutputType::Other => compress_generic(&content, &self.config.compression_level),
        };

        let compressed_tokens = estimate_tokens(&compressed);

        let ratio = if original_tokens > 0 {
            compressed_tokens as f64 / original_tokens as f64
        } else {
            1.0
        };

        debug!(
            tool = tool_name,
            original_tokens = original_tokens,
            compressed_tokens = compressed_tokens,
            ratio = ratio,
            "Compressed tool output"
        );

        CompressedOutput {
            original: content,
            compressed,
            original_tokens,
            compressed_tokens,
            output_type,
        }
    }

    /// Tee-on-failure: on compression failure, return original content
    pub fn compress_with_fallback(
        &self,
        tool_name: &str,
        output_type: OutputType,
        content: String,
        _context: Option<&str>,
    ) -> CompressedOutput {
        // Try compression; on any failure, return original (fail-open)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.compress_output(tool_name, output_type.clone(), content.clone(), _context)
        }));

        match result {
            Ok(compressed) => compressed,
            Err(_) => {
                warn!(
                    tool = tool_name,
                    "Compression failed, returning original (fail-open)"
                );
                let token_count = estimate_tokens(&content);
                CompressedOutput {
                    original: content.clone(),
                    compressed: content,
                    original_tokens: token_count,
                    compressed_tokens: token_count,
                    output_type,
                }
            }
        }
    }
}

// ── Type-Specific Compressors ───────────────────────────────────────────

/// Compress file read output: remove imports-only lines, preserve definitions
fn compress_file_read(content: &str, level: &str) -> String {
    let max_lines = match level {
        "light" => 500,
        "aggressive" => 100,
        _ => 200, // balanced
    };

    let mut result: Vec<String> = Vec::new();
    let mut consecutive_empty = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip consecutive empty lines
        if trimmed.is_empty() {
            consecutive_empty += 1;
            if consecutive_empty <= 1 {
                result.push(line.to_string());
            }
            continue;
        }
        consecutive_empty = 0;

        // Skip pure import/include lines in aggressive mode
        if level == "aggressive" && is_import_line(trimmed) {
            continue;
        }

        result.push(line.to_string());

        if result.len() >= max_lines {
            result.push("... [truncated]".to_string());
            break;
        }
    }

    result.join("\n")
}

/// Compress search results: group by file, keep top results
fn compress_search_result(content: &str) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut _current_file = String::new();
    let mut file_matches = 0;
    let max_per_file = 5;

    for line in content.lines() {
        let trimmed = line.trim();

        // Detect file headers
        if is_file_header(trimmed) {
            _current_file = trimmed.to_string();
            file_matches = 0;
            result.push(line.to_string());
            continue;
        }

        if file_matches < max_per_file {
            result.push(line.to_string());
            file_matches += 1;
        }
    }

    result.join("\n")
}

/// Compress shell output: remove ANSI codes, preserve errors/warnings
fn compress_shell_output(content: &str) -> String {
    let mut result: Vec<String> = Vec::new();

    for line in content.lines() {
        let cleaned = strip_ansi_codes(line);

        // Skip progress indicators
        if cleaned.contains('\r')
            || (cleaned.contains('%')
                && cleaned
                    .trim()
                    .chars()
                    .all(|c| c.is_numeric() || c == '%' || c == ' '))
        {
            continue;
        }

        // Always keep errors and warnings (use cleaned version)
        let lower = cleaned.to_lowercase();
        if lower.contains("error") || lower.contains("warning") || lower.contains("fatal") {
            result.push(cleaned);
            continue;
        }

        // Skip very repetitive lines
        if is_repetitive(&cleaned) {
            continue;
        }

        result.push(cleaned);
    }

    // Keep the last few lines (often the most important)
    let total = result.len();
    if total > 50 {
        let keep = 20;
        let mut truncated = Vec::new();
        let first_5: Vec<&str> = result[..5].iter().map(|s| s.as_str()).collect();
        truncated.push(first_5.join("\n"));
        truncated.push("... [middle truncated] ...".to_string());
        let last_20: Vec<&str> = result[total - keep..].iter().map(|s| s.as_str()).collect();
        truncated.push(last_20.join("\n"));
        truncated.join("\n")
    } else {
        result.join("\n")
    }
}

/// Compress generic content: truncate to max lines
fn compress_generic(content: &str, level: &str) -> String {
    let max_lines = match level {
        "light" => 300,
        "aggressive" => 50,
        _ => 100,
    };

    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= max_lines {
        content.to_string()
    } else {
        let mut result: Vec<&str> = lines[..max_lines].to_vec();
        result.push("... [truncated]");
        result.join("\n")
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Estimate token count (rough: 1 token ≈ 4 chars or ≈ 0.75 words)
fn estimate_tokens(text: &str) -> usize {
    let char_count = text.len();
    let word_count = text.split_whitespace().count();
    let by_chars = char_count / 4;
    let by_words = (word_count as f64 * 1.3) as usize;
    by_chars.max(by_words)
}

/// Check if a line is an import/include statement
fn is_import_line(line: &str) -> bool {
    line.starts_with("use ")
        || line.starts_with("import ")
        || line.starts_with("from ")
        || line.starts_with("include!")
        || line.starts_with("#include")
        || line.starts_with("require ")
        || line.starts_with("require_relative")
}

/// Check if a line looks like a file header in search results
fn is_file_header(line: &str) -> bool {
    (line.ends_with(':') && !line.contains(' ') && line.contains('/'))
        || line.starts_with("== ")
        || line.starts_with("-- ")
}

/// Strip ANSI escape codes from a string
fn strip_ansi_codes(s: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").to_string()
}

/// Check if a line is repetitive (same character repeated)
fn is_repetitive(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    let first = trimmed.chars().next().unwrap();
    trimmed.chars().all(|c| c == first || c == ' ' || c == '\t')
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_optimizer() -> ExecutionOptimizer {
        ExecutionOptimizer::new(OptimizerConfig::default())
    }

    #[test]
    fn test_compress_file_read() {
        let content = "use std::collections::HashMap;\nuse std::io;\n\nfn main() {\n    println!(\"Hello\");\n}\n";
        let compressed = compress_file_read(content, "balanced");
        assert!(compressed.contains("fn main()"));
    }

    #[test]
    fn test_compress_shell_output_removes_ansi() {
        let content = "\x1b[31mError\x1b[0m: something failed\n\x1b[32mOK\x1b[0m: done\n";
        let compressed = compress_shell_output(content);
        assert!(!compressed.contains("\x1b["));
    }

    #[test]
    fn test_compress_shell_output_keeps_errors() {
        let content = "Compiling...\nerror[E0001]: something\nDone.\n";
        let compressed = compress_shell_output(content);
        assert!(compressed.contains("error[E0001]"));
    }

    #[test]
    fn test_compress_generic_truncation() {
        let content = (0..200)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let compressed = compress_generic(&content, "balanced");
        let lines: Vec<&str> = compressed.lines().collect();
        assert!(lines.len() <= 101); // 100 + truncation message
    }

    #[test]
    fn test_estimate_tokens() {
        assert!(estimate_tokens("hello world") > 0);
        assert!(estimate_tokens("") == 0);
        assert!(estimate_tokens(&"a".repeat(100)) > 20);
    }

    #[test]
    fn test_is_import_line() {
        assert!(is_import_line("use std::io;"));
        assert!(is_import_line("import os"));
        assert!(is_import_line("#include <stdio.h>"));
        assert!(!is_import_line("fn main() {}"));
    }

    #[test]
    fn test_strip_ansi_codes() {
        assert_eq!(strip_ansi_codes("\x1b[31mError\x1b[0m"), "Error");
        assert_eq!(strip_ansi_codes("no codes"), "no codes");
    }

    #[test]
    fn test_optimizer_disabled_passthrough() {
        let config = OptimizerConfig {
            enabled: false,
            ..Default::default()
        };
        let optimizer = ExecutionOptimizer::new(config);
        let result = optimizer.compress_output("test", OutputType::FileRead, "hello".to_string(), None);
        assert_eq!(result.original, "hello");
        assert_eq!(result.compressed, "hello");
        assert_eq!(result.original_tokens, result.compressed_tokens);
    }

    #[test]
    fn test_compress_with_fallback() {
        let optimizer = default_optimizer();
        let result = optimizer.compress_with_fallback(
            "test",
            OutputType::ShellOutput,
            "line1\nline2\nline3".to_string(),
            None,
        );
        assert!(!result.compressed.is_empty());
    }

    #[test]
    fn test_is_repetitive() {
        assert!(is_repetitive("=========="));
        assert!(!is_repetitive("normal text"));
        assert!(!is_repetitive("    "));  // blank lines are not repetitive
    }

    #[test]
    fn test_compression_stats() {
        let optimizer = default_optimizer();
        let content = "use std::io;\nuse std::fs;\n\nfn main() {\n    println!(\"test\");\n}";
        let result = optimizer.compress_output("test", OutputType::FileRead, content.to_string(), None);
        assert!(result.original_tokens > 0);
        assert!(result.compressed_tokens > 0);
    }
}
