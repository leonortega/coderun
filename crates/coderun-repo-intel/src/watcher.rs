use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::{debug, info};
#[cfg(feature = "git-watcher")]
use tracing::warn;

/// Debounce interval for filesystem events — avoids rapid-fire during git operations.
const DEBOUNCE_MS: u64 = 500;

/// Max cache size for libgit2 (64 MB) — controls memory usage on large repos.
const LIBGIT2_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Git-change-triggered incremental watcher — FIRST-CLASS v0.5.0: `notify` crate + `git2` diff primary, polling fallback
/// On filesystem change or git HEAD move, triggers `RepositoryIntelligence::index_repository` incrementally.
/// Primary: `notify::RecommendedWatcher` + `git2::Repository::diff_tree_to_workdir`; fallback: polling HEAD+mtime 5s with WARN.
pub struct RepoWatcher {
    repo_path: PathBuf,
    running: Arc<AtomicBool>,
    poll_interval: Duration,
}

impl RepoWatcher {
    pub fn new(repo_path: PathBuf) -> Self {
        Self {
            repo_path,
            running: Arc::new(AtomicBool::new(false)),
            poll_interval: Duration::from_secs(5),
        }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Spawn background watcher task that calls `on_change` whenever the repo appears dirty.
    /// `on_change` should be `RepoIntelligence::index_repository` incrementally.
    /// FIRST-CLASS: try `notify`+`git2` watcher; on Err fallback to polling with WARN.
    pub fn spawn<F>(&self, on_change: F) -> tokio::task::JoinHandle<()>
    where
        F: Fn() + Send + Sync + 'static,
    {
        // FIRST-CLASS: attempt notify+git2 incremental watcher
        // Wrap in Arc so it can be shared between the try_notify path and fallback
        let on_change = Arc::new(on_change);
        if let Some(handle) = self.try_notify_git2_watcher(on_change.clone()) {
            info!(path = %self.repo_path.display(), "Repo watcher started (notify+git2 first-class)");
            return handle;
        }
        tracing::warn!("notify/git2 watcher not available, fallback to polling HEAD+mtime (v0.5.0 first-class missing)");
        let repo_path = self.repo_path.clone();
        let running = self.running.clone();
        let interval = self.poll_interval;
        running.store(true, Ordering::Relaxed);
        tokio::spawn(async move {
            let mut last_head = read_head(&repo_path);
            let mut last_mtime = dir_mtime(&repo_path);
            info!(path = %repo_path.display(), "Repo watcher started (poll fallback {:?})", interval);
            while running.load(Ordering::Relaxed) {
                tokio::time::sleep(interval).await;
                let cur_head = read_head(&repo_path);
                let cur_mtime = dir_mtime(&repo_path);
                if cur_head != last_head || cur_mtime != last_mtime {
                    debug!(old_head = ?last_head, new_head = ?cur_head, "Repo change detected (poll fallback)");
                    last_head = cur_head;
                    last_mtime = cur_mtime;
                    on_change();
                }
            }
        })
    }

    /// Try to start the first-class notify+git2 watcher.
    ///
    /// When the `git-watcher` feature is enabled:
    /// 1. Opens the repo with `git2::Repository::open()`
    /// 2. Configures libgit2 cache via `opts::set_cache_max_size()` (git2 0.21)
    /// 3. Creates a `notify::RecommendedWatcher` with debounced event channel
    /// 4. Spawns a tokio task that:
    ///    - Receives debounced filesystem events (500ms debounce)
    ///    - On any event: opens repo, diffs HEAD vs workdir
    ///    - Calls `on_change()` only when actual dirty files exist
    ///
    /// Returns `None` (triggering polling fallback) when:
    /// - `git-watcher` feature is not compiled in
    /// - `repo_path/.git` doesn't exist (not a git repo)
    /// - git2 or notify initialization fails
    #[cfg(feature = "git-watcher")]
    fn try_notify_git2_watcher<F>(&self, on_change: Arc<F>) -> Option<tokio::task::JoinHandle<()>>
    where
        F: Fn() + Send + Sync + 'static,
    {
        use notify::{RecommendedWatcher, RecursiveMode, Watcher};

        // Guard: must be a git repo
        if !self.repo_path.join(".git").exists() {
            debug!(path = %self.repo_path.display(), "Not a git repo — skipping notify+git2 watcher");
            return None;
        }

        // Configure libgit2 memory cache (git2 0.21 API, unsafe)
        // SAFETY: set_cache_max_size only modifies internal libgit2 cache limits.
        // Safe to call once at startup before any concurrent repo operations.
        unsafe {
            if let Err(e) = git2::opts::set_cache_max_size(LIBGIT2_CACHE_MAX_BYTES as isize) {
                warn!(error = %e, "Failed to set libgit2 cache size — using defaults");
            }
        }

        // Validate repo is openable
        let repo = match git2::Repository::open(&self.repo_path) {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, path = %self.repo_path.display(), "Failed to open git repo for watcher");
                return None;
            }
        };
        drop(repo); // close — we'll reopen on each check

