//! HTTP server export adapter.
//!
//! This adapter serves files from sources over HTTP using axum,
//! routing requests through the `FileAggregatorService`.

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
use fourtou_app::FileAggregatorService;
use fourtou_domain::{DomainError, Exporter, FileEntry, FileType, SourceReader};
use futures::StreamExt;
use tokio::net::TcpListener;
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

/// Maps a URL alias to a source ID in the aggregator.
#[derive(Debug, Clone)]
pub struct SourceMapping {
    /// The alias used in URL paths.
    pub alias: String,
    /// The source ID in the aggregator.
    pub source_id: String,
}

/// Shared state for the HTTP server handlers.
struct AppState<S: SourceReader> {
    /// The file aggregator service.
    aggregator: Arc<FileAggregatorService<S>>,
    /// Maps URL aliases to source IDs.
    alias_to_source: HashMap<String, String>,
}

/// HTTP server exporter.
///
/// This exporter serves files from sources via the `FileAggregatorService`.
pub struct HttpExporter<S: SourceReader> {
    config: HttpExporterConfig,
    aggregator: Arc<FileAggregatorService<S>>,
    mappings: Vec<SourceMapping>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl<S: SourceReader> std::fmt::Debug for HttpExporter<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpExporter")
            .field("config", &self.config)
            .field("mappings", &self.mappings)
            .finish_non_exhaustive()
    }
}

impl<S: SourceReader + 'static> HttpExporter<S> {
    /// Creates a new HTTP exporter with the given configuration and aggregator.
    #[must_use]
    pub fn new(
        config: HttpExporterConfig,
        aggregator: Arc<FileAggregatorService<S>>,
        mappings: Vec<SourceMapping>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            config,
            aggregator,
            mappings,
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
        let alias_to_source: HashMap<String, String> = self
            .mappings
            .iter()
            .map(|m| (m.alias.clone(), m.source_id.clone()))
            .collect();

        let state = Arc::new(AppState {
            aggregator: Arc::clone(&self.aggregator),
            alias_to_source,
        });

        let prefix = self.config.prefix.trim_end_matches('/');

        // Build the router with routes for each source alias
        let app = Router::new()
            .route("/", get(handle_root::<S>))
            .route("/{alias}", get(handle_source_root_redirect))
            .route("/{alias}/", get(handle_source_root::<S>))
            .route("/{alias}/{*path}", get(handle_path::<S>))
            .with_state(state);

        // Apply prefix if configured
        if prefix.is_empty() {
            app
        } else {
            Router::new().nest(prefix, app)
        }
    }
}

