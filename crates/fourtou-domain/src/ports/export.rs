use crate::errors::DomainError;
use std::future::Future;

/// Port for exporting/serving files to clients.
///
/// This trait defines the interface that all export adapters must implement.
/// Exporters are responsible for serving files from sources through various
/// protocols (HTTP, Samba, NFS, etc.).
pub trait Exporter: Send + Sync {
    /// Starts serving files.
    ///
    /// This method should start the server and run until `shutdown` is called
    /// or an error occurs. It should be cancellation-safe.
    ///
    /// # Returns
    ///
    /// `Ok(())` when the server shuts down cleanly, or an error if something
    /// goes wrong during startup or operation.
    fn serve(&self) -> impl Future<Output = Result<(), DomainError>> + Send;

    /// Initiates a graceful shutdown of the server.
    ///
    /// This method signals the server to stop accepting new connections and
    /// finish processing existing requests before shutting down.
    ///
    /// # Returns
    ///
    /// `Ok(())` when the server has shut down, or an error if the shutdown
    /// fails.
    fn shutdown(&self) -> impl Future<Output = Result<(), DomainError>> + Send;
}
