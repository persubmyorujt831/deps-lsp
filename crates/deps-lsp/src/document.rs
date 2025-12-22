use crate::cache::HttpCache;
use crate::cargo::{ParsedDependency, types::CargoVersion};
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinHandle;
use tower_lsp::lsp_types::Url;

/// State for a single open document.
///
/// Stores the document content, parsed dependency information, and cached
/// version data for a single file. The state is updated when the document
/// changes or when version information is fetched from the registry.
///
/// # Examples
///
/// ```
/// use deps_lsp::document::DocumentState;
/// use deps_lsp::cargo::ParsedDependency;
///
/// let state = DocumentState::new(
///     "[dependencies]\nserde = \"1.0\"".into(),
///     vec![],
/// );
///
/// assert!(state.versions.is_empty());
/// assert_eq!(state.dependencies.len(), 0);
/// ```
#[derive(Debug, Clone)]
pub struct DocumentState {
    /// Original document content
    pub content: String,
    /// Parsed dependencies with positions
    pub dependencies: Vec<ParsedDependency>,
    /// Cached version information
    pub versions: HashMap<String, CargoVersion>,
    /// Last successful parse time
    pub parsed_at: Instant,
}

impl DocumentState {
    /// Creates a new document state.
    ///
    /// Initializes with the given content and parsed dependencies.
    /// Version information starts empty and is populated asynchronously.
    pub fn new(content: String, dependencies: Vec<ParsedDependency>) -> Self {
        Self {
            content,
            dependencies,
            versions: HashMap::new(),
            parsed_at: Instant::now(),
        }
    }

    /// Updates the cached version information for dependencies.
    ///
    /// This is called after fetching version data from the registry.
    pub fn update_versions(&mut self, versions: HashMap<String, CargoVersion>) {
        self.versions = versions;
    }
}

/// Global LSP server state.
///
/// Manages all open documents, HTTP cache, and background tasks for the server.
/// This state is shared across all LSP handlers via `Arc` and uses concurrent
/// data structures (`DashMap`, `RwLock`) for thread-safe access.
///
/// # Examples
///
/// ```
/// use deps_lsp::document::ServerState;
/// use tower_lsp::lsp_types::Url;
///
/// let state = ServerState::new();
/// assert_eq!(state.document_count(), 0);
/// ```
pub struct ServerState {
    /// Open documents by URI
    pub documents: DashMap<Url, DocumentState>,
    /// HTTP cache for registry requests
    pub cache: Arc<HttpCache>,
    /// Background task handles
    tasks: tokio::sync::RwLock<HashMap<Url, JoinHandle<()>>>,
}

impl ServerState {
    /// Creates a new server state with default configuration.
    pub fn new() -> Self {
        Self {
            documents: DashMap::new(),
            cache: Arc::new(HttpCache::new()),
            tasks: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Retrieves document state by URI.
    ///
    /// Returns a read-only reference to the document state if it exists.
    /// The reference holds a lock on the internal map, so it should be
    /// dropped as soon as possible.
    pub fn get_document(
        &self,
        uri: &Url,
    ) -> Option<dashmap::mapref::one::Ref<'_, Url, DocumentState>> {
        self.documents.get(uri)
    }

    /// Updates or inserts document state.
    ///
    /// If a document already exists at the given URI, it is replaced.
    /// Otherwise, a new entry is created.
    pub fn update_document(&self, uri: Url, state: DocumentState) {
        self.documents.insert(uri, state);
    }

    /// Removes document state and returns the removed entry.
    ///
    /// Returns `None` if no document exists at the given URI.
    pub fn remove_document(&self, uri: &Url) -> Option<(Url, DocumentState)> {
        self.documents.remove(uri)
    }

    /// Spawns a background task for a document.
    ///
    /// If a task already exists for the given URI, it is aborted before
    /// the new task is registered. This ensures only one background task
    /// runs per document.
    ///
    /// Typical use case: fetching version data asynchronously after
    /// document open or change.
    pub async fn spawn_background_task(&self, uri: Url, task: JoinHandle<()>) {
        let mut tasks = self.tasks.write().await;

        // Cancel existing task if any
        if let Some(old_task) = tasks.remove(&uri) {
            old_task.abort();
        }

        tasks.insert(uri, task);
    }

    /// Cancels the background task for a document.
    ///
    /// If no task exists, this is a no-op.
    pub async fn cancel_background_task(&self, uri: &Url) {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.remove(uri) {
            task.abort();
        }
    }

    /// Returns the number of open documents.
    pub fn document_count(&self) -> usize {
        self.documents.len()
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cargo::types::{DependencySection, DependencySource};
    use tower_lsp::lsp_types::{Position, Range};

    fn create_test_dependency() -> ParsedDependency {
        ParsedDependency {
            name: "serde".into(),
            name_range: Range::new(Position::new(0, 0), Position::new(0, 5)),
            version_req: Some("1.0".into()),
            version_range: Some(Range::new(Position::new(0, 9), Position::new(0, 14))),
            features: vec![],
            features_range: None,
            source: DependencySource::Registry,
            workspace_inherited: false,
            section: DependencySection::Dependencies,
        }
    }

    #[test]
    fn test_document_state_creation() {
        let deps = vec![create_test_dependency()];
        let state = DocumentState::new("test content".into(), deps.clone());

        assert_eq!(state.content, "test content");
        assert_eq!(state.dependencies.len(), 1);
        assert!(state.versions.is_empty());
    }

    #[test]
    fn test_document_state_update_versions() {
        let deps = vec![create_test_dependency()];
        let mut state = DocumentState::new("test".into(), deps);

        let mut versions = HashMap::new();
        versions.insert(
            "serde".into(),
            CargoVersion {
                num: "1.0.0".into(),
                yanked: false,
                features: HashMap::new(),
            },
        );

        state.update_versions(versions);
        assert_eq!(state.versions.len(), 1);
        assert!(state.versions.contains_key("serde"));
    }

    #[test]
    fn test_server_state_creation() {
        let state = ServerState::new();
        assert_eq!(state.document_count(), 0);
        assert!(state.cache.is_empty(), "Cache should start empty");
    }

    #[test]
    fn test_server_state_document_operations() {
        let state = ServerState::new();
        let uri = Url::parse("file:///test.toml").unwrap();
        let deps = vec![create_test_dependency()];
        let doc_state = DocumentState::new("test".into(), deps);

        // Insert document
        state.update_document(uri.clone(), doc_state.clone());
        assert_eq!(state.document_count(), 1);

        // Get document
        let retrieved = state.get_document(&uri);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().content, "test");

        // Remove document
        let removed = state.remove_document(&uri);
        assert!(removed.is_some());
        assert_eq!(state.document_count(), 0);
    }

    #[tokio::test]
    async fn test_server_state_background_tasks() {
        let state = ServerState::new();
        let uri = Url::parse("file:///test.toml").unwrap();

        // Spawn task
        let task = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        });

        state.spawn_background_task(uri.clone(), task).await;

        // Cancel task
        state.cancel_background_task(&uri).await;
    }
}
