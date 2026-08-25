use std::path::Path;

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, QueryParser, Query, TermQuery};
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy};
use tracing::info;

// ── Schema Definition ────────────────────────────────────────────────────

/// Schema for code search index
pub struct CodeIndexSchema {
    pub schema: Schema,
    pub path_field: Field,
    pub content_field: Field,
    pub language_field: Field,
    pub symbols_field: Field,
    /// Repository scope field (TASK-030) — every doc is stamped so retrieval can be repo-scoped
    pub repository_field: Field,
}

impl Default for CodeIndexSchema {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeIndexSchema {
    pub fn new() -> Self {
        let mut schema_builder = Schema::builder();

        let path_field = schema_builder.add_text_field("path", STRING | STORED);
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let language_field = schema_builder.add_text_field("language", STRING | STORED);
        let symbols_field = schema_builder.add_text_field("symbols", TEXT | STORED);
        let repository_field = schema_builder.add_text_field("repository_id", STRING | STORED);

        Self {
            schema: schema_builder.build(),
            path_field,
            content_field,
            language_field,
            symbols_field,
            repository_field,
        }
    }
}

// ── Tantivy Index ────────────────────────────────────────────────────────

/// Tantivy-based full-text search index
pub struct TantivyIndex {
    index: Index,
    schema: CodeIndexSchema,
    index_path: String,
}

impl TantivyIndex {
    /// Create or open a tantivy index
    pub fn open(index_path: &str) -> Result<Self, String> {
        let schema = CodeIndexSchema::new();

        let path = Path::new(index_path);
        if !path.exists() {
            std::fs::create_dir_all(path)
                .map_err(|e| format!("Failed to create index directory: {}", e))?;
        }

        let index = Index::open_or_create(
            tantivy::directory::MmapDirectory::open(path)
                .map_err(|e| format!("Failed to open index directory: {}", e))?,
            schema.schema.clone(),
        )
        .map_err(|e| format!("Failed to open tantivy index: {}", e))?;

        info!(path = index_path, "Tantivy index opened");

        Ok(Self {
            index,
            schema,
            index_path: index_path.to_string(),
        })
    }

    /// Get a writer for indexing documents
    pub fn writer(&self) -> Result<IndexWriter, String> {
        self.index
            .writer(50_000_000) // 50MB heap size
            .map_err(|e| format!("Failed to create index writer: {}", e))
    }

    /// Get a reader for searching
    pub fn reader(&self) -> Result<IndexReader, String> {
        self.index
            .reader_builder()
                .reload_policy(ReloadPolicy::OnCommitWithDelay)
                .try_into()
                .map_err(|e| format!("Failed to create index reader: {}", e))
    }

    /// Add a document to the index, stamped with its repository scope (TASK-030)
    pub fn add_document(
        &self,
        writer: &mut IndexWriter,
        path: &str,
        content: &str,
        language: &str,
        symbols: &[String],
        repository_id: &str,
    ) -> Result<(), String> {
        let symbols_text = symbols.join(" ");

        let doc = doc!(
            self.schema.path_field => path,
            self.schema.content_field => content,
            self.schema.language_field => language,
            self.schema.symbols_field => symbols_text.as_str(),
            self.schema.repository_field => repository_id,
        );

        writer
            .add_document(doc)
            .map_err(|e| format!("Failed to add document: {}", e))?;

        Ok(())
    }

    /// Delete a document from the index — scoped to a repository so two repos
    /// sharing a relative path never delete each other's docs (TASK-030)
    pub fn delete_document(
        &self,
        writer: &mut IndexWriter,
        path: &str,
        repository_id: &str,
    ) -> Result<(), String> {
        let repo_term = tantivy::Term::from_field_text(self.schema.repository_field, repository_id);
        let path_term = tantivy::Term::from_field_text(self.schema.path_field, path);
        let both = BooleanQuery::new(vec![
            (tantivy::query::Occur::Must, Box::new(TermQuery::new(repo_term, IndexRecordOption::Basic)) as Box<dyn Query>),
            (tantivy::query::Occur::Must, Box::new(TermQuery::new(path_term, IndexRecordOption::Basic)) as Box<dyn Query>),
        ]);
        let _ = writer
            .delete_query(Box::new(both))
            .map_err(|e| format!("Failed to delete document: {}", e))?;

        Ok(())
    }

    /// Commit changes to the index
    pub fn commit(&self, writer: &mut IndexWriter) -> Result<(), String> {
        writer
            .commit()
            .map_err(|e| format!("Failed to commit index: {}", e))?;

        Ok(())
    }

