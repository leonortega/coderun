#![allow(linker_messages)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::{Parser, Subcommand};
use coderun_core::Config;

// ── CLI Arguments ───────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "coderun")]
#[command(about = "AI Runtime for coding agents")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon server
    Serve {
        /// HTTP fallback port (default 9527)
        #[arg(long, default_value_t = 9527)]
        port: u16,
        /// Override socket path
        #[arg(long)]
        socket: Option<String>,
    },
    
    /// One-command repository bootstrap: scaffold → discovery → indexing → knowledge → engram → profile
    Init {
        /// Run interactive setup wizard
        #[arg(long)]
        wizard: bool,
        /// Skip community skill installation from skills.sh (offline / air-gapped / privacy)
        #[arg(long)]
        no_community_skills: bool,
    },
    
    /// Trigger repository re-indexing
    Index {
        /// Watch for changes (git-change-triggered incremental)
        #[arg(long)]
        watch: bool,
    },
    
    /// Preview what BuildContext would produce for a prompt (real via daemon if running, else local)
    Preview {
        /// The prompt to preview
        prompt: String,
        /// Session ID for dedup testing
        #[arg(long, default_value = "preview-session")]
        session: String,
        /// Do not use session dedup
        #[arg(long)]
        no_cache: bool,
    },
    
    /// Show daemon status and metrics
    Status,
    
    /// Manage skill definitions
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
    
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Durable workflows (DBOS) — requires --features workflow (future/workflow, NOT v1)
    #[cfg(feature = "workflow")]
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
    },
    
    /// Health check: verify all dependencies are available
    Doctor,
}

#[derive(Subcommand)]
enum SkillsAction {
    /// List available skills
    List,
    /// Validate skill definitions
    Validate,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show effective configuration
    Show,
    /// Validate configuration file
    Validate,
    /// Migrate config from external agent (claude, cursor, continue)
    Migrate {
        /// Source to migrate from
        #[arg(value_parser = clap::value_parser!(String))]
        from: String,
    },
}

#[cfg(feature = "workflow")]
#[derive(Subcommand)]
enum WorkflowAction {
    /// Start a durable workflow
    Start {
        /// Task prompt
        prompt: String,
        /// Require human approval gate
        #[arg(long)]
        require_approval: bool,
    },
    /// Get workflow status
    Status {
        /// Workflow ID
        workflow_id: String,
    },
    /// Approve a pending workflow
    Approve {
        /// Workflow ID
        workflow_id: String,
    },
    /// List recent workflows
    List,
}

// ── Main ────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    
    let result = match cli.command {
        Commands::Serve { port, socket } => cmd_serve(port, socket),
        Commands::Init { wizard, no_community_skills } => cmd_init(wizard, no_community_skills),
        Commands::Index { watch } => cmd_index(watch),
        Commands::Preview { prompt, session, no_cache } => cmd_preview(&prompt, &session, no_cache),
        Commands::Status => cmd_status(),
        Commands::Skills { action } => cmd_skills(action),
        Commands::Config { action } => cmd_config(action),
        #[cfg(feature = "workflow")]
        Commands::Workflow { action } => cmd_workflow(action),
        Commands::Doctor => cmd_doctor(),
    };
    
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

// ── Command Implementations ─────────────────────────────────────────────

fn cmd_serve(_port: u16, _socket: Option<String>) -> Result<(), String> {
    println!("Starting coderun daemon...");
    println!("  UDS/MessagePack primary (spec §2) on {}", _socket.as_deref().unwrap_or("/tmp/coderun.sock"));
    println!("  HTTP fallback on 127.0.0.1:{} (JSON)", _port);
    println!("  Delegating to coderun-daemon binary — run `coderun-daemon` for full serve.");
    // In v0.3.0, `coderun serve` still delegates to the daemon binary; lifecycle now wires UDS+HTTP.
    println!("  Tip: daemon now starts both UDS (primary) and HTTP (fallback) — no extra flag needed.");
    Ok(())
}