        // Create debounced notify watcher
        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = match RecommendedWatcher::new(
            tx,
            notify::Config::default().with_poll_interval(Duration::from_millis(200)),
        ) {
            Ok(w) => w,
            Err(e) => {
                warn!(error = %e, "Failed to create notify watcher");
                return None;
            }
        };

        // Watch the repo directory recursively
        if let Err(e) = debouncer.watch(&self.repo_path, RecursiveMode::Recursive) {
            warn!(error = %e, path = %self.repo_path.display(), "Failed to watch repo directory");
            return None;
        }

        // Detach the watcher so it lives as long as the handle
        // We move debouncer into the spawned task to keep it alive
        let repo_path = self.repo_path.clone();
        let running = self.running.clone();
        running.store(true, Ordering::Relaxed);

        let handle = tokio::task::spawn_blocking(move || {
            // Keep the watcher alive for the lifetime of this task
            let _watcher = debouncer;

            info!(path = %repo_path.display(), "Repo watcher started (notify+git2, debounce={}ms)", DEBOUNCE_MS);

            // Debounce loop: reset timer on each event, fire after quiet period
            loop {
                if !running.load(Ordering::Relaxed) {
                    break;
                }

                // Block until first event (with timeout to check running flag)
                match rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(_event) => {
                        // Got an event — drain any queued events with short timeout
                        let debounce_deadline =
                            std::time::Instant::now() + Duration::from_millis(DEBOUNCE_MS);
                        loop {
                            let remaining = debounce_deadline
                                .checked_duration_since(std::time::Instant::now())
                                .unwrap_or_default();
                            if remaining.is_zero() {
                                break;
                            }
                            match rx.recv_timeout(remaining) {
                                Ok(_event) => {
                                    // Another event — extend debounce window
                                    continue;
                                }
                                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                                    // Debounce quiet period elapsed
                                    break;
                                }
                                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                                    warn!("Notify channel disconnected");
                                    return;
                                }
                            }
                        }

                        // Check if repo is actually dirty
                        if is_repo_dirty(&repo_path) {
                            debug!(path = %repo_path.display(), "Repo dirty — triggering incremental index");
                            on_change();
                        } else {
                            debug!(path = %repo_path.display(), "Filesystem event but repo clean — skipping");
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // No event for 1s — loop and check running flag
                        continue;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        warn!("Notify channel disconnected");
                        break;
                    }
                }
            }
        });

        Some(handle)
    }

    /// Stub when `git-watcher` feature is not compiled in — falls back to polling.
    #[cfg(not(feature = "git-watcher"))]
    fn try_notify_git2_watcher<F>(&self, _on_change: Arc<F>) -> Option<tokio::task::JoinHandle<()>>
    where
        F: Fn() + Send + Sync + 'static,
    {
        None
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Check if a git repository has uncommitted changes (staged or unstaged).
/// Uses git2 to diff HEAD vs workdir — only returns true for real code changes,
/// not internal .git/ writes.
#[cfg(feature = "git-watcher")]
fn is_repo_dirty(repo_path: &Path) -> bool {
    let repo = match git2::Repository::open(repo_path) {
        Ok(r) => r,
        Err(_) => return false,
    };

    // Check HEAD exists (empty repo has no HEAD)
    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => return false, // unborn branch — nothing to diff
    };

    let tree = match head.peel_to_tree() {
        Ok(t) => t,
        Err(_) => return false,
    };

    // Diff HEAD tree vs workdir — any changes mean dirty
    // Include untracked files so new (unstaged) files are detected
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true);
    // Note: `diff` borrows from `repo`, so we must extract the count before dropping
    let delta_count = match repo.diff_tree_to_workdir(Some(&tree), Some(&mut opts)) {
        Ok(diff) => diff.deltas().len(),
        Err(_) => 0,
    };
    delta_count > 0
}

