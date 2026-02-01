use crate::entities::{FileEntry, FileMetadata, FileStream, SourceId};
use crate::errors::DomainError;
use crate::ports::SourceReader;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Mutex;

/// An in-memory source implementation for testing.
///
/// This test double allows you to configure files and their content
/// for use in unit tests without needing real external sources.
pub struct InMemorySource {
    source_id: SourceId,
    files: Mutex<HashMap<String, Vec<FileEntry>>>,
    content: Mutex<HashMap<String, Bytes>>,
    metadata: Mutex<HashMap<String, FileMetadata>>,
}

impl InMemorySource {
    /// Creates a new empty in-memory source.
    #[must_use]
    pub fn new(source_id: impl Into<SourceId>) -> Self {
        Self {
            source_id: source_id.into(),
            files: Mutex::new(HashMap::new()),
            content: Mutex::new(HashMap::new()),
            metadata: Mutex::new(HashMap::new()),
        }
    }

    /// Adds file entries at a given path.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn with_files(self, path: impl Into<String>, files: Vec<FileEntry>) -> Self {
        self.files.lock().unwrap().insert(path.into(), files);
        self
    }

    /// Adds file content at a given path.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn with_content(self, path: impl Into<String>, content: impl Into<Bytes>) -> Self {
        self.content
            .lock()
            .unwrap()
            .insert(path.into(), content.into());
        self
    }

    /// Adds metadata for a given path.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn with_metadata(self, path: impl Into<String>, metadata: FileMetadata) -> Self {
        self.metadata.lock().unwrap().insert(path.into(), metadata);
        self
    }
}

impl SourceReader for InMemorySource {
    async fn list_files(&self, path: &str) -> Result<Vec<FileEntry>, DomainError> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| DomainError::FileNotFound {
                path: path.to_string(),
            })
    }

    async fn get_metadata(&self, path: &str) -> Result<FileMetadata, DomainError> {
        self.metadata
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| DomainError::FileNotFound {
                path: path.to_string(),
            })
    }

    async fn read_file(&self, path: &str) -> Result<FileStream, DomainError> {
        let content = self
            .content
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| DomainError::FileNotFound {
                path: path.to_string(),
            })?;

        Ok(FileStream::from_bytes(content))
    }

    fn source_id(&self) -> &SourceId {
        &self.source_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn should_return_file_entries_when_path_exists() {
        let source = InMemorySource::new("test").with_files(
            "/",
            vec![FileEntry::file("a.txt"), FileEntry::directory("dir")],
        );

        let files = source.list_files("/").await.unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "a.txt");
        assert_eq!(files[1].name, "dir");
    }

    #[tokio::test]
    async fn should_return_error_when_listing_nonexistent_path() {
        let source = InMemorySource::new("test");

        let result = source.list_files("/nonexistent").await;
        assert!(matches!(result, Err(DomainError::FileNotFound { .. })));
    }

    #[tokio::test]
    async fn should_return_metadata_when_path_exists() {
        let meta = FileMetadata::new("/file.txt").with_size(100);
        let source = InMemorySource::new("test").with_metadata("/file.txt", meta.clone());

        let result = source.get_metadata("/file.txt").await.unwrap();
        assert_eq!(result, meta);
    }

    #[tokio::test]
    async fn should_return_content_stream_when_file_exists() {
        let source = InMemorySource::new("test").with_content("/file.txt", "hello world");

        let mut stream = source.read_file("/file.txt").await.unwrap();
        let chunk = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk, Bytes::from("hello world"));
    }

    #[tokio::test]
    async fn should_return_configured_source_id_when_queried() {
        let source = InMemorySource::new("my-source");
        assert_eq!(source.source_id().as_str(), "my-source");
    }
}