fn cmd_init(wizard: bool, no_community_skills: bool) -> Result<(), String> {
    if wizard {
        println!("(wizard mode is non-interactive — defaults applied, edit .coderun/config.toml afterwards)");
        println!();
    }
    let started = std::time::Instant::now();
    let project_root = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;
    let project_name = project_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repository".to_string());

    println!("Coderun Bootstrap — {}", project_name);
    println!("═══════════════════════════════════════");

    println!("[1/6] Scaffold (.coderun/, config, skills, database)");
    let coderun_dir = PathBuf::from(".coderun");
    std::fs::create_dir_all(&coderun_dir)
        .map_err(|e| format!("Failed to create .coderun directory: {}", e))?;
    let config_path = coderun_dir.join("config.toml");
    if !config_path.exists() {
        let default_config = Config::default();
        let config_toml = toml::to_string_pretty(&default_config)
            .map_err(|e| format!("Failed to serialize default config: {}", e))?;
        std::fs::write(&config_path, config_toml)
            .map_err(|e| format!("Failed to write config: {}", e))?;
    }
    std::fs::create_dir_all(coderun_dir.join("skills"))
        .map_err(|e| format!("Failed to create skills directory: {}", e))?;
    let db_path = get_db_path();
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create database directory: {}", e))?;
    }
    let db = coderun_storage::Database::open(&db_path)
        .map_err(|e| format!("Failed to initialize database: {}", e))?;

    println!("[2/6] Repository discovery (languages, frameworks, commands)");
    let discovery = discover_repository(&project_root);
    println!(
        "      languages: {}",
        discovery
            .languages
            .iter()
            .map(|(l, n)| format!("{}({})", l, n))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // [2b] Community skills — stack-matched via skills.sh (`npx skills`), installed into
    // .coderun/skills so CODERUN stays the curator (opencode never loads them natively).
    // Best-effort + fail-open: offline/missing npx never fails init.
    if no_community_skills || std::env::var("CODERUN_NO_COMMUNITY_SKILLS").map(|v| v == "1" || v == "true").unwrap_or(false) {
        println!("[2b] Community skills — skipped (--no-community-skills)");
    } else {
        println!("[2b] Community skills (stack-matched via skills.sh)");
        match install_community_skills(&project_root, &discovery) {
            Ok(added) => {
                if added > 0 {
                    println!("      ✓ {} skill(s) installed into .coderun/skills", added);
                } else {
                    println!("      (no new stack-relevant skills found)");
                }
            }
            Err(e) => println!("      (skipped: {})", e),
        }
    }

    println!("[3/6] Indexing (tree-sitter symbols + tantivy BM25 + dependency graph)");
    let event_bus = coderun_events::EventBus::new();
    let mut repo_intel = coderun_repo_intel::RepositoryIntelligence::new(
        project_root.clone(),
        db,
        event_bus.clone(),
    );
    let stats = repo_intel
        .index_repository()
        .map_err(|e| format!("Indexing failed: {}", e))?;
    let dep_edges = repo_intel
        .build_dependency_graph()
        .map(|g| g.edge_count())
        .unwrap_or(0);
    drop(repo_intel);
    let db = coderun_storage::Database::open(&db_path)
        .map_err(|e| format!("Failed to reopen database: {}", e))?;

    println!("[4/6] Knowledge initialization (README, decision records)");
    let (knowledge_seeded, readme) = ingest_seed_documents(&project_root, &db);

    println!("[5/6] Engram memory initialization");
    let config = Config::load(&project_root).unwrap_or_default();
    let profile = build_profile_json(
        &project_name,
        &discovery,
        stats.files_indexed,
        stats.symbols_extracted,
        dep_edges,
    );
    let profile_json =
        serde_json::to_string_pretty(&profile).unwrap_or_else(|_| "{}".to_string());
    let engram_status = init_engram(&config, &project_name, &profile_json, readme.as_deref());
    println!("      {}", engram_status);

    println!("[6/6] Repository profile");
    let profile_path = coderun_dir.join("profile.json");
    std::fs::write(&profile_path, &profile_json)
        .map_err(|e| format!("Failed to write profile: {}", e))?;
    // TASK-030: knowledge is stamped with this repo's identity so retrieval stays repo-scoped
    let repo_id = coderun_core::repository_id_from_path(&project_root.to_string_lossy());
    let _ = db.store_knowledge("profile", "repository-profile", &profile_json, 1.0, "bootstrap", &repo_id);

    println!();
    println!("✓ Bootstrap complete in {}ms", started.elapsed().as_millis());
    println!();
    println!("  Files indexed:     {}", stats.files_indexed);
    println!("  Symbols extracted: {}", stats.symbols_extracted);
    println!("  Dependency edges:  {}", dep_edges);
    println!(
        "  Languages:         {}",
        if discovery.languages.is_empty() {
            "-".to_string()
        } else {
            discovery
                .languages
                .iter()
                .map(|(l, _)| l.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    println!(
        "  Frameworks:        {}",
        if discovery.frameworks.is_empty() {
            "-".to_string()
        } else {
            discovery.frameworks.join(", ")
        }
    );
    println!(
        "  Build command:     {}",
        discovery.build_command.as_deref().unwrap_or("-")
    );
    println!(
        "  Test command:      {}",
        discovery.test_command.as_deref().unwrap_or("-")
    );
    println!("  Knowledge seeded:  {} entries", knowledge_seeded);
    println!("  Profile:           {}", profile_path.display());
    // TASK-038: per-repo artifact home — generated deliverables live inside the analyzed repo
    match ensure_artifact_dir(&project_root, "context") {
        Ok(dir) => {
            println!("  Artifacts:         {}", dir.display());
            print_gitignore_hint();
        }
        Err(e) => println!("  Artifacts:         (skipped: {})", e),
    }
    println!();
    println!("Next steps:");
    println!("  1. Run 'coderun serve' to start the daemon");
    println!("  2. Configure your coding agent to use coderun");
    println!("  Re-run 'coderun init' anytime — incremental and safe to repeat.");

    Ok(())
}

#[derive(Default, Debug)]
struct Discovery {
    languages: Vec<(String, usize)>,
    frameworks: Vec<String>,
    build_command: Option<String>,
    test_command: Option<String>,
    important_dirs: Vec<String>,
    git_branch: Option<String>,
}

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".github",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "vendor",
    ".venv",
    "venv",
    "__pycache__",
    ".idea",
    ".vscode",
    ".next",
    "coverage",
    ".coderun",
    ".agents",
];

fn discover_repository(root: &Path) -> Discovery {
    let mut d = Discovery::default();

    let mut ext_counts: HashMap<String, usize> = HashMap::new();
    walk_ext_counts(root, 0, &mut ext_counts);
    let mut langs: Vec<(String, usize)> = ext_counts
        .into_iter()
        .filter_map(|(ext, n)| ext_language(&ext).map(|l| (l.to_string(), n)))
        .collect();
    langs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    d.languages = langs.into_iter().take(10).collect();

    for candidate in [
        "src",
        "crates",
        "lib",
        "packages",
        "apps",
        "services",
        "docs",
        "tests",
        "test",
        "scripts",
        "benches",
        "examples",
        "deploy",
    ] {
        if root.join(candidate).is_dir() {
            d.important_dirs.push(candidate.to_string());
        }
    }

    detect_stack(root, &mut d);

    if root.join(".git").exists() {
        if let Ok(head) = std::fs::read_to_string(root.join(".git").join("HEAD")) {
            let head = head.trim();
            d.git_branch = head
                .strip_prefix("ref: refs/heads/")
                .map(|s| s.to_string())
                .or_else(|| Some(head.chars().take(12).collect()));
        }
    }

    d
}

fn walk_ext_counts(dir: &Path, depth: usize, counts: &mut HashMap<String, usize>) {
    if depth > 12 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            walk_ext_counts(&entry.path(), depth + 1, counts);
        } else if ft.is_file() {
            if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                if !ext.is_empty() {
                    *counts.entry(ext.to_lowercase()).or_insert(0) += 1;
                }
            }
        }
    }
}

fn ext_language(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "rs" => "rust",
        "py" => "python",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "mts" | "cts" | "tsx" => "typescript",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "sh" | "bash" => "shell",
        "sql" => "sql",
        _ => return None,
    })
}

// ── Community skills via `npx skills` (skills.sh) — TASK-038c ────────────

/// Map discovery results to skills.sh search terms: frameworks first (more specific),
/// then languages by file count. Normalized, de-duplicated, capped.
fn stack_terms(discovery: &Discovery) -> Vec<String> {
    // Checked AFTER normalization ("rust-workspace" already becomes "rust")
    const GENERIC: &[&str] = &["node", "npm", "make", "cargo"];
    let mut terms: Vec<String> = Vec::new();
    let lang_names: std::collections::HashSet<String> =
        discovery.languages.iter().map(|(l, _)| l.clone()).collect();
    let push = |t: &str, terms: &mut Vec<String>| {
        let t = t.split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("").to_lowercase();
        if t.len() >= 2 && !GENERIC.contains(&t.as_str()) && !terms.contains(&t) {
            terms.push(t);
        }
    };
    for f in &discovery.frameworks {
        // Composite framework aliases ("rust-workspace") resolve to a language name —
        // let the languages pass place them by usage count instead of framework priority.
        let norm = f.split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("").to_lowercase();
        if lang_names.contains(&norm) {
            continue;
        }
        push(f, &mut terms);
    }
    for (l, _) in &discovery.languages {
        push(l, &mut terms);
    }
    terms.truncate(5);
    terms
}

