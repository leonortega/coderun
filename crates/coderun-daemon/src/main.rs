// coderun-daemon: daemon binary — UDS server, signal handling, process lifecycle

mod adapter;
mod lifecycle;

use std::path::PathBuf;

use coderun_core::Config;
use lifecycle::DaemonState;

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

    // Initialize daemon state
    let state = match DaemonState::initialize(config) {
        Ok(state) => state,
        Err(e) => {
            eprintln!("Failed to initialize daemon: {}", e);
            std::process::exit(1);
        }
    };

    // Start the daemon
    if let Err(e) = state.serve().await {
        eprintln!("Daemon error: {}", e);
        std::process::exit(1);
    }
}
