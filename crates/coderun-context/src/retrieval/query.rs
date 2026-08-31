//! RetrievalQuery — normalized input to the Retrieval Engine.

/// Normalized query for the Retrieval Engine.
/// Deterministic and independently testable without an LLM.
#[derive(Debug, Clone)]
pub struct RetrievalQuery {
    pub text: String,
    pub repository_id: String,
    pub language: Option<String>,
}

impl RetrievalQuery {
    pub fn new(text: impl Into<String>, repository_id: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            repository_id: repository_id.into(),
            language: None,
        }
    }

    pub fn with_language(mut self, lang: impl Into<String>) -> Self {
        let l = lang.into();
        if !l.is_empty() {
            self.language = Some(l);
        }
        self
    }

    /// Effective text for Tantivy query sanitization (empty check)
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}