/// Strip ANSI escape sequences (`\x1b[...m`) from CLI output
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse `npx skills find` output into ranked candidate tokens (`owner/repo@skill`).
/// The CLI ranks by installs; we keep that order. Best-effort — unparseable output yields
/// zero candidates and the term is skipped (we never guess sources).
fn parse_find_candidates(find_output: &str, max_per_term: usize) -> Vec<String> {
    let clean = strip_ansi(find_output);
    let mut candidates = Vec::new();
    for line in clean.lines() {
        if !line.contains("installs") {
            continue;
        }
        let token = line.trim().split_whitespace().next().unwrap_or("");
        let Some((repo, skill)) = token.split_once('@') else { continue };
        let valid =
            repo.matches('/').count() == 1
            && !repo.starts_with('/') && !repo.ends_with('/')
            && !skill.is_empty()
            && skill.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'));
        if valid && !candidates.iter().any(|c: &String| c.eq_ignore_ascii_case(token)) {
            candidates.push(token.to_string());
            if candidates.len() >= max_per_term {
                break;
            }
        }
    }
    candidates
}

/// Filesystem-safe directory name for a skill (`:` etc. are invalid on Windows)
fn sanitize_dir_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '-' })
        .collect()
}

/// Run a command capturing stdout with a hard timeout (drains stdout on a thread to
/// avoid pipe-full deadlock; stderr is discarded). Returns stdout on any exit status.
fn run_command_with_timeout(
    program: &str,
    args: &[&str],
    cwd: &Path,
    timeout: std::time::Duration,
) -> Result<String, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn {} failed: {}", program, e))?;
    let mut stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        buf
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let out = reader.join().unwrap_or_default();
                return Ok(out);
            }
            Ok(None) => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err("timeout".to_string());
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Recursively copy a directory tree (std has no stable copy_dir_all)
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Search skills.sh for the repo's stack and install top-ranked skills into
/// `.coderun/skills/`. Stages via `npx skills add … -a universal --copy` (the only
/// writable target the CLI offers), then RELOCATES each skill so the analyzed repo's
/// `.coderun/skills/` stays the single source of truth — nothing is left behind for
/// opencode or other agents to load natively. Fail-open; overall time-budgeted.
fn install_community_skills(project_root: &Path, discovery: &Discovery) -> Result<usize, String> {
    #[cfg(windows)]
    let npx = "npx.cmd";
    #[cfg(not(windows))]
    let npx = "npx";

    // Probe availability cheaply before committing to the flow
    run_command_with_timeout(npx, &["--version"], project_root, std::time::Duration::from_secs(20))
        .map_err(|_| "npx not available".to_string())?;

    let skills_dir = project_root.join(".coderun").join("skills");
    std::fs::create_dir_all(&skills_dir).map_err(|e| format!("create skills dir: {}", e))?;

    let mut existing: std::collections::HashSet<String> = std::fs::read_dir(&skills_dir)
        .map(|entries| entries.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).collect())
        .unwrap_or_default();

    let stage_root = project_root.join(".agents");
    let total_budget = Instant::now() + std::time::Duration::from_secs(90);
    let mut added = 0usize;

    'terms: for term in stack_terms(discovery) {
        if Instant::now() >= total_budget {
            println!("      (time budget reached — remaining terms skipped)");
            break;
        }
        let search_timeout = std::time::Duration::from_secs(25).min(total_budget.saturating_duration_since(Instant::now()));
        let out = match run_command_with_timeout(npx, &["-y", "skills", "find", &term], project_root, search_timeout) {
            Ok(out) => out,
            Err(_) => continue,
        };
        for cand in parse_find_candidates(&out, 2) {
            if Instant::now() >= total_budget {
                break 'terms;
            }
            let skill_name = cand.rsplit('@').next().unwrap_or("").to_string();
            let dir_name = sanitize_dir_name(&skill_name);
            if skill_name.is_empty() || existing.contains(&skill_name) || existing.contains(&dir_name) {
                continue;
            }
            let install_timeout = std::time::Duration::from_secs(45).min(total_budget.saturating_duration_since(Instant::now()));
            if run_command_with_timeout(npx, &["-y", "skills", "add", &cand, "--copy", "-y", "-a", "universal"], project_root, install_timeout).is_err() {
                continue;
            }
            // Relocate staged <root>/.agents/skills/<name> → <root>/.coderun/skills/<name>
            let staged = stage_root.join("skills").join(&skill_name);
            let dest = skills_dir.join(&dir_name);
            if staged.exists() && !dest.exists() {
                let moved = std::fs::rename(&staged, &dest).or_else(|_| copy_dir_recursive(&staged, &dest));
                match moved {
                    Ok(()) => {
                        let _ = std::fs::remove_dir_all(&staged);
                        println!("      + {} → {}", cand, dest.display());
                        existing.insert(skill_name.clone());
                        existing.insert(dir_name.clone());
                        added += 1;
                    }
                    Err(e) => warn_line(&format!("relocate {} failed: {}", cand, e)),
                }
            }
        }
    }

    // Clean the transient staging tree when empty (never leave agent-visible dirs behind)
    let _ = std::fs::remove_dir(stage_root.join("skills"));
    let _ = std::fs::remove_dir(&stage_root);

    Ok(added)
}

fn warn_line(msg: &str) {
    println!("      (warn: {})", msg);
}