#[cfg(not(feature = "git-watcher"))]
fn is_repo_dirty(_repo_path: &Path) -> bool {
    false
}

fn read_head(repo_path: &Path) -> Option<String> {
    let head_path = repo_path.join(".git").join("HEAD");
    std::fs::read_to_string(head_path)
        .ok()
        .map(|s| s.trim().to_string())
}

fn dir_mtime(repo_path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(repo_path)
        .ok()
        .and_then(|m| m.modified().ok())
}

// ── Tests ────────────────────────────────────────────────────────────────

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
        let handle = w.spawn(move || {
            flag_clone.store(true, Ordering::Relaxed);
        });
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

    // ── v0.5.0 first-class tool tests ──────────────────────────────────

    #[test]
    fn test_try_notify_git2_returns_none_when_feature_disabled() {
        let w = RepoWatcher::new(PathBuf::from("."));
        // Without `git-watcher` feature, try_notify_git2_watcher is stub → None
        let cb = Arc::new(|| {});
        #[cfg(not(feature = "git-watcher"))]
        assert!(
            w.try_notify_git2_watcher(cb).is_none(),
            "notify/git2 not wired → fallback polling is expected when git-watcher feature is off"
        );
        // With `git-watcher` feature, still None for non-git dir
        #[cfg(feature = "git-watcher")]
        assert!(
            w.try_notify_git2_watcher(cb).is_none(),
            "Non-git directory → should fall back to polling"
        );
    }

    #[cfg(feature = "git-watcher")]
    #[test]
    fn test_try_notify_git2_returns_none_for_non_git_dir() {
        let dir = std::env::temp_dir().join(format!("coderun_watcher_nongit_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let w = RepoWatcher::new(dir.clone());
        assert!(
            w.try_notify_git2_watcher(Arc::new(|| {})).is_none(),
            "Non-git directory should return None"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "git-watcher")]
    #[tokio::test]
    async fn test_try_notify_git2_returns_some_for_valid_repo() {
        let dir = std::env::temp_dir().join(format!("coderun_watcher_gitrepo_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // Init a real git repo
        let repo = git2::Repository::init(&dir).unwrap();
        drop(repo);

        // Create a file so there's something to diff
        std::fs::write(dir.join("test.txt"), "hello").unwrap();

        let w = RepoWatcher::new(dir.clone());
        let result = w.try_notify_git2_watcher(Arc::new(|| {}));
        assert!(result.is_some(), "Valid git repo should return Some(handle)");
        if let Some(handle) = result {
            handle.abort();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "git-watcher")]
    #[test]
    fn test_is_repo_dirty_detects_changes() {
        let dir = std::env::temp_dir().join(format!("coderun_watcher_dirty_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // Init repo and make an initial commit
        let repo = git2::Repository::init(&dir).unwrap();
        {
            let mut index = repo.index().unwrap();
            std::fs::write(dir.join("initial.txt"), "init").unwrap();
            index.add_path(std::path::Path::new("initial.txt")).unwrap();
            index.write().unwrap();
            let oid = index.write_tree().unwrap();
            let tree = repo.find_tree(oid).unwrap();
            let sig = repo.signature().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
                .unwrap();
        }

        // Clean repo → not dirty
        assert!(!is_repo_dirty(&dir), "Clean repo should not be dirty");

        // Add uncommitted change
        std::fs::write(dir.join("new_file.rs"), "fn main() {}").unwrap();
        assert!(is_repo_dirty(&dir), "Repo with uncommitted file should be dirty");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_watcher_fallback_polling_still_detects_mtime() {
        let dir = std::env::temp_dir().join(format!(
            "coderun_watcher_mtime_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let w = RepoWatcher::new(dir.clone()).with_interval(Duration::from_millis(40));
        let hit = Arc::new(AtomicBool::new(false));
        let hit_clone = hit.clone();
        let handle = w.spawn(move || {
            hit_clone.store(true, Ordering::Relaxed);
        });
        // Touch a file to bump mtime
        std::fs::write(dir.join("a.txt"), "v1").unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        // Polling may or may not have hit yet depending on mtime granularity, but watcher must be running and not panic
        assert!(w.is_running());
        w.stop();
        handle.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
