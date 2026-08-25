#![allow(linker_messages)]
// coderun-daemon: daemon binary — HTTP server, signal handling, process lifecycle

#[allow(dead_code)]
mod adapter;
mod http_server;
mod lifecycle;
mod metrics;
mod ratelimit;

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
