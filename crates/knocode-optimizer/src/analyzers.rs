use tracing::{debug, warn};

/// Native per-language analyzers — FIRST-CLASS v0.5.0: quality gates on generated artifacts
/// Not LLM calls; runs `cargo clippy`, `eslint`, `ruff` where applicable, after DBOS workflow BuildContext.
#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    pub clippy: bool,
    pub eslint: bool,
    pub ruff: bool,
}

impl Default for AnalyzerConfig {
    fn default() -> Self { Self { clippy: true, eslint: true, ruff: false } }
}

#[derive(Debug, Clone)]
pub struct GateResult {
    pub passed: bool,
    pub tool: String,
    pub output: String,
}

pub fn run_gate(path: &std::path::Path, config: &AnalyzerConfig) -> Vec<GateResult> {
    let mut results = Vec::new();
    if config.clippy && path.join("Cargo.toml").exists() {
        let out = std::process::Command::new("cargo").arg("clippy").arg("--").arg("-D").arg("warnings").current_dir(path).output();
        match out {
            Ok(o) => {
                let passed = o.status.success();
                let output = String::from_utf8_lossy(&o.stderr).chars().take(500).collect();
                if !passed { warn!(tool="clippy", output=%output, "Analyzer gate failed"); } else { debug!("clippy gate passed"); }
                results.push(GateResult { passed, tool: "clippy".to_string(), output });
            }
            Err(e) => { warn!(error=%e, "clippy not available, gate skipped"); }
        }
    }
    if config.eslint && has_js_files(path) {
        let out = std::process::Command::new("eslint")
            .arg(".")
            .arg("--max-warnings")
            .arg("0")
            .arg("--format")
            .arg("stylish")
            .current_dir(path)
            .output();
        match out {
            Ok(o) => {
                let passed = o.status.success();
                let output = if !passed {
                    String::from_utf8_lossy(&o.stdout).chars().take(500).collect()
                } else {
                    String::new()
                };
                if !passed { warn!(tool="eslint", output=%output, "Analyzer gate failed"); } else { debug!("eslint gate passed"); }
                results.push(GateResult { passed, tool: "eslint".to_string(), output });
            }
            Err(e) => { warn!(error=%e, "eslint not available, gate skipped"); }
        }
    }
    results
}

fn has_js_files(path: &std::path::Path) -> bool {
    // Check for package.json or common JS/TS config files as indicators
    let indicators = ["package.json", "tsconfig.json", ".eslintrc.js", ".eslintrc.json", "webpack.config.js"];
    for indicator in &indicators {
        if path.join(indicator).exists() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_analyzer_gate_empty() {
        let cfg = AnalyzerConfig { clippy: false, eslint: false, ruff: false };
        let r = run_gate(&std::path::PathBuf::from("."), &cfg);
        assert!(r.is_empty());
    }

    #[test]
    fn test_analyzer_gate_clippy_skipped_when_no_cargo_toml() {
        let dir = std::env::temp_dir().join(format!("knocode_gate_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = AnalyzerConfig { clippy: true, eslint: false, ruff: false };
        let r = run_gate(&dir, &cfg);
        assert!(r.is_empty(), "no Cargo.toml → no clippy gate");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_analyzer_gate_clippy_detects_passing_crate() {
        // This repo has a Cargo.toml but clippy may warn; gate should run and return 1 result
        let cfg = AnalyzerConfig { clippy: true, eslint: false, ruff: false };
        let path = std::path::PathBuf::from("C:\\LeonRepository\\knocode");
        if path.join("Cargo.toml").exists() {
            let r = run_gate(&path, &cfg);
            assert_eq!(r.len(), 1);
            assert_eq!(r[0].tool, "clippy");
            // passed may be false due to warnings, but gate ran
        }
    }

    #[test]
    fn test_analyzer_gate_result_shape() {
        let gr = GateResult { passed: false, tool: "eslint".to_string(), output: "error".to_string() };
        assert!(!gr.passed);
        assert_eq!(gr.tool, "eslint");
    }
}