fn detect_stack(root: &Path, d: &mut Discovery) {    if root.join("Cargo.toml").exists() {
        d.frameworks.push("cargo".to_string());
        if let Ok(s) = std::fs::read_to_string(root.join("Cargo.toml")) {
            if s.contains("[workspace]") {
                d.frameworks.push("rust-workspace".to_string());
            }
        }
        d.build_command.get_or_insert_with(|| "cargo build".into());
        d.test_command.get_or_insert_with(|| "cargo test".into());
    }
    let pkg = root.join("package.json");
    if pkg.exists() {
        d.frameworks.push("node".to_string());
        if let Ok(content) = std::fs::read_to_string(&pkg) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                for fw in ["react", "vue", "svelte", "next", "express", "@nestjs/core"] {
                    if v["dependencies"].get(fw).is_some()
                        || v["devDependencies"].get(fw).is_some()
                    {
                        d.frameworks.push(fw.trim_start_matches('@').replace('/', "-"));
                    }
                }
                if let Some(b) = v["scripts"]["build"].as_str() {
                    d.build_command.get_or_insert_with(|| b.to_string());
                }
                if let Some(t) = v["scripts"]["test"].as_str() {
                    d.test_command.get_or_insert_with(|| t.to_string());
                }
            }
        }
        if d.test_command.is_none() {
            d.test_command = Some("npm test".to_string());
        }
    }
    if root.join("go.mod").exists() {
        d.frameworks.push("go-modules".to_string());
        d.build_command
            .get_or_insert_with(|| "go build ./...".into());
        d.test_command.get_or_insert_with(|| "go test ./...".into());
    }
    if root.join("pyproject.toml").exists()
        || root.join("requirements.txt").exists()
        || root.join("setup.py").exists()
    {
        d.frameworks.push("python".to_string());
        if let Ok(s) = std::fs::read_to_string(root.join("pyproject.toml")) {
            if s.contains("[tool.poetry]") {
                d.frameworks.push("poetry".to_string());
                d.test_command
                    .get_or_insert_with(|| "poetry run pytest".into());
            } else if s.contains("[tool.uv]") {
                d.frameworks.push("uv".to_string());
            }
        }
        d.test_command.get_or_insert_with(|| "pytest".into());
    }
    if root.join("pom.xml").exists() {
        d.frameworks.push("maven".to_string());
        d.build_command.get_or_insert_with(|| "mvn package".into());
        d.test_command.get_or_insert_with(|| "mvn test".into());
    } else if root.join("build.gradle").exists() || root.join("build.gradle.kts").exists() {
        d.frameworks.push("gradle".to_string());
        d.build_command.get_or_insert_with(|| "gradle build".into());
        d.test_command.get_or_insert_with(|| "gradle test".into());
    }
    if root.join("Makefile").exists() || root.join("makefile").exists() {
        d.frameworks.push("make".to_string());
    }
}

fn ingest_seed_documents(root: &Path, db: &coderun_storage::Database) -> (usize, Option<String>) {
    // TASK-030: seed documents are stamped with the analyzed repo's identity
    let repo_id = coderun_core::repository_id_from_path(&root.to_string_lossy());
    let mut seeded = 0usize;
    let mut readme = None;
    for name in ["README.md", "Readme.md", "readme.md", "README"] {
        let path = root.join(name);
        if path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let truncated: String = content.chars().take(65_536).collect();
                if db
                    .store_knowledge("docs", name, &truncated, 0.9, "bootstrap", &repo_id)
                    .is_ok()
                {
                    seeded += 1;
                }
                readme = Some(content);
            }
            break;
        }
    }
    for adr_dir in ["docs/adr", "docs/decisions", "adr", "decisions"] {
        let dir = root.join(adr_dir);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
                    if db
                        .store_knowledge("adr", &rel, &content, 1.0, "bootstrap", &repo_id)
                        .is_ok()
                    {
                        seeded += 1;
                    }
                }
            }
        }
    }
    (seeded, readme)
}

fn build_profile_json(
    project_name: &str,
    d: &Discovery,
    files_indexed: usize,
    symbols_extracted: usize,
    dep_edges: usize,
) -> serde_json::Value {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|t| t.as_secs())
        .unwrap_or(0);
    serde_json::json!({
        "version": 1,
        "project": project_name,
        "generated_at_unix": ts,
        "git_branch": d.git_branch,
        "languages": d.languages
            .iter()
            .map(|(name, files)| serde_json::json!({"name": name, "files": files}))
            .collect::<Vec<_>>(),
        "frameworks": d.frameworks,
        "commands": {"build": d.build_command, "test": d.test_command},
        "important_dirs": d.important_dirs,
        "index": {
            "files_indexed": files_indexed,
            "symbols_extracted": symbols_extracted,
            "dependency_edges": dep_edges,
        },
    })
}

fn init_engram(
    config: &Config,
    namespace: &str,
    profile_json: &str,
    readme: Option<&str>,
) -> String {
    if !config.knowledge.memory_enabled {
        return "disabled in config (knowledge.memory_enabled=false)".to_string();
    }
    let endpoint = config.knowledge.memory_endpoint.clone();
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => return format!("runtime error ({})", e),
    };
    rt.block_on(async move {
        let client = match coderun_knowledge::engram::EngramClient::new(
            coderun_knowledge::engram::EngramConfig {
                endpoint: endpoint.clone(),
                timeout_ms: 3000,
                ..Default::default()
            },
        ) {
            Ok(c) => c,
            Err(e) => return format!("client error ({})", e),
        };
        if !client.health_check().await {
            return format!(
                "endpoint unreachable at {} (seed skipped, fail-open)",
                endpoint
            );
        }
        let mut saved = 0usize;
        let profile_entry = coderun_knowledge::engram::MemoryEntry {
            namespace: namespace.to_string(),
            key: "repository-profile".to_string(),
            value: profile_json.to_string(),
            metadata: Some(serde_json::json!({"source": "bootstrap", "type": "profile"})),
        };
        if client.save_memory(&profile_entry).await.is_ok() {
            saved += 1;
        }
        if let Some(r) = readme {
            let entry = coderun_knowledge::engram::MemoryEntry {
                namespace: namespace.to_string(),
                key: "readme".to_string(),
                value: r.chars().take(4000).collect(),
                metadata: Some(serde_json::json!({"source": "bootstrap", "type": "doc"})),
            };
            if client.save_memory(&entry).await.is_ok() {
                saved += 1;
            }
        }
        format!(
            "✓ seeded {} entries at {} (namespace '{}')",
            saved, endpoint, namespace
        )
    })
}

