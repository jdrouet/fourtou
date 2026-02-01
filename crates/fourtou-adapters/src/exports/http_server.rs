//! HTTP server export adapter.
//!
//! This adapter serves files from sources over HTTP using axum.

use std::collections::HashMap;
use std::fmt::Write;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{Method, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use fourtou_domain::{DomainError, Exporter, FileEntry, FileType, SourceReader};
use futures::StreamExt;
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::sources::AnySource;

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

/// A source with its alias for routing.
#[derive(Debug, Clone)]
pub struct AliasedSource {
    /// The alias used in URL paths.
    pub alias: String,
    /// The source reader.
    pub source: Arc<AnySource>,
}

/// Shared state for the HTTP server handlers.
struct AppState {
    sources: HashMap<String, Arc<AnySource>>,
}

/// HTTP server exporter.
///
/// This exporter serves files from source readers over HTTP.
#[derive(Debug)]
pub struct HttpExporter {
    config: HttpExporterConfig,
    sources: Vec<AliasedSource>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl HttpExporter {
    /// Creates a new HTTP exporter with the given configuration and sources.
    #[must_use]
    pub fn new(config: HttpExporterConfig, sources: Vec<AliasedSource>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            config,
            sources,
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

    /// Builds the axum router for the HTTP server.
    fn build_router(&self) -> Router {
        let mut sources_map = HashMap::new();
        for aliased in &self.sources {
            sources_map.insert(aliased.alias.clone(), Arc::clone(&aliased.source));
        }

        let state = Arc::new(AppState {
            sources: sources_map,
        });

        let prefix = self.config.prefix.trim_end_matches('/');

        // Build the router with routes for each source alias
        let app = Router::new()
            .route("/", get(handle_root))
            .route("/{alias}", get(handle_source_root))
            .route("/{alias}/", get(handle_source_root))
            .route("/{alias}/{*path}", get(handle_path))
            .with_state(state);

        // Apply prefix if configured
        if prefix.is_empty() {
            app
        } else {
            Router::new().nest(prefix, app)
        }
    }
}

impl Exporter for HttpExporter {
    async fn serve(&self) -> Result<(), DomainError> {
        let router = self.build_router();

        let listener = TcpListener::bind(self.config.socket)
            .await
            .map_err(DomainError::Io)?;

        tracing::info!(
            socket = ?self.config.socket,
            prefix = ?self.config.prefix,
            sources = self.sources.len(),
            "Starting HTTP exporter"
        );

        let mut shutdown_rx = self.shutdown_rx.clone();

        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                shutdown_rx.changed().await.ok();
            })
            .await
            .map_err(|err| DomainError::Unexpected(err.into()))?;

        tracing::info!("HTTP exporter stopped");
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), DomainError> {
        tracing::info!("Shutting down HTTP exporter");
        self.shutdown_tx.send(true).ok();
        Ok(())
    }
}

/// Handler for the root path - lists available sources.
async fn handle_root(State(state): State<Arc<AppState>>) -> Html<String> {
    let mut html = String::from(
        r"<!DOCTYPE html>
<html>
<head><title>Fourtou - Sources</title></head>
<body>
<h1>Available Sources</h1>
<ul>
",
    );

    let mut aliases: Vec<_> = state.sources.keys().collect();
    aliases.sort();

    for alias in aliases {
        let _ = writeln!(html, r#"<li><a href="{alias}/">{alias}</a></li>"#);
    }

    html.push_str(
        r"</ul>
</body>
</html>",
    );

    Html(html)
}

/// Handler for source root path - lists files at root of source.
async fn handle_source_root(
    State(state): State<Arc<AppState>>,
    Path(alias): Path<String>,
    method: Method,
) -> Response {
    handle_source_path(&state, &alias, "/", &method).await
}

/// Handler for paths within a source.
async fn handle_path(
    State(state): State<Arc<AppState>>,
    Path((alias, path)): Path<(String, String)>,
    method: Method,
) -> Response {
    let normalized_path = if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    };

    handle_source_path(&state, &alias, &normalized_path, &method).await
}

/// Common handler for source paths.
async fn handle_source_path(
    state: &AppState,
    alias: &str,
    path: &str,
    method: &Method,
) -> Response {
    let Some(source) = state.sources.get(alias) else {
        return (StatusCode::NOT_FOUND, "Source not found").into_response();
    };

    // Try to list as directory first
    match source.list_files(path).await {
        Ok(entries) => {
            // It's a directory - return listing
            render_directory_listing(alias, path, &entries).into_response()
        }
        Err(DomainError::FileNotFound { .. }) => {
            // Not a directory, try as file
            serve_file(source.as_ref(), path, method).await
        }
        Err(e) => {
            tracing::error!(error = ?e, path = ?path, "Failed to list directory");
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
        }
    }
}

/// Renders an HTML directory listing.
fn render_directory_listing(alias: &str, path: &str, entries: &[FileEntry]) -> Html<String> {
    let display_path = if path == "/" {
        format!("/{alias}/")
    } else {
        format!("/{alias}{path}")
    };

    let mut html = format!(
        r"<!DOCTYPE html>
<html>
<head><title>Index of {display_path}</title></head>
<body>
<h1>Index of {display_path}</h1>
<ul>
"
    );

    // Add parent directory link if not at root
    if path != "/" {
        let parent = parent_path(path);
        let parent_href = if parent == "/" {
            format!("/{alias}/")
        } else {
            format!("/{alias}{parent}")
        };
        let _ = writeln!(html, r#"<li><a href="{parent_href}">../</a></li>"#);
    }

    // Sort entries: directories first, then files, alphabetically
    let mut sorted_entries: Vec<_> = entries.iter().collect();
    sorted_entries.sort_by(|a, b| match (a.file_type, b.file_type) {
        (FileType::Directory, FileType::File) => std::cmp::Ordering::Less,
        (FileType::File, FileType::Directory) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    for entry in sorted_entries {
        let name = &entry.name;
        let display_name = if entry.is_directory() {
            format!("{name}/")
        } else {
            name.clone()
        };

        let href = if path == "/" {
            format!("/{alias}/{name}")
        } else {
            format!("/{alias}{path}/{name}")
        };

        let href = if entry.is_directory() {
            format!("{href}/")
        } else {
            href
        };

        let _ = writeln!(html, r#"<li><a href="{href}">{display_name}</a></li>"#);
    }

    html.push_str(
        r"</ul>
</body>
</html>",
    );

    Html(html)
}

/// Serves a file from the source.
async fn serve_file(source: &AnySource, path: &str, method: &Method) -> Response {
    // Get metadata first
    let metadata = match source.get_metadata(path).await {
        Ok(m) => m,
        Err(DomainError::FileNotFound { .. }) => {
            return (StatusCode::NOT_FOUND, "File not found").into_response();
        }
        Err(e) => {
            tracing::error!(error = ?e, path = ?path, "Failed to get file metadata");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    // Determine content type
    let content_type = metadata
        .content_type
        .clone()
        .unwrap_or_else(|| guess_mime_type(path));

    // Build response headers
    let mut headers = vec![(header::CONTENT_TYPE, content_type)];

    if let Some(size) = metadata.size {
        headers.push((header::CONTENT_LENGTH, size.to_string()));
    }

    // For HEAD requests, just return headers
    if *method == Method::HEAD {
        let mut response = Response::builder().status(StatusCode::OK);
        for (name, value) in headers {
            response = response.header(name, value);
        }
        return response.body(Body::empty()).unwrap();
    }

    // For GET requests, stream the file content
    let stream = match source.read_file(path).await {
        Ok(s) => s,
        Err(DomainError::FileNotFound { .. }) => {
            return (StatusCode::NOT_FOUND, "File not found").into_response();
        }
        Err(e) => {
            tracing::error!(error = ?e, path = ?path, "Failed to read file");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    // Convert FileStream to axum Body
    let body_stream =
        stream.map(|result| result.map_err(|err| std::io::Error::other(err.to_string())));
    let body = Body::from_stream(body_stream);

    let mut response = Response::builder().status(StatusCode::OK);
    for (name, value) in headers {
        response = response.header(name, value);
    }
    response.body(body).unwrap()
}

/// Guesses the MIME type from the file path.
fn guess_mime_type(path: &str) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string()
}

/// Gets the parent path of a given path.
fn parent_path(path: &str) -> &str {
    if path == "/" {
        return "/";
    }

    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => "/",
        Some(idx) => &trimmed[..idx],
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
    fn should_return_root_when_parent_of_root_path() {
        assert_eq!(parent_path("/"), "/");
    }

    #[test]
    fn should_return_root_when_parent_of_top_level_path() {
        assert_eq!(parent_path("/foo"), "/");
        assert_eq!(parent_path("/foo/"), "/");
    }

    #[test]
    fn should_return_parent_directory_when_nested_path() {
        assert_eq!(parent_path("/foo/bar"), "/foo");
        assert_eq!(parent_path("/foo/bar/"), "/foo");
        assert_eq!(parent_path("/a/b/c"), "/a/b");
    }

    #[test]
    fn should_guess_html_mime_type_when_html_extension() {
        assert_eq!(guess_mime_type("/index.html"), "text/html");
    }

    #[test]
    fn should_guess_json_mime_type_when_json_extension() {
        assert_eq!(guess_mime_type("/data.json"), "application/json");
    }

    #[test]
    fn should_guess_octet_stream_when_unknown_extension() {
        assert_eq!(guess_mime_type("/file.xyz123"), "application/octet-stream");
    }
}