impl<S: SourceReader + 'static> Exporter for HttpExporter<S> {
    async fn serve(&self) -> Result<(), DomainError> {
        let router = self.build_router();

        let listener = TcpListener::bind(self.config.socket)
            .await
            .map_err(DomainError::Io)?;

        tracing::info!(
            socket = ?self.config.socket,
            prefix = ?self.config.prefix,
            mappings = self.mappings.len(),
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
async fn handle_root<S: SourceReader>(State(state): State<Arc<AppState<S>>>) -> Html<String> {
    let mut html = String::from(
        r"<!DOCTYPE html>
<html>
<head><title>Fourtou - Sources</title></head>
<body>
<h1>Available Sources</h1>
<ul>
",
    );

    let mut aliases: Vec<_> = state.alias_to_source.keys().collect();
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
async fn handle_source_root<S: SourceReader + 'static>(
    State(state): State<Arc<AppState<S>>>,
    Path(alias): Path<String>,
    method: Method,
) -> Response {
    handle_source_path(&state, &alias, "/", &method).await
}

/// Handler for paths within a source.
async fn handle_path<S: SourceReader + 'static>(
    State(state): State<Arc<AppState<S>>>,
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
async fn handle_source_path<S: SourceReader>(
    state: &AppState<S>,
    alias: &str,
    path: &str,
    method: &Method,
) -> Response {
    let Some(source_id) = state.alias_to_source.get(alias) else {
        return (StatusCode::NOT_FOUND, "Source not found").into_response();
    };

    // Try to list as directory first via the aggregator
    match state
        .aggregator
        .list_files_from_source(source_id, path)
        .await
    {
        Ok(entries) => {
            // It's a directory - return listing
            render_directory_listing(alias, path, &entries).into_response()
        }
        Err(err) => {
            // Check if it's a file not found error (might be a file, not a directory)
            if is_not_found_error(&err) {
                // Not a directory, try as file
                serve_file(&state.aggregator, source_id, path, method).await
            } else {
                tracing::error!(error = ?err, path = ?path, "Failed to list directory");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            }
        }
    }
}

/// Checks if an app error indicates a file/path not found.
const fn is_not_found_error(err: &fourtou_app::AppError) -> bool {
    matches!(
        err,
        fourtou_app::AppError::Domain(DomainError::FileNotFound { .. })
            | fourtou_app::AppError::AggregationFailed {
                cause: DomainError::FileNotFound { .. },
                ..
            }
    )
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

/// Serves a file from the aggregator.
async fn serve_file<S: SourceReader>(
    aggregator: &FileAggregatorService<S>,
    source_id: &str,
    path: &str,
    method: &Method,
) -> Response {
    // Get metadata first via the aggregator
    let metadata = match aggregator.get_metadata(source_id, path).await {
        Ok(m) => m,
        Err(err) => {
            if is_not_found_error(&err) {
                return (StatusCode::NOT_FOUND, "File not found").into_response();
            }
            tracing::error!(error = ?err, path = ?path, "Failed to get file metadata");
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

    // For GET requests, stream the file content via the aggregator
    let stream = match aggregator.read_file(source_id, path).await {
        Ok(s) => s,
        Err(err) => {
            if is_not_found_error(&err) {
                return (StatusCode::NOT_FOUND, "File not found").into_response();
            }
            tracing::error!(error = ?err, path = ?path, "Failed to read file");
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
    use fourtou_domain::ports::test_support::InMemorySource;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn create_test_aggregator() -> Arc<FileAggregatorService<InMemorySource>> {
        let source = Arc::new(
            InMemorySource::new("test-source")
                .with_files("/", vec![FileEntry::file("hello.txt")])
                .with_content("/hello.txt", "Hello, World!")
                .with_metadata(
                    "/hello.txt",
                    fourtou_domain::FileMetadata::new("/hello.txt")
                        .with_size(13)
                        .with_content_type("text/plain".to_string()),
                ),
        );
        Arc::new(FileAggregatorService::new(vec![source]))
    }

    fn create_test_exporter() -> HttpExporter<InMemorySource> {
        let aggregator = create_test_aggregator();
        let config = HttpExporterConfig::default();
        let mappings = vec![SourceMapping {
            alias: "files".to_string(),
            source_id: "test-source".to_string(),
        }];

        HttpExporter::new(config, aggregator, mappings)
    }

    #[test]
    fn should_return_socket_when_queried() {
        let aggregator = create_test_aggregator();
        let config = HttpExporterConfig {
            socket: SocketAddr::from(([127, 0, 0, 1], 9090)),
            prefix: String::new(),
        };
        let exporter = HttpExporter::new(config, aggregator, vec![]);
        assert_eq!(exporter.socket(), SocketAddr::from(([127, 0, 0, 1], 9090)));
    }

    #[test]
    fn should_return_prefix_when_queried() {
        let aggregator = create_test_aggregator();
        let config = HttpExporterConfig {
            socket: SocketAddr::from(([0, 0, 0, 0], 8080)),
            prefix: "/api".to_string(),
        };
        let exporter = HttpExporter::new(config, aggregator, vec![]);
        assert_eq!(exporter.prefix(), "/api");
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
    async fn should_list_directory_when_requesting_source_root() {
        let exporter = create_test_exporter();
        let router = exporter.build_router();

        let request = Request::builder()
            .uri("/files/")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        assert!(body_str.contains("Index of /files/"));
        assert!(body_str.contains("hello.txt"));
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
    async fn should_serve_file_content_when_requesting_file() {
        let exporter = create_test_exporter();
        let router = exporter.build_router();

        let request = Request::builder()
            .uri("/files/hello.txt")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        assert_eq!(body_str, "Hello, World!");
    }

    #[tokio::test]
    async fn should_return_headers_only_when_head_request() {
        let exporter = create_test_exporter();
        let router = exporter.build_router();

        let request = Request::builder()
            .method("HEAD")
            .uri("/files/hello.txt")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().contains_key("content-type"));

        // Body should be empty for HEAD requests
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn should_return_not_found_when_file_does_not_exist() {
        let exporter = create_test_exporter();
        let router = exporter.build_router();

        let request = Request::builder()
            .uri("/files/nonexistent.txt")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn should_apply_prefix_when_configured() {
        let aggregator = create_test_aggregator();
        let config = HttpExporterConfig {
            socket: SocketAddr::from(([0, 0, 0, 0], 8080)),
            prefix: "/api/v1".to_string(),
        };
        let mappings = vec![SourceMapping {
            alias: "data".to_string(),
            source_id: "test-source".to_string(),
        }];

        let exporter = HttpExporter::new(config, aggregator, mappings);
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
}