    /// Search the index — optionally scoped to a single repository (TASK-030).
    /// `repository_id: Some(id)` filters hits to docs stamped with that id; `None` searches all.
    pub fn search(
        &self,
        reader: &IndexReader,
        query: &str,
        _language_filter: Option<&str>,
        max_results: usize,
        repository_id: Option<&str>,
    ) -> Result<Vec<SearchHit>, String> {
        let searcher = reader.searcher();

        // Build query
        let query_parser = QueryParser::for_index(
            &self.index,
            vec![
                self.schema.content_field,
                self.schema.symbols_field,
                self.schema.path_field,
            ],
        );

        let user_query = query_parser
            .parse_query(query)
            .map_err(|e| format!("Failed to parse query: {}", e))?;

        // Repository scope filter (TermQuery AND user query) — TASK-030/F-1
        let full_query: Box<dyn Query> = match repository_id {
            Some(repo) => {
                let repo_term =
                    tantivy::Term::from_field_text(self.schema.repository_field, repo);
                Box::new(BooleanQuery::new(vec![
                    (
                        tantivy::query::Occur::Must,
                        Box::new(TermQuery::new(repo_term, IndexRecordOption::Basic)) as Box<dyn Query>,
                    ),
                    (tantivy::query::Occur::Must, user_query),
                ]))
            }
            None => Box::new(user_query),
        };

        let top_docs = searcher
            .search(&full_query, &TopDocs::with_limit(max_results))
            .map_err(|e| format!("Search failed: {}", e))?;

        let mut results = Vec::new();

        for (score, doc_addr) in top_docs {
            if let Ok(doc) = searcher.doc::<tantivy::TantivyDocument>(doc_addr) {
                let path = doc
                    .get_first(self.schema.path_field)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let content = doc
                    .get_first(self.schema.content_field)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let language = doc
                    .get_first(self.schema.language_field)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let symbols = doc
                    .get_first(self.schema.symbols_field)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                results.push(SearchHit {
                    path,
                    content,
                    language,
                    symbols,
                    score,
                });
            }
        }

        Ok(results)
    }

    /// Get index statistics
    pub fn stats(&self, reader: &IndexReader) -> Result<IndexStats, String> {
        let searcher = reader.searcher();
        let doc_count = searcher.num_docs() as usize;

        Ok(IndexStats {
            doc_count,
            index_path: self.index_path.clone(),
        })
    }
}

// ── Data Types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub path: String,
    pub content: String,
    pub language: String,
    pub symbols: String,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct IndexStats {
    pub doc_count: usize,
    pub index_path: String,
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_index() {
        let temp_dir = TempDir::new().unwrap();
        let index_path = temp_dir.path().to_str().unwrap();

        let index = TantivyIndex::open(index_path);
        assert!(index.is_ok());
    }

    #[test]
    fn test_add_and_search() {
        let temp_dir = TempDir::new().unwrap();
        let index_path = temp_dir.path().to_str().unwrap();

        let index = TantivyIndex::open(index_path).unwrap();
        let mut writer = index.writer().unwrap();

        // Add a document
        index
            .add_document(
                &mut writer,
                "src/main.rs",
                "fn main() { println!(\"Hello\"); }",
                "rust",
                &["main".to_string()],
                "repo_a",
            )
            .unwrap();

        index.commit(&mut writer).unwrap();

        // Search — scoped to the right repo finds it (TASK-030)
        let reader = index.reader().unwrap();
        let results = index.search(&reader, "main", None, 10, Some("repo_a")).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "src/main.rs");

        // Scoped to another repo → no cross-repo leakage (F-1)
        let other = index.search(&reader, "main", None, 10, Some("repo_b")).unwrap();
        assert_eq!(other.len(), 0);

        // Unscoped still sees it (back-compat)
        let all = index.search(&reader, "main", None, 10, None).unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn test_delete_document() {
        let temp_dir = TempDir::new().unwrap();
        let index_path = temp_dir.path().to_str().unwrap();

        let index = TantivyIndex::open(index_path).unwrap();
        let mut writer = index.writer().unwrap();

        // Same relative path in two repos (TASK-030: deletes must be repo-scoped)
        index
            .add_document(&mut writer, "src/main.rs", "fn main() {}", "rust", &[], "repo_a")
            .unwrap();
        index
            .add_document(&mut writer, "src/main.rs", "fn main() {}", "rust", &[], "repo_b")
            .unwrap();
        index.commit(&mut writer).unwrap();

        // Delete only repo_a's copy
        index.delete_document(&mut writer, "src/main.rs", "repo_a").unwrap();
        index.commit(&mut writer).unwrap();

        let reader = index.reader().unwrap();
        let repo_a = index.search(&reader, "main", None, 10, Some("repo_a")).unwrap();
        assert_eq!(repo_a.len(), 0);
        let repo_b = index.search(&reader, "main", None, 10, Some("repo_b")).unwrap();
        assert_eq!(repo_b.len(), 1, "other repo's doc must survive");
    }



    #[test]
    fn test_index_stats() {
        let temp_dir = TempDir::new().unwrap();
        let index_path = temp_dir.path().to_str().unwrap();

        let index = TantivyIndex::open(index_path).unwrap();
        let mut writer = index.writer().unwrap();

        index
            .add_document(&mut writer, "test.rs", "test", "rust", &[], "repo_a")
            .unwrap();
        index.commit(&mut writer).unwrap();

        let reader = index.reader().unwrap();
        let stats = index.stats(&reader).unwrap();

        assert_eq!(stats.doc_count, 1);
    }
}
