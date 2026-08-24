use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::{debug, info};

/// Git-change-triggered incremental watcher (spec §2 principle: like language server indexing)
/// On filesystem change or git HEAD move, triggers `RepositoryIntelligence::index_repository` incrementally.
/// v0.3.0: polling + mtime check; future: `notify` crate + `git2` diff.
pub struct RepoWatcher {
    repo_path: PathBuf,
    running: Arc<AtomicBool>,
    poll_interval: Duration,
}

impl RepoWatcher {
    pub fn new(repo_path: PathBuf) -> Self {
        Self { repo_path, running: Arc::new(AtomicBool::new(false)), poll_interval: Duration::from_secs(5) }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Spawn background watcher task that calls `on_change` whenever the repo appears dirty.
    /// `on_change` should be `RepoIntelligence::index_repository` incrementally.
    pub fn spawn<F>(&self, on_change: F) -> tokio::task::JoinHandle<()>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let repo_path = self.repo_path.clone();
        let running = self.running.clone();
        let interval = self.poll_interval;
        running.store(true, Ordering::Relaxed);
        tokio::spawn(async move {
            let mut last_head = read_head(&repo_path);
            let mut last_mtime = dir_mtime(&repo_path);
            info!(path = %repo_path.display(), "Repo watcher started (poll {:?})", interval);
            while running.load(Ordering::Relaxed) {
                tokio::time::sleep(interval).await;
                let cur_head = read_head(&repo_path);
                let cur_mtime = dir_mtime(&repo_path);
                if cur_head != last_head || cur_mtime != last_mtime {
                    debug!(old_head = ?last_head, new_head = ?cur_head, "Repo change detected");
                    last_head = cur_head;
                    last_mtime = cur_mtime;
                    on_change();
                }
            }
        })
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

fn read_head(repo_path: &Path) -> Option<String> {
    let head_path = repo_path.join(".git").join("HEAD");
    std::fs::read_to_string(head_path).ok().map(|s| s.trim().to_string())
}

fn dir_mtime(repo_path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(repo_path).ok().and_then(|m| m.modified().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_watcher_new() {
        let w = RepoWatcher::new(PathBuf::from("."));
        assert!(!w.is_running());
    }

    #[tokio::test]
    async fn test_watcher_spawn_and_stop() {
        let dir = std::env::temp_dir().join(format!("coderun_watcher_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let w = RepoWatcher::new(dir.clone()).with_interval(Duration::from_millis(50));
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();
        let handle = w.spawn(move || { flag_clone.store(true, Ordering::Relaxed); });
        assert!(w.is_running());
        // Trigger by touching HEAD
        let head = dir.join(".git").join("HEAD");
        std::fs::create_dir_all(head.parent().unwrap()).unwrap();
        std::fs::write(&head, "ref: refs/heads/main").unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        w.stop();
        assert!(!w.is_running());
        handle.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
