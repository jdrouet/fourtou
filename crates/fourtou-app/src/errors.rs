use fourtou_domain::DomainError;
use thiserror::Error;

/// Application-level errors.
#[derive(Error, Debug)]
pub enum AppError {
    /// An error occurred while aggregating files from a source.
    #[error("aggregation failed for source {source_id:?}")]
    AggregationFailed {
        source_id: String,
        #[source]
        cause: DomainError,
    },

    /// A configuration error occurred.
    #[error("configuration error")]
    Config(#[source] anyhow::Error),

    /// The requested source was not found in the registry.
    #[error("source not found: {0}")]
    SourceNotFound(String),

    /// A domain error occurred.
    #[error(transparent)]
    Domain(#[from] DomainError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_display_source_id_when_aggregation_failed() {
        let err = AppError::AggregationFailed {
            source_id: "my-source".to_string(),
            cause: DomainError::FileNotFound {
                path: "/test".to_string(),
            },
        };
        assert_eq!(
            err.to_string(),
            r#"aggregation failed for source "my-source""#
        );
    }

    #[test]
    fn should_display_message_when_config_error() {
        let err = AppError::Config(anyhow::anyhow!("invalid port"));
        assert_eq!(err.to_string(), "configuration error");
    }

    #[test]
    fn should_display_source_name_when_source_not_found() {
        let err = AppError::SourceNotFound("unknown".to_string());
        assert_eq!(err.to_string(), "source not found: unknown");
    }

    #[test]
    fn should_convert_to_domain_variant_when_from_domain_error() {
        let domain_err = DomainError::FileNotFound {
            path: "/file".to_string(),
        };
        let app_err: AppError = domain_err.into();
        assert!(matches!(app_err, AppError::Domain(_)));
    }
}
