//! Storage layer — SQLite persistence backbone for Coderun.
//!
//! This crate owns:
//! - **Database:** SQLite connection with WAL mode, migrations, and CRUD for files, symbols, knowledge, and sessions.
//! - **TantivyIndex:** Full-text BM25 search index (in-process Tantivy with MmapDirectory).
//!
//! SQLite stores all structured metadata (file hashes, AST symbols, knowledge entries, dependency edges,
//! token usage). Tantivy is the search index for fast full-text retrieval. Both are built from the
//! same source code walk during `coderun init` and kept in sync during incremental updates.

pub mod tantivy_index;

use std::path::Path;
use std::time::Instant;

use chrono::Utc;
use coderun_events::{EventBus, RuntimeEvent};
use rusqlite::{params, Connection};
use tracing::{debug, info, warn};

/// Database wrapper for SQLite operations
///
/// Note: Database is !Send and !Sync due to rusqlite::Connection.
/// This is fine for single-process daemon use. Each component that
/// needs database access opens its own connection.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create a database at the given path
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("Failed to open database: {}", e))?;

        // Enable WAL mode for concurrent reads
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| format!("Failed to set WAL mode: {}", e))?;

        let db = Self { conn };
        db.run_migrations()?;
        Ok(db)
    }

    /// Run all pending migrations
    fn run_migrations(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );",
            )
            .map_err(|e| format!("Failed to create migration table: {}", e))?;

        // Migration 001
        let migration_001 = include_str!("migrations/001_initial.sql");
        self.apply_migration("001_initial", migration_001)?;

        // Migration 002: Knowledge Hub
        let migration_002 = include_str!("migrations/002_knowledge.sql");
        self.apply_migration("002_knowledge", migration_002)?;

        // Migration 003: Dependency graph + cost
        let migration_003 = include_str!("migrations/003_graph.sql");
        self.apply_migration("003_graph", migration_003)?;

        // Migration 006: repository-scoped knowledge (TASK-030) + dedup cleanup (TASK-032)
        let migration_006 = include_str!("migrations/006_knowledge_repo.sql");
        self.apply_migration("006_knowledge_repo", migration_006)?;

        // v1: 004_events and 005_audits removed per TASK-002/TASK-001 — preserved in future/workflow/migrations/
        // Event persistence (ring buffer + SQLite) and workflow audits are NOT part of v1 hot path.
        // v1 keeps only tracing + metrics + correlation IDs (EventBus is in-memory only).

        Ok(())
    }

    fn apply_migration(&self, name: &str, sql: &str) -> Result<(), String> {
        let already_applied: bool = self
            .conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM schema_migrations WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to check migration status: {}", e))?;

        if already_applied {
            return Ok(());
        }

        self.conn
            .execute_batch(sql)
            .map_err(|e| format!("Migration '{}' failed: {}", name, e))?;

        self.conn
            .execute(
                "INSERT INTO schema_migrations (name) VALUES (?1)",
                params![name],
            )
            .map_err(|e| format!("Failed to record migration '{}': {}", name, e))?;

        info!(migration = name, "Applied migration");
        Ok(())
    }

    // ── Files ───────────────────────────────────────────────────────

    /// Insert a new file record
    pub fn insert_file(
        &self,
        path: &str,
        hash: &str,
        size: i64,
        language: Option<&str>,
    ) -> Result<i64, String> {
        let start = Instant::now();
        self.conn
            .execute(
                "INSERT INTO files (path, hash, size, language, last_indexed_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![path, hash, size, language, Utc::now().to_rfc3339()],
            )
            .map_err(|e| format!("Failed to insert file: {}", e))?;
        let id = self.conn.last_insert_rowid();
        log_slow("insert_file", start);
        Ok(id)
    }

    /// Update an existing file's hash and size
    pub fn update_file(&self, id: i64, hash: &str, size: i64) -> Result<(), String> {
        let start = Instant::now();
        self.conn
            .execute(
                "UPDATE files SET hash = ?1, size = ?2, last_indexed_at = ?3 WHERE id = ?4",
                params![hash, size, Utc::now().to_rfc3339(), id],
            )
            .map_err(|e| format!("Failed to update file: {}", e))?;
        log_slow("update_file", start);
        Ok(())
    }

    /// Delete a file by path — cascades to symbols (TASK-010: stale symbols must disappear)
    pub fn delete_file(&self, path: &str) -> Result<(), String> {
        let start = Instant::now();
        // Delete symbols first to avoid FOREIGN KEY constraint (symbols.file_id → files.id)
        let _ = self.conn.execute(
            "DELETE FROM symbols WHERE file_id IN (SELECT id FROM files WHERE path = ?1)",
            params![path],
        );
        self.conn
            .execute("DELETE FROM files WHERE path = ?1", params![path])
            .map_err(|e| format!("Failed to delete file: {}", e))?;
        log_slow("delete_file", start);
        Ok(())
    }

    /// Get all files as (path, hash) pairs
    pub fn get_all_files(&self) -> Result<Vec<(String, String)>, String> {
        let start = Instant::now();
        let mut stmt = self
            .conn
            .prepare("SELECT path, hash FROM files")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let files = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Failed to query files: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect files: {}", e))?;

        log_slow("get_all_files", start);
        Ok(files)
    }

    /// Get all files with full meta (for incremental mtime+size shortcut — Phase2)
    pub fn get_all_files_meta(&self) -> Result<Vec<FileRecord>, String> {
        let start = Instant::now();
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, hash, size, language, last_indexed_at FROM files")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;
        let files = stmt
            .query_map([], |row| {
                Ok(FileRecord {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    hash: row.get(2)?,
                    size: row.get(3)?,
                    language: row.get(4)?,
                    last_indexed_at: row.get(5)?,
                })
            })
            .map_err(|e| format!("Failed to query files: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect files: {}", e))?;
        log_slow("get_all_files_meta", start);
        Ok(files)
    }

    /// Get a file by path
    pub fn get_file(&self, path: &str) -> Result<Option<FileRecord>, String> {
        let start = Instant::now();
        let result = self.conn.query_row(
            "SELECT id, path, hash, size, language, last_indexed_at FROM files WHERE path = ?1",
            params![path],
            |row| {
                Ok(FileRecord {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    hash: row.get(2)?,
                    size: row.get(3)?,
                    language: row.get(4)?,
                    last_indexed_at: row.get(5)?,
                })
            },
        );

        log_slow("get_file", start);
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Failed to get file: {}", e)),
        }
    }

    /// Get a file record by ID
    pub fn get_file_by_id(&self, id: i64) -> Result<Option<FileRecord>, String> {
        let start = Instant::now();
        let result = self.conn.query_row(
            "SELECT id, path, hash, size, language, last_indexed_at FROM files WHERE id = ?1",
            params![id],
            |row| {
                Ok(FileRecord {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    hash: row.get(2)?,
                    size: row.get(3)?,
                    language: row.get(4)?,
                    last_indexed_at: row.get(5)?,
                })
            },
        );

        log_slow("get_file_by_id", start);
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Failed to get file by id: {}", e)),
        }
    }

    /// Get the count of indexed files
    pub fn get_file_count(&self) -> Result<usize, String> {
        self.conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get::<_, i64>(0))
            .map(|v| v as usize)
            .map_err(|e| format!("Failed to count files: {}", e))
    }

    // ── Symbols ─────────────────────────────────────────────────────

    /// Insert a symbol
    pub fn insert_symbol(
        &self,
        file_id: i64,
        name: &str,
        kind: &str,
        line_start: i64,
        line_end: i64,
        parent_id: Option<i64>,
    ) -> Result<i64, String> {
        let start = Instant::now();
        self.conn
            .execute(
                "INSERT INTO symbols (file_id, name, kind, line_start, line_end, parent_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![file_id, name, kind, line_start, line_end, parent_id],
            )
            .map_err(|e| format!("Failed to insert symbol: {}", e))?;
        let id = self.conn.last_insert_rowid();
        log_slow("insert_symbol", start);
        Ok(id)
    }

    /// Get all symbols for a file
    pub fn get_symbols_for_file(&self, file_id: i64) -> Result<Vec<Symbol>, String> {
        let start = Instant::now();
        let mut stmt = self
            .conn
            .prepare("SELECT id, file_id, name, kind, line_start, line_end, parent_id FROM symbols WHERE file_id = ?1")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let symbols = stmt
            .query_map(params![file_id], |row| {
                Ok(Symbol {
                    id: row.get(0)?,
                    file_id: row.get(1)?,
                    name: row.get(2)?,
                    kind: row.get(3)?,
                    line_start: row.get(4)?,
                    line_end: row.get(5)?,
                    parent_id: row.get(6)?,
                })
            })
            .map_err(|e| format!("Failed to query symbols: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect symbols: {}", e))?;

        log_slow("get_symbols_for_file", start);
        Ok(symbols)
    }

    /// Find symbols by name (partial match)
    pub fn find_symbol(&self, name: &str) -> Result<Vec<Symbol>, String> {
        let start = Instant::now();
        let pattern = format!("%{}%", name);
        let mut stmt = self
            .conn
            .prepare("SELECT id, file_id, name, kind, line_start, line_end, parent_id FROM symbols WHERE name LIKE ?1")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let symbols = stmt
            .query_map(params![pattern], |row| {
                Ok(Symbol {
                    id: row.get(0)?,
                    file_id: row.get(1)?,
                    name: row.get(2)?,
                    kind: row.get(3)?,
                    line_start: row.get(4)?,
                    line_end: row.get(5)?,
                    parent_id: row.get(6)?,
                })
            })
            .map_err(|e| format!("Failed to query symbols: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect symbols: {}", e))?;

        log_slow("find_symbol", start);
        Ok(symbols)
    }

    /// Get the count of symbols
    pub fn get_symbol_count(&self) -> Result<usize, String> {
        self.conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get::<_, i64>(0))
            .map(|v| v as usize)
            .map_err(|e| format!("Failed to count symbols: {}", e))
    }

    // ── Token Usage ─────────────────────────────────────────────────

    /// Record token usage
    pub fn insert_usage(
        &self,
        correlation_id: &str,
        request_type: &str,
        input_tokens: i64,
        output_tokens: i64,
        model: &str,
        tier: &str,
    ) -> Result<(), String> {
        let start = Instant::now();
        self.conn
            .execute(
                "INSERT INTO token_usage (correlation_id, request_type, input_tokens, output_tokens, model, tier) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![correlation_id, request_type, input_tokens, output_tokens, model, tier],
            )
            .map_err(|e| format!("Failed to insert usage: {}", e))?;
        log_slow("insert_usage", start);
        Ok(())
    }

    /// Get aggregated usage statistics
    pub fn get_usage_stats(&self) -> Result<UsageStats, String> {
        let start = Instant::now();
        let stats = self.conn.query_row(
            "SELECT COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0), COUNT(*) FROM token_usage",
            [],
            |row| {
                Ok(UsageStats {
                    total_input_tokens: row.get(0)?,
                    total_output_tokens: row.get(1)?,
                    total_requests: row.get(2)?,
                })
            },
        ).map_err(|e| format!("Failed to get usage stats: {}", e))?;
        log_slow("get_usage_stats", start);
        Ok(stats)
    }

    // ── Knowledge Operations ───────────────────────────────────────

    /// Store a knowledge entry — idempotent upsert on (category, key, repository_id) (TASK-032)
    pub fn store_knowledge(&self, category: &str, key: &str, value: &str, confidence: f64, source: &str, repository_id: &str) -> Result<i64, String> {
        let start = Instant::now();
        self.conn
            .execute(
                "INSERT INTO knowledge (category, key, value, confidence, source, repository_id, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(category, key, repository_id) DO UPDATE SET
                   value = excluded.value, confidence = excluded.confidence, source = excluded.source, updated_at = excluded.updated_at",
                params![category, key, value, confidence, source, repository_id, Utc::now().to_rfc3339()],
            )
            .map_err(|e| format!("Failed to store knowledge: {}", e))?;
        let id = self.conn.last_insert_rowid();
        log_slow("store_knowledge", start);
        Ok(id)
    }

    /// Get a knowledge entry by category and key
    pub fn get_knowledge(&self, category: &str, key: &str) -> Result<Option<KnowledgeRecord>, String> {
        let start = Instant::now();
        let result = self.conn.query_row(
            "SELECT id, category, key, value, confidence, source, created_at, updated_at, repository_id FROM knowledge WHERE category = ?1 AND key = ?2 ORDER BY confidence DESC LIMIT 1",
            params![category, key],
            |row| {
                Ok(KnowledgeRecord {
                    id: row.get(0)?,
                    category: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    confidence: row.get(4)?,
                    source: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    repository_id: row.get(8)?,
                })
            },
        );
        log_slow("get_knowledge", start);
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Failed to get knowledge: {}", e)),
        }
    }

    /// Get all knowledge entries
    pub fn get_all_knowledge(&self) -> Result<Vec<KnowledgeRecord>, String> {
        let start = Instant::now();
        let mut stmt = self
            .conn
            .prepare("SELECT id, category, key, value, confidence, source, created_at, updated_at, repository_id FROM knowledge ORDER BY confidence DESC")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let records = stmt
            .query_map([], |row| {
                Ok(KnowledgeRecord {
                    id: row.get(0)?,
                    category: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    confidence: row.get(4)?,
                    source: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    repository_id: row.get(8)?,
                })
            })
            .map_err(|e| format!("Failed to query knowledge: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect knowledge: {}", e))?;

        log_slow("get_all_knowledge", start);
        Ok(records)
    }

    /// Get knowledge entries by category
    pub fn get_knowledge_by_category(&self, category: &str) -> Result<Vec<KnowledgeRecord>, String> {
        let start = Instant::now();
        let mut stmt = self
            .conn
            .prepare("SELECT id, category, key, value, confidence, source, created_at, updated_at, repository_id FROM knowledge WHERE category = ?1 ORDER BY confidence DESC")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let records = stmt
            .query_map(params![category], |row| {
                Ok(KnowledgeRecord {
                    id: row.get(0)?,
                    category: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    confidence: row.get(4)?,
                    source: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    repository_id: row.get(8)?,
                })
            })
            .map_err(|e| format!("Failed to query knowledge: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect knowledge: {}", e))?;

        log_slow("get_knowledge_by_category", start);
        Ok(records)
    }

    /// Update confidence for a knowledge entry
    pub fn update_knowledge_confidence(&self, id: i64, confidence: f64) -> Result<(), String> {
        let start = Instant::now();
        self.conn
            .execute(
                "UPDATE knowledge SET confidence = ?1, updated_at = ?2 WHERE id = ?3",
                params![confidence, Utc::now().to_rfc3339(), id],
            )
            .map_err(|e| format!("Failed to update confidence: {}", e))?;
        log_slow("update_knowledge_confidence", start);
        Ok(())
    }

    /// Decay confidence for knowledge entries older than min_age_days
    pub fn decay_knowledge_confidence(&self, min_age_days: i64, decay_amount: f64) -> Result<usize, String> {
        let start = Instant::now();
        let cutoff = (Utc::now() - chrono::Duration::days(min_age_days)).to_rfc3339();
        let affected = self.conn
            .execute(
                "UPDATE knowledge SET confidence = MAX(0.1, confidence - ?1), updated_at = ?2 WHERE updated_at < ?3",
                params![decay_amount, Utc::now().to_rfc3339(), cutoff],
            )
            .map_err(|e| format!("Failed to decay confidence: {}", e))?;
        log_slow("decay_knowledge_confidence", start);
        Ok(affected)
    }

    /// Search knowledge by text (LIKE-based) — optionally scoped to a repository (TASK-030).
    /// `repository_filter: Some(id)` matches ONLY rows stamped with that id (strict — legacy ''
    /// rows never leak across repos). `None` returns everything.
    /// Count total knowledge entries (for hub initialization checks — P0 #3)
    pub fn count_knowledge(&self, repository_filter: Option<&str>) -> Result<usize, String> {
        let sql = if repository_filter.is_some() {
            "SELECT COUNT(*) FROM knowledge WHERE repository_id = ?1"
        } else {
            "SELECT COUNT(*) FROM knowledge"
        };
        let count: i64 = if let Some(repo) = repository_filter {
            self.conn.query_row(sql, params![repo], |row| row.get(0))
        } else {
            self.conn.query_row(sql, params![], |row| row.get(0))
        }
        .map_err(|e| format!("Failed to count knowledge: {}", e))?;
        Ok(count as usize)
    }

    pub fn search_knowledge(&self, query: &str, category_filter: Option<&str>, min_confidence: f64, max_results: usize, repository_filter: Option<&str>) -> Result<Vec<KnowledgeRecord>, String> {
        let start = Instant::now();
        let pattern = format!("%{}%", query);

        let mut records = Vec::new();

        // Build parameterized SQL manually to keep the existing filter shapes
        if let Some(cat) = category_filter {
            let sql_with_repo = "SELECT id, category, key, value, confidence, source, created_at, updated_at, repository_id FROM knowledge WHERE (key LIKE ?1 OR value LIKE ?1) AND category = ?2 AND confidence >= ?3 AND repository_id = ?4 ORDER BY confidence DESC LIMIT ?5";
            let sql_plain   = "SELECT id, category, key, value, confidence, source, created_at, updated_at, repository_id FROM knowledge WHERE (key LIKE ?1 OR value LIKE ?1) AND category = ?2 AND confidence >= ?3 ORDER BY confidence DESC LIMIT ?4";
            let mut stmt = self.conn.prepare(if repository_filter.is_some() { sql_with_repo } else { sql_plain })
                .map_err(|e| format!("Failed to prepare query: {}", e))?;
            let map_row = |row: &rusqlite::Row| -> Result<KnowledgeRecord, rusqlite::Error> {
                Ok(KnowledgeRecord {
                    id: row.get(0)?,
                    category: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    confidence: row.get(4)?,
                    source: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    repository_id: row.get(8)?,
                })
            };
            let rows = if let Some(repo) = repository_filter {
                stmt.query_map(params![pattern, cat, min_confidence, repo, max_results as i64], map_row)
            } else {
                stmt.query_map(params![pattern, cat, min_confidence, max_results as i64], map_row)
            }.map_err(|e| format!("Failed to query knowledge: {}", e))?;
            for row in rows {
                records.push(row.map_err(|e| format!("Failed to read row: {}", e))?);
            }
        } else {
            let sql_with_repo = "SELECT id, category, key, value, confidence, source, created_at, updated_at, repository_id FROM knowledge WHERE (key LIKE ?1 OR value LIKE ?1) AND confidence >= ?2 AND repository_id = ?3 ORDER BY confidence DESC LIMIT ?4";
            let sql_plain   = "SELECT id, category, key, value, confidence, source, created_at, updated_at, repository_id FROM knowledge WHERE (key LIKE ?1 OR value LIKE ?1) AND confidence >= ?2 ORDER BY confidence DESC LIMIT ?3";
            let mut stmt = self.conn.prepare(if repository_filter.is_some() { sql_with_repo } else { sql_plain })
                .map_err(|e| format!("Failed to prepare query: {}", e))?;
            let map_row = |row: &rusqlite::Row| -> Result<KnowledgeRecord, rusqlite::Error> {
                Ok(KnowledgeRecord {
                    id: row.get(0)?,
                    category: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    confidence: row.get(4)?,
                    source: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    repository_id: row.get(8)?,
                })
            };
            let rows = if let Some(repo) = repository_filter {
                stmt.query_map(params![pattern, min_confidence, repo, max_results as i64], map_row)
            } else {
                stmt.query_map(params![pattern, min_confidence, max_results as i64], map_row)
            }.map_err(|e| format!("Failed to query knowledge: {}", e))?;
            for row in rows {
                records.push(row.map_err(|e| format!("Failed to read row: {}", e))?);
            }
        }

        log_slow("search_knowledge", start);
        Ok(records)
    }

    /// Delete a knowledge entry
    pub fn delete_knowledge(&self, category: &str, key: &str) -> Result<(), String> {
        let start = Instant::now();
        self.conn
            .execute(
                "DELETE FROM knowledge WHERE category = ?1 AND key = ?2",
                params![category, key],
            )
            .map_err(|e| format!("Failed to delete knowledge: {}", e))?;
        log_slow("delete_knowledge", start);
        Ok(())
    }

    // ── Memory Operations ──────────────────────────────────────────

    /// Save a memory entry
    pub fn save_memory(&self, namespace: &str, key: &str, value: &str) -> Result<i64, String> {
        let start = Instant::now();
        self.conn
            .execute(
                "INSERT OR REPLACE INTO memory (namespace, key, value) VALUES (?1, ?2, ?3)",
                params![namespace, key, value],
            )
            .map_err(|e| format!("Failed to save memory: {}", e))?;
        let id = self.conn.last_insert_rowid();
        log_slow("save_memory", start);
        Ok(id)
    }

    /// Get a memory entry
    pub fn get_memory(&self, namespace: &str, key: &str) -> Result<Option<MemoryRecord>, String> {
        let start = Instant::now();
        let result = self.conn.query_row(
            "SELECT id, namespace, key, value, created_at FROM memory WHERE namespace = ?1 AND key = ?2",
            params![namespace, key],
            |row| {
                Ok(MemoryRecord {
                    id: row.get(0)?,
                    namespace: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        );
        log_slow("get_memory", start);
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Failed to get memory: {}", e)),
        }
    }

    /// Search memory by text
    pub fn search_memory(&self, namespace: &str, query: &str, max_results: usize) -> Result<Vec<MemoryRecord>, String> {
        let start = Instant::now();
        let pattern = format!("%{}%", query);
        let mut stmt = self
            .conn
            .prepare("SELECT id, namespace, key, value, created_at FROM memory WHERE namespace = ?1 AND (key LIKE ?2 OR value LIKE ?2) LIMIT ?3")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let records = stmt
            .query_map(params![namespace, pattern, max_results as i64], |row| {
                Ok(MemoryRecord {
                    id: row.get(0)?,
                    namespace: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| format!("Failed to query memory: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect memory: {}", e))?;

        log_slow("search_memory", start);
        Ok(records)
    }

    /// Delete a memory entry
    pub fn delete_memory(&self, namespace: &str, key: &str) -> Result<(), String> {
        let start = Instant::now();
        self.conn
            .execute(
                "DELETE FROM memory WHERE namespace = ?1 AND key = ?2",
                params![namespace, key],
            )
            .map_err(|e| format!("Failed to delete memory: {}", e))?;
        log_slow("delete_memory", start);
        Ok(())
    }

    // ── Audits & Workflows — future/workflow only (TASK-001, not v1) ─────────────────────
    #[cfg(feature = "workflow")]
    pub fn insert_audit(&self, workflow_id: Option<&str>, correlation_id: Option<&str>, actor: &str, task: &str, ctx_pack_hash: Option<&str>, payload: Option<&str>) -> Result<i64, String> {
        self.conn.execute(
            "INSERT INTO audits (workflow_id, correlation_id, actor, task, ctx_pack_hash, payload) VALUES (?1,?2,?3,?4,?5,?6)",
            params![workflow_id, correlation_id, actor, task, ctx_pack_hash, payload],
        ).map_err(|e| format!("Failed to insert audit: {e}"))?;
        Ok(self.conn.last_insert_rowid())
    }

    #[cfg(feature = "workflow")]
    pub fn list_audits(&self, limit: usize) -> Result<Vec<AuditRecord>, String> {
        let mut stmt = self.conn.prepare("SELECT id, workflow_id, correlation_id, actor, task, ctx_pack_hash, payload, created_at FROM audits ORDER BY id DESC LIMIT ?1").map_err(|e| format!("Failed to prepare: {e}"))?;
        let rows = stmt.query_map(params![limit as i64], |row| Ok(AuditRecord {
            id: row.get(0)?, workflow_id: row.get(1)?, correlation_id: row.get(2)?, actor: row.get(3)?, task: row.get(4)?, ctx_pack_hash: row.get(5)?, payload: row.get(6)?, created_at: row.get(7)?,
        })).map_err(|e| format!("Failed to query audits: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("{e}"))
    }

    #[cfg(feature = "workflow")]
    pub fn upsert_workflow(&self, workflow_id: &str, status: &str, task: &str) -> Result<(), String> {
        self.conn.execute(
            "INSERT INTO workflows (workflow_id, status, task, updated_at) VALUES (?1,?2,?3,?4) ON CONFLICT(workflow_id) DO UPDATE SET status=excluded.status, updated_at=excluded.updated_at",
            params![workflow_id, status, task, Utc::now().to_rfc3339()],
        ).map_err(|e| format!("Failed to upsert workflow: {e}"))?;
        Ok(())
    }

    #[cfg(feature = "workflow")]
    pub fn get_workflow(&self, workflow_id: &str) -> Result<Option<WorkflowRecord>, String> {
        let res = self.conn.query_row("SELECT workflow_id, status, task, created_at, updated_at FROM workflows WHERE workflow_id=?1", params![workflow_id], |row| Ok(WorkflowRecord {
            workflow_id: row.get(0)?, status: row.get(1)?, task: row.get(2)?, created_at: row.get(3)?, updated_at: row.get(4)?,
        }));
        match res { Ok(r) => Ok(Some(r)), Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None), Err(e) => Err(format!("{e}")) }
    }

    #[cfg(feature = "workflow")]
    pub fn list_workflows(&self, limit: usize) -> Result<Vec<WorkflowRecord>, String> {
        let mut stmt = self.conn.prepare("SELECT workflow_id, status, task, created_at, updated_at FROM workflows ORDER BY created_at DESC LIMIT ?1").map_err(|e| format!("{e}"))?;
        let rows = stmt.query_map(params![limit as i64], |row| Ok(WorkflowRecord {
            workflow_id: row.get(0)?, status: row.get(1)?, task: row.get(2)?, created_at: row.get(3)?, updated_at: row.get(4)?,
        })).map_err(|e| format!("{e}"))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("{e}"))
    }

    // ── Utility ─────────────────────────────────────────────────────

    /// Begin batched transaction for bulk indexing (Phase1 perf — avoids fsync per row)
    pub fn begin_batch(&self) -> Result<(), String> {
        self.conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| format!("Failed to begin batch: {}", e))
    }

    /// Commit batched transaction
    pub fn commit_batch(&self) -> Result<(), String> {
        self.conn
            .execute_batch("COMMIT")
            .map_err(|e| format!("Failed to commit batch: {}", e))
    }

    /// Rollback batched transaction (best-effort)
    pub fn rollback_batch(&self) -> Result<(), String> {
        let _ = self.conn.execute_batch("ROLLBACK");
        Ok(())
    }

    /// Emit indexing progress event via the event bus
    pub fn emit_indexing_progress(&self, event_bus: &EventBus, files_indexed: usize, symbols_extracted: usize, duration_ms: u64) {
        event_bus.emit(RuntimeEvent::RepositoryUpdated {
            files_indexed,
            symbols_extracted,
            duration_ms,
        });
    }
}

// ── Data Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub id: i64,
    pub path: String,
    pub hash: String,
    pub size: i64,
    pub language: Option<String>,
    pub last_indexed_at: String,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub id: i64,
    pub file_id: i64,
    pub name: String,
    pub kind: String,
    pub line_start: i64,
    pub line_end: i64,
    pub parent_id: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct UsageStats {
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_requests: i64,
}

#[derive(Debug, Clone)]
pub struct KnowledgeRecord {
    pub id: i64,
    pub category: String,
    pub key: String,
    pub value: String,
    pub confidence: f64,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
    /// Owning repository scope (TASK-030) — '' marks legacy/global rows
    pub repository_id: String,
}

#[derive(Debug, Clone)]
pub struct MemoryRecord {
    pub id: i64,
    pub namespace: String,
    pub key: String,
    pub value: String,
    pub created_at: String,
}

#[cfg(feature = "workflow")]
#[derive(Debug, Clone)]
pub struct AuditRecord {
    pub id: i64,
    pub workflow_id: Option<String>,
    pub correlation_id: Option<String>,
    pub actor: String,
    pub task: String,
    pub ctx_pack_hash: Option<String>,
    pub payload: Option<String>,
    pub created_at: String,
}

#[cfg(feature = "workflow")]
#[derive(Debug, Clone)]
pub struct WorkflowRecord {
    pub workflow_id: String,
    pub status: String,
    pub task: String,
    pub created_at: String,
    pub updated_at: String,
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Log a warning if a database operation took more than 100ms
fn log_slow(operation: &str, start: Instant) {
    let elapsed = start.elapsed();
    if elapsed.as_millis() > 100 {
        warn!(
            operation = operation,
            duration_ms = elapsed.as_millis() as u64,
            "Slow database query"
        );
    } else {
        debug!(
            operation = operation,
            duration_ms = elapsed.as_millis() as u64,
            "Database query"
        );
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_db() -> Database {
        let path = PathBuf::from(":memory:");
        Database::open(&path).expect("Failed to create in-memory database")
    }

    #[test]
    fn test_database_creation() {
        let db = test_db();
        let count = db.get_file_count().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_insert_and_get_file() {
        let db = test_db();
        let id = db
            .insert_file("src/main.rs", "abc123", 1024, Some("rust"))
            .unwrap();
        assert!(id > 0);

        let file = db.get_file("src/main.rs").unwrap().unwrap();
        assert_eq!(file.path, "src/main.rs");
        assert_eq!(file.hash, "abc123");
        assert_eq!(file.size, 1024);
        assert_eq!(file.language.as_deref(), Some("rust"));
    }

    #[test]
    fn test_get_nonexistent_file() {
        let db = test_db();
        let file = db.get_file("nonexistent.rs").unwrap();
        assert!(file.is_none());
    }

    #[test]
    fn test_update_file() {
        let db = test_db();
        let id = db
            .insert_file("src/main.rs", "old_hash", 100, None)
            .unwrap();
        db.update_file(id, "new_hash", 200).unwrap();

        let file = db.get_file("src/main.rs").unwrap().unwrap();
        assert_eq!(file.hash, "new_hash");
        assert_eq!(file.size, 200);
    }

    #[test]
    fn test_delete_file() {
        let db = test_db();
        db.insert_file("src/main.rs", "abc", 100, None)
            .unwrap();
        db.delete_file("src/main.rs").unwrap();
        let file = db.get_file("src/main.rs").unwrap();
        assert!(file.is_none());
    }

    #[test]
    fn test_get_all_files() {
        let db = test_db();
        db.insert_file("a.rs", "h1", 10, None).unwrap();
        db.insert_file("b.rs", "h2", 20, None).unwrap();
        let files = db.get_all_files().unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_file_count() {
        let db = test_db();
        assert_eq!(db.get_file_count().unwrap(), 0);
        db.insert_file("a.rs", "h", 10, None).unwrap();
        assert_eq!(db.get_file_count().unwrap(), 1);
        db.insert_file("b.rs", "h", 10, None).unwrap();
        assert_eq!(db.get_file_count().unwrap(), 2);
    }

    #[test]
    fn test_insert_and_find_symbols() {
        let db = test_db();
        let file_id = db
            .insert_file("src/main.rs", "h", 100, None)
            .unwrap();

        let sym_id = db
            .insert_symbol(file_id, "main", "function", 1, 10, None)
            .unwrap();
        assert!(sym_id > 0);

        let symbols = db.get_symbols_for_file(file_id).unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "main");
        assert_eq!(symbols[0].kind, "function");
    }

    #[test]
    fn test_find_symbol() {
        let db = test_db();
        let file_id = db
            .insert_file("a.rs", "h", 100, None)
            .unwrap();
        db.insert_symbol(file_id, "UserService", "struct", 1, 20, None)
            .unwrap();
        db.insert_symbol(file_id, "handle_request", "function", 25, 50, None)
            .unwrap();

        let results = db.find_symbol("User").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "UserService");
    }

    #[test]
    fn test_symbol_count() {
        let db = test_db();
        let file_id = db
            .insert_file("a.rs", "h", 100, None)
            .unwrap();
        assert_eq!(db.get_symbol_count().unwrap(), 0);
        db.insert_symbol(file_id, "a", "function", 1, 5, None)
            .unwrap();
        db.insert_symbol(file_id, "b", "function", 6, 10, None)
            .unwrap();
        assert_eq!(db.get_symbol_count().unwrap(), 2);
    }

    #[test]
    fn test_token_usage() {
        let db = test_db();
        db.insert_usage("req_123", "pre_generation", 1000, 500, "gpt-4o", "balanced")
            .unwrap();
        db.insert_usage("req_456", "pre_tool", 2000, 300, "gpt-4o", "balanced")
            .unwrap();

        let stats = db.get_usage_stats().unwrap();
        assert_eq!(stats.total_input_tokens, 3000);
        assert_eq!(stats.total_output_tokens, 800);
        assert_eq!(stats.total_requests, 2);
    }

    #[test]
    fn test_migration_idempotency() {
        let dir = std::env::temp_dir().join(format!("coderun_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        {
            let db1 = Database::open(&path).unwrap();
            db1.insert_file("a.rs", "h", 10, None).unwrap();
        }

        // Open again — migrations should not fail
        let db2 = Database::open(&path).unwrap();
        let count = db2.get_file_count().unwrap();
        assert_eq!(count, 1);

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    // v1: 004/005 migrations removed — tests moved to future/workflow
    // Workflow audits/workflows are NOT part of v1 hot path (TASK-001/002)

    #[test]
    fn test_knowledge_repo_scoping_and_upsert() {
        // TASK-030/032: repository-scoped search, idempotent upsert, duplicate collapse
        let db = test_db();
        db.store_knowledge("docs", "guide.md", "v1 eshop checkout", 0.8, "mkdocs", "repo_a").unwrap();
        // Re-store same key → upsert, not growth (F-3)
        db.store_knowledge("docs", "guide.md", "v2 eshop checkout", 0.9, "mkdocs", "repo_a").unwrap();
        // Same key in another repo → separate row (cross-repo isolation)
        db.store_knowledge("docs", "guide.md", "other repo checkout content zzz", 0.7, "mkdocs", "repo_b").unwrap();

        let all = db.get_all_knowledge().unwrap();
        assert_eq!(all.len(), 2, "upsert must not grow the table");
        let a = db.get_knowledge("docs", "guide.md").unwrap().unwrap();
        assert!((a.confidence - 0.9).abs() < 1e-9, "highest-confidence write wins");

        let hits_a = db.search_knowledge("checkout", None, 0.0, 10, Some("repo_a")).unwrap();
        assert_eq!(hits_a.len(), 1);
        assert_eq!(hits_a[0].repository_id, "repo_a");
        let hits_b = db.search_knowledge("checkout", None, 0.0, 10, Some("repo_b")).unwrap();
        assert_eq!(hits_b.len(), 1);
        assert!(hits_b[0].value.contains("other repo"), "no cross-repo leakage (F-1)");
    }

    #[test]
    fn test_migration_006_collapses_legacy_duplicates() {
        // Simulate a legacy DB (pre-006): create schema without repository_id via manual SQL,
        // insert duplicates, then open through Database::open to trigger migration.
        let dir = std::env::temp_dir().join(format!("coderun_m006_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy.db");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE knowledge (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    category TEXT NOT NULL,
                    key TEXT NOT NULL,
                    value TEXT NOT NULL,
                    confidence REAL NOT NULL DEFAULT 0.5,
                    source TEXT NOT NULL DEFAULT 'manual',
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );",
            )
            .unwrap();
            for _ in 0..4 {
                conn.execute(
                    "INSERT INTO knowledge (category, key, value, confidence, source) VALUES ('docs', 'dup.md', 'dup content', 0.6, 'mkdocs')",
                    [],
                )
                .unwrap();
            }
        }
        let db = Database::open(&path).unwrap();
        let rows = db.search_knowledge("dup content", None, 0.0, 100, None).unwrap();
        assert_eq!(rows.len(), 1, "migration must collapse duplicate rows keeping max confidence");
        assert_eq!(rows[0].repository_id, "");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
