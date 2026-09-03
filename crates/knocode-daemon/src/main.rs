#![allow(linker_messages)]
// knocode-daemon: daemon binary — HTTP server, signal handling, process lifecycle

use std::path::PathBuf;

use knocode_core::Config;
use knocode_daemon::lifecycle::DaemonState;

#[tokio::main]
async fn main() {
    // Load configuration
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config = match Config::load(&project_root) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Validate configuration
    if let Err(e) = config.validate() {
        eprintln!("Invalid configuration: {}", e);
        std::process::exit(1);
    }

    // Initialize daemon state (creates context engine, optimizer, etc.)
    let state = match DaemonState::initialize(config) {
        Ok(state) => state,
        Err(e) => {
            eprintln!("Failed to initialize daemon: {}", e);
            std::process::exit(1);
        }
    };

    // Start the daemon (HTTP server + background indexing + signal handling)
    if let Err(e) = state.serve().await {
        eprintln!("Daemon error: {}", e);
        std::process::exit(1);
    }
}
