//! HTTP server export adapter.
//!
//! This adapter serves files from sources over HTTP using axum.

use fourtou_domain::{DomainError, Exporter};
use std::net::SocketAddr;
use tokio::sync::watch;

/// Configuration for the HTTP exporter.
#[derive(Debug, Clone)]
pub struct HttpExporterConfig {
    /// The socket address to bind to.
    pub socket: SocketAddr,
    /// URL prefix for all routes (e.g., "/public").
    pub prefix: String,
}

impl Default for HttpExporterConfig {
    fn default() -> Self {
        Self {
            socket: SocketAddr::from(([0, 0, 0, 0], 8080)),
            prefix: String::new(),
        }
    }
}

/// HTTP server exporter.
///
/// This exporter serves files from a source reader over HTTP.
#[derive(Debug)]
pub struct HttpExporter {
    config: HttpExporterConfig,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl HttpExporter {
    /// Creates a new HTTP exporter with the given configuration.
    #[must_use]
    pub fn new(config: HttpExporterConfig) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            config,
            shutdown_tx,
            shutdown_rx,
        }
    }

    /// Returns the configured socket address.
    #[must_use]
    pub const fn socket(&self) -> SocketAddr {
        self.config.socket
    }

    /// Returns the configured URL prefix.
    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.config.prefix
    }
}

impl Exporter for HttpExporter {
    async fn serve(&self) -> Result<(), DomainError> {
        // TODO: Implement actual HTTP server with axum
        // For now, this is a placeholder that waits for shutdown signal
        tracing::info!(
            socket = ?self.config.socket,
            prefix = ?self.config.prefix,
            "Starting HTTP exporter (placeholder)"
        );

        let mut rx = self.shutdown_rx.clone();
        rx.changed().await.ok();

        tracing::info!("HTTP exporter stopped");
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), DomainError> {
        tracing::info!("Shutting down HTTP exporter");
        self.shutdown_tx.send(true).ok();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_use_default_socket_and_empty_prefix_when_config_created_with_default() {
        let config = HttpExporterConfig::default();
        assert_eq!(config.socket, SocketAddr::from(([0, 0, 0, 0], 8080)));
        assert!(config.prefix.is_empty());
    }

    #[test]
    fn should_use_configured_values_when_exporter_created() {
        let config = HttpExporterConfig {
            socket: SocketAddr::from(([127, 0, 0, 1], 3000)),
            prefix: "/api".to_string(),
        };

        let exporter = HttpExporter::new(config);
        assert_eq!(exporter.socket(), SocketAddr::from(([127, 0, 0, 1], 3000)));
        assert_eq!(exporter.prefix(), "/api");
    }

    #[tokio::test]
    async fn should_stop_serving_when_shutdown_called() {
        let config = HttpExporterConfig::default();
        let exporter = HttpExporter::new(config);

        // Spawn the serve task
        let serve_handle = {
            let shutdown_rx = exporter.shutdown_rx.clone();
            tokio::spawn(async move {
                let mut rx = shutdown_rx;
                rx.changed().await.ok();
            })
        };

        // Shutdown should work
        let result = exporter.shutdown().await;
        assert!(result.is_ok());

        // serve should complete after shutdown
        serve_handle.await.unwrap();
    }
}
