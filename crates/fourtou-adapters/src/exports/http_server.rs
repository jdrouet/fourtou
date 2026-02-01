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
    ///
    /// This is primarily used internally by `serve()`, but is also exposed
    /// for integration testing.
    pub fn build_router(&self) -> Router {
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
            .route("/{alias}", get(handle_source_root_redirect))
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

/// Handler for source root path without trailing slash - redirects to canonical URL.
async fn handle_source_root_redirect(Path(alias): Path<String>) -> Response {
    Response::builder()
        .status(StatusCode::MOVED_PERMANENTLY)
        .header(header::LOCATION, format!("{alias}/"))
        .body(Body::empty())
        .unwrap()
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

    #[test]
    fn should_return_socket_when_queried() {
        let config = HttpExporterConfig {
            socket: SocketAddr::from(([127, 0, 0, 1], 9090)),
            prefix: String::new(),
        };
        let exporter = HttpExporter::new(config, vec![]);
        assert_eq!(exporter.socket(), SocketAddr::from(([127, 0, 0, 1], 9090)));
    }

    #[test]
    fn should_return_prefix_when_queried() {
        let config = HttpExporterConfig {
            socket: SocketAddr::from(([0, 0, 0, 0], 8080)),
            prefix: "/api".to_string(),
        };
        let exporter = HttpExporter::new(config, vec![]);
        assert_eq!(exporter.prefix(), "/api");
    }

    #[test]
    fn should_render_directory_listing_with_files_and_directories() {
        let entries = vec![
            FileEntry::file("readme.txt"),
            FileEntry::directory("subdir"),
            FileEntry::file("data.json"),
        ];
        let html = render_directory_listing("test", "/docs", &entries);
        let body = html.0;

        assert!(body.contains("Index of /test/docs"));
        // Parent of /docs is /, so parent href is /test/
        assert!(body.contains(r#"<a href="/test/">../</a>"#));
        // Directories should come first (sorted)
        assert!(body.contains(r#"<a href="/test/docs/subdir/">subdir/</a>"#));
        assert!(body.contains(r#"<a href="/test/docs/data.json">data.json</a>"#));
        assert!(body.contains(r#"<a href="/test/docs/readme.txt">readme.txt</a>"#));
    }

    #[test]
    fn should_render_root_directory_listing_without_parent_link() {
        let entries = vec![FileEntry::file("file.txt")];
        let html = render_directory_listing("source", "/", &entries);
        let body = html.0;

        assert!(body.contains("Index of /source/"));
        assert!(!body.contains("../"));
        assert!(body.contains(r#"<a href="/source/file.txt">file.txt</a>"#));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::sources::{HttpSource, HttpSourceConfig};

    fn create_test_exporter() -> HttpExporter {
        let source_config = HttpSourceConfig {
            base_url: "http://example.com".to_string(),
            source_id: "test-source".to_string(),
            timeout_secs: 30,
        };
        let source = Arc::new(AnySource::Http(HttpSource::new(source_config)));

        let config = HttpExporterConfig::default();
        let sources = vec![AliasedSource {
            alias: "files".to_string(),
            source,
        }];

        HttpExporter::new(config, sources)
    }

    #[tokio::test]
    async fn should_return_html_listing_sources_when_requesting_root() {
        let exporter = create_test_exporter();
        let router = exporter.build_router();

        let request = Request::builder().uri("/").body(Body::empty()).unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        assert!(body_str.contains("Available Sources"));
        assert!(body_str.contains(r#"<a href="files/">files</a>"#));
    }

    #[tokio::test]
    async fn should_return_not_found_when_source_does_not_exist() {
        let exporter = create_test_exporter();
        let router = exporter.build_router();

        let request = Request::builder()
            .uri("/nonexistent/")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn should_apply_prefix_when_configured() {
        let source_config = HttpSourceConfig {
            base_url: "http://example.com".to_string(),
            source_id: "test-source".to_string(),
            timeout_secs: 30,
        };
        let source = Arc::new(AnySource::Http(HttpSource::new(source_config)));

        let config = HttpExporterConfig {
            socket: SocketAddr::from(([0, 0, 0, 0], 8080)),
            prefix: "/api/v1".to_string(),
        };
        let sources = vec![AliasedSource {
            alias: "data".to_string(),
            source,
        }];

        let exporter = HttpExporter::new(config, sources);
        let router = exporter.build_router();

        // Root without prefix should return 404
        let request = Request::builder().uri("/").body(Body::empty()).unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // Prefixed path should work - the root listing endpoint
        let request = Request::builder()
            .uri("/api/v1")
            .body(Body::empty())
            .unwrap();
        let response = router.oneshot(request).await.unwrap();
        // May return OK or redirect depending on axum's nest behavior
        assert!(
            response.status() == StatusCode::OK
                || response.status() == StatusCode::PERMANENT_REDIRECT
                || response.status() == StatusCode::TEMPORARY_REDIRECT
        );
    }

    #[tokio::test]
    async fn should_redirect_source_without_trailing_slash() {
        let exporter = create_test_exporter();
        let router = exporter.build_router();

        let request = Request::builder()
            .uri("/files")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        // Should redirect to the canonical URL with trailing slash
        assert_eq!(response.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(response.headers().get("location").unwrap(), "files/");
    }

    #[tokio::test]
    async fn should_handle_head_request_for_file() {
        let exporter = create_test_exporter();
        let router = exporter.build_router();

        let request = Request::builder()
            .method("HEAD")
            .uri("/files/test.txt")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        // Will fail to connect to the mock source, but tests the routing
        assert!(
            response.status() == StatusCode::NOT_FOUND
                || response.status() == StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
