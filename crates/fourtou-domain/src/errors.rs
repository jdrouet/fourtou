use thiserror::Error;

/// Domain-level errors for Fourtou operations.
#[derive(Error, Debug)]
pub enum DomainError {
    /// The requested source was not found.
    #[error("source not found: {0}")]
    SourceNotFound(String),

    /// The requested file or path was not found.
    #[error("file not found: {path}")]
    FileNotFound { path: String },

    /// Permission was denied for the requested operation.
    #[error("permission denied for path: {0}")]
    PermissionDenied(String),

    /// The provided path is invalid.
    #[error("invalid path: {0}")]
    InvalidPath(String),

    /// Failed to connect to or communicate with a source.
    #[error("connection failed to source {source_id:?}")]
    ConnectionFailed {
        source_id: String,
        #[source]
        cause: anyhow::Error,
    },

    /// An I/O error occurred.
    #[error("i/o error")]
    Io(#[from] std::io::Error),

    /// An unexpected error occurred.
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_display_source_name_when_source_not_found() {
        let err = DomainError::SourceNotFound("my-source".to_string());
        assert_eq!(err.to_string(), "source not found: my-source");
    }

    #[test]
    fn should_display_path_when_file_not_found() {
        let err = DomainError::FileNotFound {
            path: "/some/path".to_string(),
        };
        assert_eq!(err.to_string(), "file not found: /some/path");
    }

    #[test]
    fn should_display_path_when_permission_denied() {
        let err = DomainError::PermissionDenied("/secret".to_string());
        assert_eq!(err.to_string(), "permission denied for path: /secret");
    }

    #[test]
    fn should_display_path_when_invalid_path() {
        let err = DomainError::InvalidPath("..".to_string());
        assert_eq!(err.to_string(), "invalid path: ..");
    }

    #[test]
    fn should_display_source_id_when_connection_failed() {
        let err = DomainError::ConnectionFailed {
            source_id: "http-source".to_string(),
            cause: anyhow::anyhow!("timeout"),
        };
        assert_eq!(
            err.to_string(),
            r#"connection failed to source "http-source""#
        );
    }

    #[test]
    fn should_convert_to_io_variant_when_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: DomainError = io_err.into();
        assert!(matches!(err, DomainError::Io(_)));
    }
}