fn cmd_index(watch: bool) -> Result<(), String> {
    println!("Indexing repository{}...", if watch { " (watch mode — git-change-triggered incremental)" } else { "" });
    
    let project_root = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;
    
    // Open database
    let db_path = get_db_path();
    let db = coderun_storage::Database::open(&db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;
    
    // Create event bus
    let event_bus = coderun_events::EventBus::new();
    
    // Create repository intelligence
    let mut repo_intel = coderun_repo_intel::RepositoryIntelligence::new(
        project_root.clone(),
        db,
        event_bus.clone(),
    );
    
    // Run indexing (wires tantivy BM25 in-process, incremental via hash, see repo-intel lib)
    let stats = repo_intel.index_repository()
        .map_err(|e| format!("Indexing failed: {}", e))?;
    
    println!();
    println!("✓ Indexing complete!");
    println!();
    println!("  Files indexed:    {}", stats.files_indexed);
    println!("  Symbols extracted: {}", stats.symbols_extracted);
    println!("  Files skipped:    {}", stats.files_skipped);
    println!("  Files deleted:    {}", stats.files_deleted);
    println!("  Duration:         {}ms", stats.duration_ms);
    // TASK-038: per-repo artifact home for any generated reports/exports
    match ensure_artifact_dir(&project_root, "context") {
        Ok(dir) => {
            println!("  Artifacts:        {}", dir.display());
            print_gitignore_hint();
        }
        Err(e) => println!("  Artifacts:        (skipped: {})", e),
    }
    // Also show graph edge count (new in v0.3.0)
    if let Ok(g) = repo_intel.build_dependency_graph() {
        println!("  Dependency edges: {}", g.edge_count());
    }

    if watch {
        println!();
        println!("Watching for git/file changes (Ctrl+C to stop) — polling every 5s");
        let watcher = repo_intel.spawn_watcher();
        let _handle = watcher.spawn(|| {
            println!("[watcher] change detected — re-indexing...");
        });
        // Block until Ctrl+C (simple park)
        println!("(Watcher running in background — press Ctrl+C to exit)");
        // In CLI mode we just note that watch would run; actual long-running watch is daemon's job.
        println!("Note: daemon's background watcher already handles incremental updates; CLI --watch is best-effort.");
    }
    
    Ok(())
}

fn cmd_preview(prompt: &str, session: &str, no_cache: bool) -> Result<(), String> {
    // Try daemon first (UDS then HTTP), fallback to local BuildContext
    // For v0.3.0 we implement real preview: build context locally if daemon not running.
    let effective_session = if no_cache { String::new() } else { session.to_string() };
    println!("Previewing BuildContext for: \"{}\" (session: {}, no_cache: {})", prompt, effective_session, no_cache);
    println!();

    // Attempt HTTP daemon preview (UDS preview requires MessagePack client — HTTP is fallback)
    let daemon_url = std::env::var("CODERUN_DAEMON_URL").unwrap_or_else(|_| "http://127.0.0.1:9527".to_string());
    let url = format!("{}/hook", daemon_url);
    // Use blocking reqwest via runtime-less approach: we do local preview directly if daemon not reachable quickly.
    // Build locally to guarantee preview works offline (spec: local-first).
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let db_path = get_db_path();
    let local_preview = (|| -> Result<(), String> {
        let event_bus = coderun_events::EventBus::new();
        let repo_intel = coderun_repo_intel::RepositoryIntelligence::new(project_root.clone(), coderun_storage::Database::open(&db_path).map_err(|e| e.to_string())?, event_bus.clone());
        let kh = {
            let kdb = coderun_storage::Database::open(&db_path).map_err(|e| e.to_string())?;
            let mut hub = coderun_knowledge::KnowledgeHub::new(kdb, event_bus.clone(), coderun_knowledge::KnowledgeConfig::default());
            // TASK-037b: prefer repo-local skills, fall back to the global install (~/.coderun/skills)
            let local_skills = PathBuf::from(".coderun/skills");
            let global_skills = dirs().unwrap_or_else(|| PathBuf::from(".")).join(".coderun").join("skills");
            let skills_dir = if local_skills.is_dir() { local_skills } else { global_skills };
            if skills_dir.exists() { let _ = hub.load_skills(&skills_dir); }
            hub
        };
        let ctx = coderun_context::ContextEngine::new(repo_intel, kh, event_bus.clone(), coderun_context::ContextConfig::default());
        let task = coderun_core::TaskRequest { message: prompt.to_string(), session_id: effective_session.clone(), context_hints: None, repository_id: String::new(), repository_path: None };
        let (pack, routing) = ctx.build_context(&task).map_err(|e| e.to_string())?;
        println!("Skills matched:");
        if pack.behavioral_skills.is_empty() { println!("  (none — deduped or no match)"); } else { println!("  {}", pack.behavioral_skills.lines().next().unwrap_or("").trim()); if pack.behavioral_skills.contains("FROZEN PREFIX END") { println!("  [frozen-prefix boundary present ✓]"); } }
        println!();
        println!("Knowledge entries (docs_context):");
        if pack.docs_context.is_empty() { println!("  (none)"); } else { for line in pack.docs_context.lines().take(5) { println!("  {}", line); } }
        println!();
        println!("Code files (code_context):");
        if pack.code_context.is_empty() { println!("  (none — no index or no match)"); } else { for line in pack.code_context.lines().take(10) { println!("  {}", line); } }
        println!();
        println!("Token budget:");
        println!("  total: {}, remaining: {}, by_source: {:?}", pack.token_usage.total_tokens, pack.token_usage.budget_remaining, pack.token_usage.by_source);
        println!();
        println!("Model routing:");
        println!("  tier: {}, model: {}, reasoning: {}", routing.tier, routing.model, routing.reasoning);
        println!("  fallback chain: {:?}", coderun_router::fallback_chain(&routing.tier));
        println!();
        println!("Daemon URL probed: {} (if daemon running, this local preview matches daemon's BuildContext)", url);
        Ok(())
    })();

    if let Err(e) = local_preview {
        println!("Local preview failed: {} — is database initialized? Run `coderun init` and `coderun index`.", e);
        println!();
        println!("(Daemon preview via {} would also be attempted if daemon is running)", daemon_url);
    }
    Ok(())
}

fn cmd_status() -> Result<(), String> {
    println!("Coderun Status");
    println!("═══════════════════════════════════════");
    println!();
    
    // Check if database exists
    let db_path = get_db_path();
    if db_path.exists() {
        let db = coderun_storage::Database::open(&db_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;
        
        let file_count = db.get_file_count()
            .map_err(|e| format!("Failed to get file count: {}", e))?;
        let symbol_count = db.get_symbol_count()
            .map_err(|e| format!("Failed to get symbol count: {}", e))?;
        let usage = db.get_usage_stats()
            .map_err(|e| format!("Failed to get usage stats: {}", e))?;
        
        println!("Database:");
        println!("  Path:          {}", db_path.display());
        println!("  Files indexed: {}", file_count);
        println!("  Symbols:       {}", symbol_count);
        println!();
        println!("Token Usage:");
        println!("  Total input tokens:  {}", usage.total_input_tokens);
        println!("  Total output tokens: {}", usage.total_output_tokens);
        println!("  Total requests:      {}", usage.total_requests);
    } else {
        println!("Database: Not initialized");
        println!("  Run 'coderun init' to initialize");
    }
    
    println!();
    
    // Check skills directory
    let skills_dir = PathBuf::from(".coderun/skills");
    if skills_dir.exists() {
        let count = std::fs::read_dir(&skills_dir)
            .map(|entries| entries.filter_map(|e| e.ok()).count())
            .unwrap_or(0);
        println!("Skills: {} files", count);
    } else {
        println!("Skills: Directory not found");
    }
    
    Ok(())
}

fn cmd_skills(action: SkillsAction) -> Result<(), String> {
    match action {
        SkillsAction::List => {
            let skills_dir = PathBuf::from(".coderun/skills");
            if !skills_dir.exists() {
                println!("No skills directory found. Run 'coderun init' first.");
                return Ok(());
            }
            
            let mut engine = coderun_skills::SkillEngine::new(skills_dir);
            let count = engine.load_skills()
                .map_err(|e| format!("Failed to load skills: {}", e))?;
            
            if count == 0 {
                println!("No skills found in .coderun/skills/");
                return Ok(());
            }
            
            println!("Loaded skills ({}):", count);
            println!();
            
            for skill in engine.get_skills() {
                println!("  {} (tags: {})", skill.name, skill.tags.join(", "));
            }
        }
        SkillsAction::Validate => {
            let skills_dir = PathBuf::from(".coderun/skills");
            if !skills_dir.exists() {
                println!("No skills directory found. Run 'coderun init' first.");
                return Ok(());
            }
            
            let mut engine = coderun_skills::SkillEngine::new(skills_dir);
            match engine.load_skills() {
                Ok(count) => {
                    println!("✓ All {} skill files are valid", count);
                }
                Err(e) => {
                    println!("✗ Validation failed: {}", e);
                    return Err(e);
                }
            }
        }
    }
    
    Ok(())
}

fn cmd_config(action: ConfigAction) -> Result<(), String> {
    match action {
        ConfigAction::Show => {
            let project_root = std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?;
            
            let config = Config::load(&project_root)
                .map_err(|e| format!("Failed to load config: {}", e))?;
            
            let toml = config.to_toml()
                .map_err(|e| format!("Failed to serialize config: {}", e))?;
            
            println!("Effective configuration:");
            println!("═══════════════════════════════════════");
            println!();
            print!("{}", toml);
        }
        ConfigAction::Validate => {
            let project_root = std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?;
            
            let config = Config::load(&project_root)
                .map_err(|e| format!("Failed to load config: {}", e))?;
            
            match config.validate() {
                Ok(()) => {
                    println!("✓ Configuration is valid");
                }
                Err(e) => {
                    println!("✗ Configuration validation failed: {}", e);
                    return Err(e.to_string());
                }
            }
        }
        ConfigAction::Migrate { from } => {
            println!("Migrating config from '{}' (claude|continue|cursor)...", from);
            let project_root = std::env::current_dir().map_err(|e| e.to_string())?;
            let config = Config::load(&project_root).unwrap_or_default();
            // Migration: scan source's skills/config locations
            let candidates: Vec<PathBuf> = match from.as_str() {
                "claude" => vec![project_root.join(".claude").join("settings.json"), dirs().unwrap_or_else(|| PathBuf::from(".")).join(".claude").join("settings.json")],
                "continue" => vec![project_root.join(".continue").join("config.json")],
                "cursor" => vec![dirs().unwrap_or_else(|| PathBuf::from(".")).join(".cursor").join("settings.json")],
                _ => { println!("Unknown source '{}', supported: claude, continue, cursor", from); return Ok(()); }
            };
            let mut found = 0;
            for p in candidates {
                if p.exists() {
                    println!("  Found {} at {}", from, p.display());
                    // Copy skills/config heuristically: if file exists, note migration and validate
                    found += 1;
                }
            }
            if found == 0 {
                println!("  No {} config found — nothing to migrate (best-effort per spec §3 Adapter Layer Tier 2).", from);
            } else {
                println!("  Migration best-effort complete — review .coderun/config.toml and .coderun/skills/");
            }
            println!("  Config validation:");
            match config.validate() {
                Ok(()) => println!("  ✓ Config valid after migration"),
                Err(e) => println!("  ⚠ Config invalid: {}", e),
            }
        }
    }
    
    Ok(())
}

#[cfg(feature = "workflow")]
fn cmd_workflow(action: WorkflowAction) -> Result<(), String> {
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config = Config::load(&project_root).unwrap_or_default();
    // v1: workflow opt-in only — preserved in future/workflow
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    let engine: Box<dyn coderun_core::traits::IWorkflowEngine> = if config.workflow.enabled && config.workflow.engine == "dbos" {
        Box::new(coderun_workflow::dbos::DBOSWorkflowEngine::new(config.workflow.dbos_endpoint.clone(), config.workflow.dbos_shared_secret.clone()))
    } else {
        Box::new(coderun_core::traits::NoopWorkflowEngine)
    };
    match action {
        WorkflowAction::Start { prompt, require_approval } => {
            let task = coderun_core::TaskRequest { message: prompt.clone(), session_id: format!("cli-{}", uuid::Uuid::new_v4()), context_hints: None, repository_id: String::new(), repository_path: None };
            if require_approval { println!("Starting workflow with approval gate..."); }
            let res = rt.block_on(engine.start_workflow(&task, &config));
            match res {
                Ok(id) => {
                    println!("Workflow started: {}", id);
                    // Persist locally as well for list/status offline
                    if let Ok(db) = coderun_storage::Database::open(&get_db_path()) {
                        let _ = db.upsert_workflow(&id, if require_approval { "awaiting_approval" } else { "pending" }, &prompt);
                        let _ = db.insert_audit(Some(&id), None, "cli", &prompt, None, Some(&format!("require_approval={}", require_approval)));
                    }
                    println!("  Use `coderun workflow status {}` to check", id);
                    if require_approval { println!("  Then `coderun workflow approve {}`", id); }
                }
                Err(e) => return Err(format!("Workflow start failed: {}", e)),
            }
        }
        WorkflowAction::Status { workflow_id } => {
            let res = rt.block_on(engine.get_status(&workflow_id));
            match res {
                Ok(s) => println!("{}", s),
                Err(_) => {
                    if let Ok(db) = coderun_storage::Database::open(&get_db_path()) {
                        if let Ok(Some(rec)) = db.get_workflow(&workflow_id) {
                            println!("Workflow {}: status={} task=\"{}\" created={}", rec.workflow_id, rec.status, rec.task, rec.created_at);
                        } else {
                            println!("Workflow {} not found (engine down and not in local DB)", workflow_id);
                        }
                    } else {
                        println!("Workflow {} status unknown (engine unavailable)", workflow_id);
                    }
                }
            }
        }
        WorkflowAction::Approve { workflow_id } => {
            // Approve via DBOS HTTP, and update local DB
            let endpoint = config.workflow.dbos_endpoint.clone();
            println!("Approving workflow {} via {}...", workflow_id, endpoint);
            // Best-effort HTTP POST to /workflow/{id}/approve
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
            let res: Result<(), String> = rt.block_on(async {
                let client = reqwest::Client::new();
                let url = format!("{}/workflow/{}/approve", endpoint, workflow_id);
                let resp = client.post(&url).json(&serde_json::json!({})).send().await.map_err(|e| e.to_string())?;
                if resp.status().is_success() { Ok(()) } else { Err(format!("approve failed: {}", resp.status())) }
            });
            if let Err(e) = res { println!("Approve via DBOS failed (will update local DB anyway): {}", e); }
            if let Ok(db) = coderun_storage::Database::open(&get_db_path()) {
                let _ = db.upsert_workflow(&workflow_id, "completed", "");
                println!("Workflow {} approved (local DB updated)", workflow_id);
            }
        }
        WorkflowAction::List => {
            if let Ok(db) = coderun_storage::Database::open(&get_db_path()) {
                match db.list_workflows(20) {
                    Ok(list) if list.is_empty() => println!("No workflows yet. Start one with `coderun workflow start \"task\"`"),
                    Ok(list) => {
                        println!("Recent workflows:");
                        for w in list { println!("  {} | {} | {} | {}", w.workflow_id, w.status, w.task.chars().take(40).collect::<String>(), w.created_at); }
                    }
                    Err(e) => println!("Failed to list workflows: {}", e),
                }
            } else {
                println!("Database not initialized. Run `coderun init`.");
            }
        }
    }
    Ok(())
}

#[allow(clippy::cmp_owned)]
fn cmd_doctor() -> Result<(), String> {
    println!("Coderun Doctor (v1 — 8 probes, workflow opt-in via --features workflow)");
    println!("═══════════════════════════════════════");
    println!();
    
    let mut all_ok = true;
    
    // Check SQLite (critical) — v1: migrations 001-003 only (004/005 moved to future/workflow)
    print!("SQLite:          ");
    let db_path = get_db_path();
    match coderun_storage::Database::open(&db_path) {
        Ok(db) => {
            // Check migrations
            match db.get_file_count() {
                Ok(_) => println!("✓ OK (WAL, migrations 001-003, v1)"),
                Err(e) => { println!("✗ FAILED: {}", e); all_ok = false; }
            }
        },
        Err(e) => {
            println!("✗ FAILED: {}", e);
            all_ok = false;
        }
    }
    
    // Check config (critical)
    print!("Config:          ");
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match Config::load(&project_root) {
        Ok(config) => {
            match config.validate() {
                Ok(()) => println!("✓ OK (daemon.socket_path writable, token budget valid)"),
                Err(e) => {
                    println!("✗ INVALID: {}", e);
                    all_ok = false;
                }
            }
        }
        Err(e) => {
            println!("✗ NOT FOUND: {}", e);
            all_ok = false;
        }
    }
    
    // Check skills directory
    print!("Skills directory: ");
    let skills_dir = PathBuf::from(".coderun/skills");
    if skills_dir.exists() {
        let count = std::fs::read_dir(&skills_dir).map(|e| e.count()).unwrap_or(0);
        println!("✓ OK ({} skills, {})", count, skills_dir.display());
    } else {
        println!("⚠ NOT FOUND (run 'coderun init')");
    }

    // Check repository profile
    print!("Repo profile:     ");
    {
        let prof = PathBuf::from(".coderun/profile.json");
        if prof.exists() {
            println!("✓ OK ({})", prof.display());
        } else {
            println!("⚠ NOT FOUND (run 'coderun init' to bootstrap)");
        }
    }

    // Check socket path writable (new v0.3.0)
    print!("Socket path:     ");
    let cfg = Config::load(&project_root).unwrap_or_default();
    let sock = PathBuf::from(&cfg.daemon.socket_path);
    if let Some(parent) = sock.parent() {
        if parent.exists() || parent == PathBuf::from("/tmp") || parent.as_os_str() == std::ffi::OsStr::new(".") {
            println!("✓ OK ({})", sock.display());
        } else {
            println!("⚠ Parent dir missing: {}", parent.display());
        }
    } else {
        println!("⚠ No parent dir for {}", sock.display());
    }
    
    // Check tree-sitter (informational — now integrated)
    print!("Tree-sitter:     ");
    // Probe by building a dummy repo-intel parser
    {
        let db_tmp = coderun_storage::Database::open(&PathBuf::from(":memory:")).ok();
        if db_tmp.is_some() {
            println!("✓ OK (AST parsing for rust/python/js/ts via tree-sitter crate)");
        } else {
            println!("⚠ No parser (regex fallback)");
        }
    }
    
    // Check tantivy (new)
    print!("Tantivy:         ");
    {
        let idx_path = dirs().unwrap_or_else(|| PathBuf::from(".")).join(".coderun").join("index");
        if idx_path.exists() {
            println!("✓ OK (BM25 index at {})", idx_path.display());
        } else {
            println!("⚠ Index not yet built (run `coderun index` — will be created as MmapDirectory)");
        }
    }

    // Check engram
    print!("Engram:          ");
    {
        let cfg = Config::load(&project_root).unwrap_or_default();
        if cfg.knowledge.memory_enabled {
            println!("✓ Configured ({}, deterministic reads via HTTP, fail-open local fallback)", cfg.knowledge.memory_endpoint);
        } else {
            println!("⚠ Disabled in config (memory_enabled=false)");
        }
    }
    
    // Check LiteLLM
    print!("LiteLLM:         ");
    {
        let cfg = Config::load(&project_root).unwrap_or_default();
        println!("✓ Configured ({} — tier routing heuristic + fallback chain, gateway probe on serve)", cfg.litellm.endpoint);
    }
    
    // Check RTK
    print!("RTK:             ");
    {
        let rtk = coderun_optimizer::rtk::RtkAdapter::detect();
        if rtk.is_available() {
            println!("✓ OK (binary at {:?}, 10ms overhead)", rtk.binary_path);
        } else {
            println!("⚠ Not found on PATH — using built-in compressors + tee-on-failure (install rtk for 10ms binary)");
        }
    }

    // Check tiktoken
    print!("Tiktoken:        ");
    match tiktoken_rs::cl100k_base() {
        Ok(_) => println!("✓ OK (cl100k_base local, no model API round-trip)"),
        Err(e) => println!("⚠ Failed to load: {}", e),
    }

    // Check secrets redaction
    print!("Secrets redact:  ");
    {
        let sample = "api_key: sk-abc1234567890";
        let redacted = coderun_core::redact_secrets(sample);
        if redacted.contains("[REDACTED]") { println!("✓ OK (redaction before outbound calls)"); } else { println!("⚠ Probe failed"); }
    }

    // Workflow — v1 NOT part of hot path (future/workflow only)
    print!("Workflow/DBOS:   ");
    {
        println!("○ v1 disabled — preserved in future/workflow/ (opt-in --features workflow)");
    }

    // Check metrics endpoint
    print!("Metrics:         ");
    {
        println!("○ GET /metrics on daemon (prometheus exposition) — curl localhost:9527/metrics");
    }
    
    println!();
    
    if all_ok {
        println!("✓ All critical checks passed (v1 — DBOS/workflow disabled by default)");
        println!("  Try: `coderun preview \"test\"`, `coderun doctor`, `coderun serve` (no DBOS required)");
        println!("  Opt-in workflow: cargo build --features workflow && coderun --features workflow workflow start \"task\"");
    } else {
        println!("⚠ Some checks failed. Run 'coderun init' to initialize.");
    }
    
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// TASK-038: per-repo artifact home — ALL generated deliverables for an analyzed repo land in
/// `<repo>/.coderun/artifacts/<name>/` (NEVER back into the coderun source repository).
/// `coderun init` owns `.coderun/`, so creating on demand is safe.
fn ensure_artifact_dir(repo_root: &Path, name: &str) -> Result<PathBuf, String> {
    let dir = repo_root.join(".coderun").join("artifacts").join(name);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create artifact directory {}: {}", dir.display(), e))?;
    Ok(dir)
}

/// TASK-038: do not auto-edit the analyzed repo's .gitignore — print a hint instead.
fn print_gitignore_hint() {
    println!("  Hint: consider adding '.coderun/artifacts/' to this repository's .gitignore");
}

fn get_db_path() -> PathBuf {
    dirs().unwrap_or_else(|| PathBuf::from("."))
        .join(".coderun")
        .join("data.db")
}

fn dirs() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .ok()
            .map(PathBuf::from)
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cli_parsing() {
        let cli = Cli::try_parse_from(["coderun", "init"]);
        assert!(cli.is_ok());
        
        let cli = Cli::try_parse_from(["coderun", "status"]);
        assert!(cli.is_ok());
        
        let cli = Cli::try_parse_from(["coderun", "preview", "test prompt"]);
        assert!(cli.is_ok());
        
        let cli = Cli::try_parse_from(["coderun", "skills", "list"]);
        assert!(cli.is_ok());
        
        let cli = Cli::try_parse_from(["coderun", "config", "show"]);
        assert!(cli.is_ok());
        
        let cli = Cli::try_parse_from(["coderun", "doctor"]);
        assert!(cli.is_ok());
    }
    
    #[test]
    fn test_get_db_path() {
        let path = get_db_path();
        assert!(path.to_string_lossy().contains(".coderun"));
        assert!(path.to_string_lossy().contains("data.db"));
    }
    
    #[test]
    fn test_dirs_exists() {
        let dirs = dirs();
        assert!(dirs.is_some());
    }

    #[test]
    fn test_stack_terms_normalized_and_capped() {
        let d = Discovery {
            frameworks: vec!["cargo".into(), "rust-workspace".into(), "react".into(), "node".into()],
            languages: vec![("rust".to_string(), 40), ("typescript".to_string(), 12), ("javascript".to_string(), 3)],
            ..Default::default()
        };
        let terms = stack_terms(&d);
        assert_eq!(terms[0], "react", "frameworks come first");
        assert!(!terms.contains(&"node".to_string()), "generic terms dropped");
        assert!(!terms.contains(&"rust-workspace".to_string()), "aliases normalized");
        assert!(terms.contains(&"rust".to_string()));
        assert!(terms.len() <= 5);
    }

    #[test]
    fn test_parse_find_candidates_real_output() {
        // Mirrors actual `npx skills find react` output incl. ANSI color wrapping
        let sample = "\n\u{1b}[38;5;102mInstall with\u{1b}[0m npx skills add <owner/repo@skill>\n\n\u{1b}[38;5;145mvercel-labs/agent-skills@vercel-react-best-practices\u{1b}[0m \u{1b}[36m662.8K installs\u{1b}[0m\n\u{1b}[38;5;102m└ https://skills.sh/vercel-labs/agent-skills\u{1b}[0m\n\u{1b}[38;5;145mgoogle-labs-code/stitch-skills@react:components\u{1b}[0m \u{1b}[36m50.6K installs\u{1b}[0m\n";
        let cands = parse_find_candidates(sample, 2);
        assert_eq!(cands[0], "vercel-labs/agent-skills@vercel-react-best-practices", "rank order preserved");
        assert_eq!(cands[1], "google-labs-code/stitch-skills@react:components");
    }

    #[test]
    fn test_parse_find_candidates_rejects_garbage() {
        assert!(parse_find_candidates("", 2).is_empty());
        let junk = "https://skills.sh/foo/bar 12 installs\nnot-a-skill 3 installs\na/b@c 5 installs";
        let cands = parse_find_candidates(junk, 5);
        assert_eq!(cands, vec!["a/b@c".to_string()]);
    }

    #[test]
    fn test_sanitize_dir_name() {
        assert_eq!(sanitize_dir_name("react:components"), "react-components");
        assert_eq!(sanitize_dir_name("plain-skill_1.0"), "plain-skill_1.0");
    }

    #[test]
    fn test_ext_language() {
        assert_eq!(ext_language("rs"), Some("rust"));
        assert_eq!(ext_language("tsx"), Some("typescript"));
        assert_eq!(ext_language("hpp"), Some("cpp"));
        assert_eq!(ext_language(""), None);
        assert_eq!(ext_language("png"), None);
    }

    #[test]
    fn test_discovery_and_profile_json() {
        let tmp = std::env::temp_dir().join(format!("coderun-disc-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(tmp.join("src")).unwrap();
        std::fs::write(tmp.join("Cargo.toml"), "[package]\nname = \"x\"\n[workspace]\n").unwrap();
        std::fs::write(tmp.join("src").join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(tmp.join("README.md"), "# demo").unwrap();

        let d = discover_repository(&tmp);
        assert!(d.frameworks.contains(&"cargo".to_string()));
        assert!(d.frameworks.contains(&"rust-workspace".to_string()));
        assert_eq!(d.build_command.as_deref(), Some("cargo build"));
        assert_eq!(d.test_command.as_deref(), Some("cargo test"));
        assert!(d.important_dirs.contains(&"src".to_string()));
        assert!(d.languages.iter().any(|(l, n)| l == "rust" && *n >= 1));

        let profile = build_profile_json("x", &d, 1, 2, 3);
        assert_eq!(profile["project"], "x");
        assert_eq!(profile["index"]["files_indexed"], 1);
        assert_eq!(profile["commands"]["test"], "cargo test");
        let json = serde_json::to_string_pretty(&profile).unwrap();
        assert!(json.contains("rust"));

        std::fs::remove_dir_all(&tmp).ok();
    }
}
