//! File aggregator port definition.
//!
//! This port defines the interface for aggregating files from multiple sources.

use crate::entities::{FileEntry, FileMetadata, FileStream};
use crate::errors::DomainError;
use std::future::Future;

/// Port for aggregating files from multiple sources.
///
/// This trait defines the interface that exporters use to access files
/// from configured sources. Implementations handle source lookup and
/// delegation to the appropriate source reader.
pub trait FileAggregator: Send + Sync {
    /// Lists files from a specific source at the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the source is not found or if listing files fails.
    fn list_files(
        &self,
        source_id: &str,
        path: &str,
    ) -> impl Future<Output = Result<Vec<FileEntry>, DomainError>> + Send;

    /// Gets metadata for a file from a specific source.
    ///
    /// # Errors
    ///
    /// Returns an error if the source is not found or if getting metadata fails.
    fn get_metadata(
        &self,
        source_id: &str,
        path: &str,
    ) -> impl Future<Output = Result<FileMetadata, DomainError>> + Send;

    /// Reads a file from a specific source.
    ///
    /// # Errors
    ///
    /// Returns an error if the source is not found or if reading the file fails.
    fn read_file(
        &self,
        source_id: &str,
        path: &str,
    ) -> impl Future<Output = Result<FileStream, DomainError>> + Send;

    /// Returns the IDs of all available sources.
    fn source_ids(&self) -> Vec<String>;
}
