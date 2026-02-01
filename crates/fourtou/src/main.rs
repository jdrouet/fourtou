//! Fourtou - Unified Data Store Aggregator
//!
//! This binary wires together all the components and runs the application.

use anyhow::{Context, Result};
use fourtou_adapters::exports::{AnyExporter, HttpExporter, HttpExporterConfig};
use fourtou_adapters::sources::{AnySource, HttpSource, HttpSourceConfig};
use fourtou_app::FileAggregatorService;
use fourtou_config::{Config, ExportConfig, SourceConfig};
use fourtou_domain::Exporter;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

/// Builds a source adapter from configuration.
fn build_source(name: &str, config: &SourceConfig) -> AnySource {
    match config {
        SourceConfig::Http(http) => {
            let adapter_config = HttpSourceConfig {
                base_url: http.base_url.clone(),
                source_id: name.to_string(),
                timeout_secs: http.timeout_secs,
            };
            AnySource::Http(HttpSource::new(adapter_config))
        }
        SourceConfig::S3(_) => {
            tracing::warn!(source = ?name, "S3 source not yet implemented");
            AnySource::S3Placeholder
        }
        SourceConfig::GoogleDrive(_) => {
            tracing::warn!(source = ?name, "Google Drive source not yet implemented");
            AnySource::GoogleDrivePlaceholder
        }
        SourceConfig::PCloud(_) => {
            tracing::warn!(source = ?name, "pCloud source not yet implemented");
            AnySource::PCloudPlaceholder
        }
        SourceConfig::Nfs(_) => {
            tracing::warn!(source = ?name, "NFS source not yet implemented");
            AnySource::NfsPlaceholder
        }
    }
}

/// Builds an exporter adapter from configuration.
fn build_exporter(name: &str, config: &ExportConfig) -> Result<AnyExporter> {
    match config {
        ExportConfig::Http(http) => {
            let socket: SocketAddr = http
                .socket
                .parse()
                .with_context(|| format!("invalid socket address for export {name:?}"))?;

            let adapter_config = HttpExporterConfig {
                socket,
                prefix: http.prefix.clone(),
            };
            Ok(AnyExporter::Http(HttpExporter::new(adapter_config)))
        }
        ExportConfig::Samba(_) => {
            tracing::warn!(export = ?name, "Samba export not yet implemented");
            Ok(AnyExporter::SambaPlaceholder)
        }
        ExportConfig::Nfs(_) => {
            tracing::warn!(export = ?name, "NFS export not yet implemented");
            Ok(AnyExporter::NfsPlaceholder)
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("fourtou=info".parse().unwrap()),
        )
        .init();

    tracing::info!("Fourtou starting up");

    // Determine config path
    let config_path = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("fourtou.toml"), PathBuf::from);

    // Load configuration
    let config = Config::load(&config_path)
        .with_context(|| format!("failed to load config from {}", config_path.display()))?;

    tracing::info!(
        sources = config.sources.len(),
        exports = config.exports.len(),
        "Configuration loaded"
    );

    // Build sources
    let sources: Vec<Arc<AnySource>> = config
        .sources
        .iter()
        .map(|(name, cfg)| {
            tracing::info!(source = ?name, "Building source");
            Arc::new(build_source(name, cfg))
        })
        .collect();

    // Create aggregator service
    let aggregator = Arc::new(FileAggregatorService::new(sources));

    tracing::info!(count = aggregator.source_count(), "Sources initialized");

    // Build and start exporters
    let mut exporter_handles = Vec::new();

    for (name, export_config) in &config.exports {
        tracing::info!(export = ?name, "Building exporter");
        let exporter = build_exporter(name, export_config)?;

        let handle = tokio::spawn(async move {
            if let Err(e) = exporter.serve().await {
                tracing::error!(error = ?e, "Exporter failed");
            }
        });

        exporter_handles.push(handle);
    }

    // Wait for shutdown signal
    tokio::signal::ctrl_c()
        .await
        .context("failed to listen for ctrl-c")?;

    tracing::info!("Shutdown signal received");

    // Cancel all exporter tasks
    for handle in exporter_handles {
        handle.abort();
    }

    tracing::info!("Fourtou stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fourtou_config::HttpSourceConfig as ConfigHttpSource;
    use fourtou_domain::SourceReader;

    #[test]
    fn should_build_http_source_when_config_type_is_http() {
        let config = SourceConfig::Http(ConfigHttpSource {
            base_url: "https://example.com/".to_string(),
            timeout_secs: 60,
        });

        let source = build_source("test", &config);
        assert!(matches!(source, AnySource::Http(_)));
        assert_eq!(source.source_id().as_str(), "test");
    }

    #[test]
    fn should_build_http_exporter_when_config_type_is_http() {
        let config = ExportConfig::Http(fourtou_config::HttpExportConfig {
            socket: "127.0.0.1:8080".to_string(),
            prefix: "/api".to_string(),
            sources: vec![],
        });

        let exporter = build_exporter("test", &config).unwrap();
        assert!(matches!(exporter, AnyExporter::Http(_)));
    }

    #[test]
    fn should_return_error_when_socket_address_invalid() {
        let config = ExportConfig::Http(fourtou_config::HttpExportConfig {
            socket: "invalid".to_string(),
            prefix: String::new(),
            sources: vec![],
        });

        let result = build_exporter("test", &config);
        assert!(result.is_err());
    }
}
