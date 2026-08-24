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

        // Migration 004: Event persistence
        let migration_004 = include_str!("migrations/004_events.sql");
        self.apply_migration("004_events", migration_004)?;

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

    /// Delete a file by path
    pub fn delete_file(&self, path: &str) -> Result<(), String> {
        let start = Instant::now();
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

    /// Get the count of indexed files
    pub fn get_file_count(&self) -> Result<usize, String> {
        self.conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
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
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
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

    /// Store a knowledge entry
    pub fn store_knowledge(&self, category: &str, key: &str, value: &str, confidence: f64, source: &str) -> Result<i64, String> {
        let start = Instant::now();
        self.conn
            .execute(
                "INSERT OR REPLACE INTO knowledge (category, key, value, confidence, source, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![category, key, value, confidence, source, Utc::now().to_rfc3339()],
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
            "SELECT id, category, key, value, confidence, source, created_at, updated_at FROM knowledge WHERE category = ?1 AND key = ?2",
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
            .prepare("SELECT id, category, key, value, confidence, source, created_at, updated_at FROM knowledge ORDER BY confidence DESC")
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
            .prepare("SELECT id, category, key, value, confidence, source, created_at, updated_at FROM knowledge WHERE category = ?1 ORDER BY confidence DESC")
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

    /// Search knowledge by text (simple LIKE-based search)
    pub fn search_knowledge(&self, query: &str, category_filter: Option<&str>, min_confidence: f64, max_results: usize) -> Result<Vec<KnowledgeRecord>, String> {
        let start = Instant::now();
        let pattern = format!("%{}%", query);
        
        let mut records = Vec::new();
        
        if let Some(cat) = category_filter {
            let mut stmt = self.conn.prepare(
                "SELECT id, category, key, value, confidence, source, created_at, updated_at FROM knowledge WHERE (key LIKE ?1 OR value LIKE ?1) AND category = ?2 AND confidence >= ?3 ORDER BY confidence DESC LIMIT ?4"
            ).map_err(|e| format!("Failed to prepare query: {}", e))?;
            let rows = stmt.query_map(params![pattern, cat, min_confidence, max_results], |row| {
                Ok(KnowledgeRecord {
                    id: row.get(0)?,
                    category: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    confidence: row.get(4)?,
                    source: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            }).map_err(|e| format!("Failed to query knowledge: {}", e))?;
            for row in rows {
                records.push(row.map_err(|e| format!("Failed to read row: {}", e))?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, category, key, value, confidence, source, created_at, updated_at FROM knowledge WHERE (key LIKE ?1 OR value LIKE ?1) AND confidence >= ?2 ORDER BY confidence DESC LIMIT ?3"
            ).map_err(|e| format!("Failed to prepare query: {}", e))?;
            let rows = stmt.query_map(params![pattern, min_confidence, max_results], |row| {
                Ok(KnowledgeRecord {
                    id: row.get(0)?,
                    category: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    confidence: row.get(4)?,
                    source: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            }).map_err(|e| format!("Failed to query knowledge: {}", e))?;
            for row in rows {
                records.push(row.map_err(|e| format!("Failed to read row: {}", e))?);
            }
        };

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
            .query_map(params![namespace, pattern, max_results], |row| {
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

    // ── Utility ─────────────────────────────────────────────────────

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
}

#[derive(Debug, Clone)]
pub struct MemoryRecord {
    pub id: i64,
    pub namespace: String,
    pub key: String,
    pub value: String,
    pub created_at: String,
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
}
