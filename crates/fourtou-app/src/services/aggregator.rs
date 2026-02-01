//! File aggregation service.
//!
//! This service aggregates files from multiple sources into a unified view.

use crate::errors::AppError;
use fourtou_domain::{FileEntry, FileMetadata, FileStream, SourceId, SourceReader};
use std::sync::Arc;

/// A file entry with its source information.
#[derive(Debug, Clone)]
pub struct AggregatedFile {
    /// The source this file came from.
    pub source_id: SourceId,
    /// The file entry.
    pub entry: FileEntry,
}

/// Service that aggregates files from multiple sources.
///
/// This service provides a unified view of files across multiple sources,
/// allowing clients to list, read metadata, and download files without
/// needing to know which specific source they come from.
pub struct FileAggregatorService<S>
where
    S: SourceReader,
{
    sources: Vec<Arc<S>>,
}

impl<S> FileAggregatorService<S>
where
    S: SourceReader,
{
    /// Creates a new file aggregator service with the given sources.
    #[must_use]
    pub const fn new(sources: Vec<Arc<S>>) -> Self {
        Self { sources }
    }

    /// Lists files from all sources at the given path.
    ///
    /// Files are returned with their source IDs so callers know where each
    /// file came from. Errors from individual sources are logged but don't
    /// prevent other sources from being queried.
    ///
    /// # Errors
    ///
    /// This method currently always succeeds (returns `Ok`), as individual
    /// source errors are logged but not propagated.
    pub async fn list_all_files(&self, path: &str) -> Result<Vec<AggregatedFile>, AppError> {
        let mut results = Vec::new();

        for source in &self.sources {
            match source.list_files(path).await {
                Ok(files) => {
                    for entry in files {
                        results.push(AggregatedFile {
                            source_id: source.source_id().clone(),
                            entry,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        source_id = ?source.source_id(),
                        error = ?e,
                        "Failed to list files from source"
                    );
                }
            }
        }

        Ok(results)
    }

    /// Lists files from a specific source at the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the source is not found or if listing files fails.
    pub async fn list_files_from_source(
        &self,
        source_id: &str,
        path: &str,
    ) -> Result<Vec<FileEntry>, AppError> {
        let source = self
            .sources
            .iter()
            .find(|s| s.source_id().as_str() == source_id)
            .ok_or_else(|| AppError::SourceNotFound(source_id.to_string()))?;

        source
            .list_files(path)
            .await
            .map_err(|e| AppError::AggregationFailed {
                source_id: source_id.to_string(),
                cause: e,
            })
    }

    /// Gets metadata for a file from a specific source.
    ///
    /// # Errors
    ///
    /// Returns an error if the source is not found or if getting metadata fails.
    pub async fn get_metadata(
        &self,
        source_id: &str,
        path: &str,
    ) -> Result<FileMetadata, AppError> {
        let source = self
            .sources
            .iter()
            .find(|s| s.source_id().as_str() == source_id)
            .ok_or_else(|| AppError::SourceNotFound(source_id.to_string()))?;

        source
            .get_metadata(path)
            .await
            .map_err(|e| AppError::AggregationFailed {
                source_id: source_id.to_string(),
                cause: e,
            })
    }

    /// Reads a file from a specific source.
    ///
    /// # Errors
    ///
    /// Returns an error if the source is not found or if reading the file fails.
    pub async fn read_file(&self, source_id: &str, path: &str) -> Result<FileStream, AppError> {
        let source = self
            .sources
            .iter()
            .find(|s| s.source_id().as_str() == source_id)
            .ok_or_else(|| AppError::SourceNotFound(source_id.to_string()))?;

        source
            .read_file(path)
            .await
            .map_err(|e| AppError::AggregationFailed {
                source_id: source_id.to_string(),
                cause: e,
            })
    }

    /// Returns the number of registered sources.
    #[must_use]
    pub const fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Returns the IDs of all registered sources.
    #[must_use]
    pub fn source_ids(&self) -> Vec<&SourceId> {
        self.sources.iter().map(|s| s.source_id()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fourtou_domain::ports::test_support::InMemorySource;

    #[tokio::test]
    async fn should_aggregate_files_from_all_sources_when_listing() {
        let source1 = Arc::new(InMemorySource::new("source1").with_files(
            "/",
            vec![FileEntry::file("a.txt"), FileEntry::file("b.txt")],
        ));

        let source2 = Arc::new(
            InMemorySource::new("source2").with_files("/", vec![FileEntry::file("c.txt")]),
        );

        let service = FileAggregatorService::new(vec![source1, source2]);
        let files = service.list_all_files("/").await.unwrap();

        assert_eq!(files.len(), 3);
        assert_eq!(files[0].source_id.as_str(), "source1");
        assert_eq!(files[0].entry.name, "a.txt");
        assert_eq!(files[1].source_id.as_str(), "source1");
        assert_eq!(files[1].entry.name, "b.txt");
        assert_eq!(files[2].source_id.as_str(), "source2");
        assert_eq!(files[2].entry.name, "c.txt");
    }

    #[tokio::test]
    async fn should_continue_listing_when_one_source_fails() {
        let source1 = Arc::new(InMemorySource::new("source1")); // No files at "/"
        let source2 = Arc::new(
            InMemorySource::new("source2").with_files("/", vec![FileEntry::file("file.txt")]),
        );

        let service = FileAggregatorService::new(vec![source1, source2]);
        let files = service.list_all_files("/").await.unwrap();

        // Should still get files from source2 even though source1 failed
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].source_id.as_str(), "source2");
    }

    #[tokio::test]
    async fn should_return_files_from_specific_source_when_source_id_provided() {
        let source1 = Arc::new(
            InMemorySource::new("source1").with_files("/", vec![FileEntry::file("a.txt")]),
        );
        let source2 = Arc::new(
            InMemorySource::new("source2").with_files("/", vec![FileEntry::file("b.txt")]),
        );

        let service = FileAggregatorService::new(vec![source1, source2]);

        let files = service
            .list_files_from_source("source2", "/")
            .await
            .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "b.txt");
    }

    #[tokio::test]
    async fn should_return_error_when_source_not_found() {
        let source = Arc::new(InMemorySource::new("source1"));
        let service = FileAggregatorService::new(vec![source]);

        let result = service.list_files_from_source("unknown", "/").await;
        assert!(matches!(result, Err(AppError::SourceNotFound(_))));
    }

    #[tokio::test]
    async fn should_return_metadata_when_file_exists() {
        let meta = fourtou_domain::FileMetadata::new("/file.txt").with_size(100);
        let source =
            Arc::new(InMemorySource::new("source1").with_metadata("/file.txt", meta.clone()));

        let service = FileAggregatorService::new(vec![source]);
        let result = service.get_metadata("source1", "/file.txt").await.unwrap();

        assert_eq!(result, meta);
    }

    #[tokio::test]
    async fn should_return_file_stream_when_file_exists() {
        use bytes::Bytes;
        use futures::StreamExt;

        let source = Arc::new(InMemorySource::new("source1").with_content("/file.txt", "hello"));

        let service = FileAggregatorService::new(vec![source]);
        let mut stream = service.read_file("source1", "/file.txt").await.unwrap();

        let chunk = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk, Bytes::from("hello"));
    }

    #[tokio::test]
    async fn should_return_correct_count_when_querying_source_count() {
        let source1 = Arc::new(InMemorySource::new("s1"));
        let source2 = Arc::new(InMemorySource::new("s2"));

        let service = FileAggregatorService::new(vec![source1, source2]);
        assert_eq!(service.source_count(), 2);
    }

    #[tokio::test]
    async fn should_return_all_source_ids_when_querying_source_ids() {
        let source1 = Arc::new(InMemorySource::new("alpha"));
        let source2 = Arc::new(InMemorySource::new("beta"));

        let service = FileAggregatorService::new(vec![source1, source2]);
        let ids: Vec<&str> = service.source_ids().iter().map(|id| id.as_str()).collect();

        assert_eq!(ids, vec!["alpha", "beta"]);
    }
}
