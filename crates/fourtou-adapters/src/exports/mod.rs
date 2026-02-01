//! Export adapters for serving files through various protocols.

#[cfg(feature = "http-export")]
mod http_server;

#[cfg(feature = "http-export")]
pub use http_server::{AliasedSource, HttpExporter, HttpExporterConfig};

use fourtou_domain::{DomainError, Exporter};

/// Enum dispatch for different export types.
///
/// This enum provides static dispatch for all supported export adapters,
/// avoiding the need for `dyn` trait objects while still allowing runtime
/// selection of export types based on configuration.
#[derive(Debug)]
pub enum AnyExporter {
    /// HTTP server export.
    #[cfg(feature = "http-export")]
    Http(HttpExporter),

    /// Placeholder for Samba export (not yet implemented).
    SambaPlaceholder,

    /// Placeholder for NFS export (not yet implemented).
    NfsPlaceholder,
}

impl Exporter for AnyExporter {
    async fn serve(&self) -> Result<(), DomainError> {
        match self {
            #[cfg(feature = "http-export")]
            Self::Http(e) => e.serve().await,
            Self::SambaPlaceholder | Self::NfsPlaceholder => {
                Err(DomainError::SourceNotFound("not implemented".to_string()))
            }
        }
    }

    async fn shutdown(&self) -> Result<(), DomainError> {
        match self {
            #[cfg(feature = "http-export")]
            Self::Http(e) => e.shutdown().await,
            Self::SambaPlaceholder | Self::NfsPlaceholder => {
                Err(DomainError::SourceNotFound("not implemented".to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn should_return_not_implemented_error_when_using_placeholder_exporter() {
        let exporter = AnyExporter::SambaPlaceholder;
        let result = exporter.serve().await;
        assert!(matches!(result, Err(DomainError::SourceNotFound(_))));
    }
}
