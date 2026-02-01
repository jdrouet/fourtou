//! Export adapters for serving files through various protocols.

#[cfg(feature = "http-export")]
mod http_server;

#[cfg(feature = "http-export")]
pub use http_server::{HttpExporter, HttpExporterConfig, SourceMapping};

use fourtou_domain::{DomainError, Exporter, FileAggregator};

/// Enum dispatch for different export types.
///
/// This enum provides static dispatch for all supported export adapters,
/// avoiding the need for `dyn` trait objects while still allowing runtime
/// selection of export types based on configuration.
pub enum AnyExporter<A: FileAggregator> {
    /// HTTP server export.
    #[cfg(feature = "http-export")]
    Http(HttpExporter<A>),

    /// Placeholder for Samba export (not yet implemented).
    SambaPlaceholder(std::marker::PhantomData<A>),

    /// Placeholder for NFS export (not yet implemented).
    NfsPlaceholder(std::marker::PhantomData<A>),
}

impl<A: FileAggregator> std::fmt::Debug for AnyExporter<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "http-export")]
            Self::Http(e) => f.debug_tuple("Http").field(e).finish(),
            Self::SambaPlaceholder(_) => f.debug_tuple("SambaPlaceholder").finish(),
            Self::NfsPlaceholder(_) => f.debug_tuple("NfsPlaceholder").finish(),
        }
    }
}

impl<A: FileAggregator + 'static> Exporter for AnyExporter<A> {
    async fn serve(&self) -> Result<(), DomainError> {
        match self {
            #[cfg(feature = "http-export")]
            Self::Http(e) => e.serve().await,
            Self::SambaPlaceholder(_) | Self::NfsPlaceholder(_) => {
                Err(DomainError::SourceNotFound("not implemented".to_string()))
            }
        }
    }

    async fn shutdown(&self) -> Result<(), DomainError> {
        match self {
            #[cfg(feature = "http-export")]
            Self::Http(e) => e.shutdown().await,
            Self::SambaPlaceholder(_) | Self::NfsPlaceholder(_) => {
                Err(DomainError::SourceNotFound("not implemented".to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fourtou_app::FileAggregatorService;
    use fourtou_domain::ports::test_support::InMemorySource;

    #[tokio::test]
    async fn should_return_not_implemented_error_when_using_placeholder_exporter() {
        let exporter: AnyExporter<FileAggregatorService<InMemorySource>> =
            AnyExporter::SambaPlaceholder(std::marker::PhantomData);
        let result = exporter.serve().await;
        assert!(matches!(result, Err(DomainError::SourceNotFound(_))));
    }
}
