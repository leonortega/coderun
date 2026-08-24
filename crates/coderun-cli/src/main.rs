use std::path::PathBuf;

use clap::{Parser, Subcommand};
use coderun_core::Config;

// ── CLI Arguments ───────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "coderun")]
#[command(about = "AI Runtime for coding agents")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon server
    Serve,
    
    /// Initialize runtime for current repository
    Init,
    
    /// Trigger repository re-indexing
    Index,
    
    /// Preview what BuildContext would produce for a prompt
    Preview {
        /// The prompt to preview
        prompt: String,
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
}

// ── Main ────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    
    let result = match cli.command {
        Commands::Serve => cmd_serve(),
        Commands::Init => cmd_init(),
        Commands::Index => cmd_index(),
        Commands::Preview { prompt } => cmd_preview(&prompt),
        Commands::Status => cmd_status(),
        Commands::Skills { action } => cmd_skills(action),
        Commands::Config { action } => cmd_config(action),
        Commands::Doctor => cmd_doctor(),
    };
    
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

// ── Command Implementations ─────────────────────────────────────────────

fn cmd_serve() -> Result<(), String> {
    // Delegate to daemon binary
    println!("Starting coderun daemon...");
    println!("Use 'coderun-daemon' binary directly, or this will be integrated in Phase 12.");
    Ok(())
}

fn cmd_init() -> Result<(), String> {
    println!("Initializing coderun for current repository...");
    
    // Create .coderun directory
    let coderun_dir = PathBuf::from(".coderun");
    std::fs::create_dir_all(&coderun_dir)
        .map_err(|e| format!("Failed to create .coderun directory: {}", e))?;
    println!("  Created .coderun/");
    
    // Create default config if not exists
    let config_path = coderun_dir.join("config.toml");
    if !config_path.exists() {
        let default_config = Config::default();
        let config_toml = toml::to_string_pretty(&default_config)
            .map_err(|e| format!("Failed to serialize default config: {}", e))?;
        std::fs::write(&config_path, config_toml)
            .map_err(|e| format!("Failed to write config: {}", e))?;
        println!("  Created .coderun/config.toml");
    } else {
        println!("  Config already exists, skipping");
    }
    
    // Create skills directory
    let skills_dir = coderun_dir.join("skills");
    std::fs::create_dir_all(&skills_dir)
        .map_err(|e| format!("Failed to create skills directory: {}", e))?;
    println!("  Created .coderun/skills/");
    
    // Initialize database
    let db_path = dirs().unwrap_or_else(|| PathBuf::from(".")).join(".coderun").join("data.db");
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create database directory: {}", e))?;
    }
    let _db = coderun_storage::Database::open(&db_path)
        .map_err(|e| format!("Failed to initialize database: {}", e))?;
    println!("  Initialized database at {}", db_path.display());
    
    println!();
    println!("✓ Initialization complete!");
    println!();
    println!("Next steps:");
    println!("  1. Run 'coderun index' to index your repository");
    println!("  2. Run 'coderun serve' to start the daemon");
    println!("  3. Configure your coding agent to use coderun");
    
    Ok(())
}

fn cmd_index() -> Result<(), String> {
    println!("Indexing repository...");
    
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
        project_root,
        db,
        event_bus,
    );
    
    // Run indexing
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
    
    Ok(())
}

fn cmd_preview(prompt: &str) -> Result<(), String> {
    println!("Previewing context for: {}", prompt);
    println!();
    
    // This would normally connect to the daemon via UDS/TCP
    // For now, show what would be included
    println!("Skills that would match:");
    println!("  (Connect to daemon to see actual matches)");
    println!();
    println!("Knowledge entries:");
    println!("  (Connect to daemon to see actual entries)");
    println!();
    println!("Code files:");
    println!("  (Connect to daemon to see actual files)");
    println!();
    println!("Model routing:");
    println!("  (Connect to daemon to see routing decision)");
    
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
    }
    
    Ok(())
}

fn cmd_doctor() -> Result<(), String> {
    println!("Coderun Doctor");
    println!("═══════════════════════════════════════");
    println!();
    
    let mut all_ok = true;
    
    // Check SQLite
    print!("SQLite:          ");
    let db_path = get_db_path();
    match coderun_storage::Database::open(&db_path) {
        Ok(_) => println!("✓ OK"),
        Err(e) => {
            println!("✗ FAILED: {}", e);
            all_ok = false;
        }
    }
    
    // Check config
    print!("Config:          ");
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match Config::load(&project_root) {
        Ok(config) => {
            match config.validate() {
                Ok(()) => println!("✓ OK"),
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
        println!("✓ OK ({})", skills_dir.display());
    } else {
        println!("⚠ NOT FOUND (run 'coderun init')");
    }
    
    // Check tree-sitter (informational)
    print!("Tree-sitter:     ");
    println!("⚠ Not integrated (using regex-based extraction)");
    
    // Check engram (informational)
    print!("Engram:          ");
    println!("⚠ Not integrated (using local SQLite memory)");
    
    // Check LiteLLM (informational)
    print!("LiteLLM:         ");
    println!("⚠ Not integrated (routing configured but no connection)");
    
    // Check RTK (informational)
    print!("RTK:             ");
    println!("⚠ Not integrated (using built-in compression)");
    
    println!();
    
    if all_ok {
        println!("✓ All critical checks passed");
    } else {
        println!("⚠ Some checks failed. Run 'coderun init' to initialize.");
    }
    
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────

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
}
