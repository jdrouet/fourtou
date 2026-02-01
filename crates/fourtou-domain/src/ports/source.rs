use crate::entities::{FileEntry, FileMetadata, FileStream, SourceId};
use crate::errors::DomainError;
use std::future::Future;

/// Port for reading from a data source.
///
/// This trait defines the interface that all source adapters must implement.
/// It uses return-position impl trait (RPITIT) to avoid `dyn` trait objects
/// while still supporting async methods.
pub trait SourceReader: Send + Sync {
    /// Lists files and directories at the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to list contents from (relative to the source root)
    ///
    /// # Returns
    ///
    /// A list of file entries at the given path, or an error if the path
    /// doesn't exist or cannot be read.
    fn list_files(
        &self,
        path: &str,
    ) -> impl Future<Output = Result<Vec<FileEntry>, DomainError>> + Send;

    /// Gets metadata for a specific file.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file (relative to the source root)
    ///
    /// # Returns
    ///
    /// Metadata about the file, or an error if the file doesn't exist.
    fn get_metadata(
        &self,
        path: &str,
    ) -> impl Future<Output = Result<FileMetadata, DomainError>> + Send;

    /// Reads the content of a file as a stream of bytes.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file (relative to the source root)
    ///
    /// # Returns
    ///
    /// A stream of bytes representing the file content, or an error if the
    /// file cannot be read.
    fn read_file(&self, path: &str)
        -> impl Future<Output = Result<FileStream, DomainError>> + Send;

    /// Returns the unique identifier of this source.
    fn source_id(&self) -> &SourceId;
}
