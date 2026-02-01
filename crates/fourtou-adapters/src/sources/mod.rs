//! Source adapters for reading from various storage backends.

#[cfg(feature = "http-source")]
mod http;

#[cfg(feature = "http-source")]
pub use http::{HttpSource, HttpSourceConfig};

use fourtou_domain::{DomainError, FileEntry, FileMetadata, FileStream, SourceId, SourceReader};

/// Enum dispatch for different source types.
///
/// This enum provides static dispatch for all supported source adapters,
/// avoiding the need for `dyn` trait objects while still allowing runtime
/// selection of source types based on configuration.
#[derive(Debug)]
pub enum AnySource {
    /// HTTP index source.
    #[cfg(feature = "http-source")]
    Http(HttpSource),

    /// Placeholder for S3 source (not yet implemented).
    S3Placeholder,

    /// Placeholder for Google Drive source (not yet implemented).
    GoogleDrivePlaceholder,

    /// Placeholder for pCloud source (not yet implemented).
    PCloudPlaceholder,

    /// Placeholder for NFS source (not yet implemented).
    NfsPlaceholder,
}

impl SourceReader for AnySource {
    async fn list_files(&self, path: &str) -> Result<Vec<FileEntry>, DomainError> {
        match self {
            #[cfg(feature = "http-source")]
            Self::Http(s) => s.list_files(path).await,
            Self::S3Placeholder
            | Self::GoogleDrivePlaceholder
            | Self::PCloudPlaceholder
            | Self::NfsPlaceholder => {
                Err(DomainError::SourceNotFound("not implemented".to_string()))
            }
        }
    }

    async fn get_metadata(&self, path: &str) -> Result<FileMetadata, DomainError> {
        match self {
            #[cfg(feature = "http-source")]
            Self::Http(s) => s.get_metadata(path).await,
            Self::S3Placeholder
            | Self::GoogleDrivePlaceholder
            | Self::PCloudPlaceholder
            | Self::NfsPlaceholder => {
                Err(DomainError::SourceNotFound("not implemented".to_string()))
            }
        }
    }

    async fn read_file(&self, path: &str) -> Result<FileStream, DomainError> {
        match self {
            #[cfg(feature = "http-source")]
            Self::Http(s) => s.read_file(path).await,
            Self::S3Placeholder
            | Self::GoogleDrivePlaceholder
            | Self::PCloudPlaceholder
            | Self::NfsPlaceholder => {
                Err(DomainError::SourceNotFound("not implemented".to_string()))
            }
        }
    }

    fn source_id(&self) -> &SourceId {
        match self {
            #[cfg(feature = "http-source")]
            Self::Http(s) => s.source_id(),
            Self::S3Placeholder
            | Self::GoogleDrivePlaceholder
            | Self::PCloudPlaceholder
            | Self::NfsPlaceholder => {
                // Return a static reference for placeholders
                static PLACEHOLDER_ID: std::sync::OnceLock<SourceId> = std::sync::OnceLock::new();
                PLACEHOLDER_ID.get_or_init(|| SourceId::new("placeholder"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn should_return_not_implemented_error_when_using_placeholder_source() {
        let source = AnySource::S3Placeholder;
        let result = source.list_files("/").await;
        assert!(matches!(result, Err(DomainError::SourceNotFound(_))));
    }
}
