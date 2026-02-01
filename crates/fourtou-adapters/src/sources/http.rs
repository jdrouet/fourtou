//! HTTP index source adapter.
//!
//! This adapter reads files from public HTTP indexes, such as those provided
//! by software mirrors (e.g., Ubuntu mirrors).

use fourtou_domain::{DomainError, FileEntry, FileMetadata, FileStream, SourceId, SourceReader};
use reqwest::Client;
use std::time::Duration;
use thiserror::Error;

/// Configuration for an HTTP source.
#[derive(Debug, Clone)]
pub struct HttpSourceConfig {
    /// The base URL of the HTTP index.
    pub base_url: String,
    /// The unique identifier for this source.
    pub source_id: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for HttpSourceConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            source_id: String::from("http"),
            timeout_secs: 30,
        }
    }
}

/// HTTP index source adapter.
#[derive(Debug)]
pub struct HttpSource {
    config: HttpSourceConfig,
    client: Client,
    source_id: SourceId,
}

/// Internal errors for the HTTP adapter.
#[derive(Error, Debug)]
enum HttpAdapterError {
    #[error("HTTP request failed")]
    RequestFailed(#[from] reqwest::Error),
}

impl From<HttpAdapterError> for DomainError {
    fn from(err: HttpAdapterError) -> Self {
        match err {
            HttpAdapterError::RequestFailed(e) => Self::ConnectionFailed {
                source_id: "http".to_string(),
                cause: e.into(),
            },
        }
    }
}

impl HttpSource {
    /// Creates a new HTTP source with the given configuration.
    ///
    /// # Panics
    ///
    /// Panics if the HTTP client cannot be created (should never happen with
    /// default settings).
    #[must_use]
    pub fn new(config: HttpSourceConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            source_id: SourceId::new(&config.source_id),
            config,
            client,
        }
    }

    /// Fetches the HTML content of a directory index.
    async fn fetch_index(&self, path: &str) -> Result<String, DomainError> {
        let url = format!("{}{}", self.config.base_url.trim_end_matches('/'), path);

        tracing::debug!(url = ?url, "Fetching HTTP index");

        let response =
            self.client
                .get(&url)
                .send()
                .await
                .map_err(|e| DomainError::ConnectionFailed {
                    source_id: self.config.source_id.clone(),
                    cause: e.into(),
                })?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(DomainError::FileNotFound {
                path: path.to_string(),
            });
        }

        if !response.status().is_success() {
            return Err(DomainError::ConnectionFailed {
                source_id: self.config.source_id.clone(),
                cause: anyhow::anyhow!("HTTP status: {}", response.status()),
            });
        }

        response
            .text()
            .await
            .map_err(|e| DomainError::Unexpected(e.into()))
    }

    /// Parses an HTML index page to extract file entries.
    ///
    /// This is a simple parser that looks for `<a href="...">` links.
    fn parse_index(html: &str) -> Vec<FileEntry> {
        let mut entries = Vec::new();

        // Simple regex-free parsing for href attributes
        for line in html.lines() {
            if let Some(start) = line.find("href=\"") {
                let rest = &line[start + 6..];
                if let Some(end) = rest.find('"') {
                    let href = &rest[..end];

                    // Skip parent directory, query strings, and absolute URLs
                    if href.starts_with("..")
                        || href.starts_with('?')
                        || href.starts_with('/')
                        || href.starts_with("http://")
                        || href.starts_with("https://")
                    {
                        continue;
                    }

                    let is_directory = href.ends_with('/');
                    let name = href.trim_end_matches('/');

                    if !name.is_empty() {
                        let entry = if is_directory {
                            FileEntry::directory(name)
                        } else {
                            FileEntry::file(name)
                        };
                        entries.push(entry);
                    }
                }
            }
        }

        entries
    }
}

impl SourceReader for HttpSource {
    async fn list_files(&self, path: &str) -> Result<Vec<FileEntry>, DomainError> {
        let html = self.fetch_index(path).await?;
        Ok(Self::parse_index(&html))
    }

    async fn get_metadata(&self, path: &str) -> Result<FileMetadata, DomainError> {
        let url = format!("{}{}", self.config.base_url.trim_end_matches('/'), path);

        let response =
            self.client
                .head(&url)
                .send()
                .await
                .map_err(|e| DomainError::ConnectionFailed {
                    source_id: self.config.source_id.clone(),
                    cause: e.into(),
                })?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(DomainError::FileNotFound {
                path: path.to_string(),
            });
        }

        let size = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let mut metadata = FileMetadata::new(path);
        if let Some(s) = size {
            metadata = metadata.with_size(s);
        }
        if let Some(ct) = content_type {
            metadata = metadata.with_content_type(ct);
        }

        Ok(metadata)
    }

    async fn read_file(&self, path: &str) -> Result<FileStream, DomainError> {
        let url = format!("{}{}", self.config.base_url.trim_end_matches('/'), path);

        let response =
            self.client
                .get(&url)
                .send()
                .await
                .map_err(|e| DomainError::ConnectionFailed {
                    source_id: self.config.source_id.clone(),
                    cause: e.into(),
                })?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(DomainError::FileNotFound {
                path: path.to_string(),
            });
        }

        let stream = response.bytes_stream();

        // Convert reqwest error to io error
        let mapped_stream =
            futures::stream::StreamExt::map(stream, |result| result.map_err(std::io::Error::other));

        Ok(FileStream::new(mapped_stream))
    }

    fn source_id(&self) -> &SourceId {
        &self.source_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_files_when_href_without_trailing_slash() {
        let html = r#"
            <html>
            <body>
            <a href="file1.txt">file1.txt</a>
            <a href="file2.iso">file2.iso</a>
            </body>
            </html>
        "#;

        let entries = HttpSource::parse_index(html);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "file1.txt");
        assert!(entries[0].is_file());
        assert_eq!(entries[1].name, "file2.iso");
        assert!(entries[1].is_file());
    }

    #[test]
    fn should_parse_directories_when_href_ends_with_slash() {
        let html = r#"
            <html>
            <body>
            <a href="subdir/">subdir</a>
            <a href="another-dir/">another-dir</a>
            </body>
            </html>
        "#;

        let entries = HttpSource::parse_index(html);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "subdir");
        assert!(entries[0].is_directory());
        assert_eq!(entries[1].name, "another-dir");
        assert!(entries[1].is_directory());
    }

    #[test]
    fn should_skip_parent_directory_when_href_starts_with_dotdot() {
        let html = r#"
            <html>
            <body>
            <a href="../">Parent</a>
            <a href="file.txt">file.txt</a>
            </body>
            </html>
        "#;

        let entries = HttpSource::parse_index(html);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "file.txt");
    }

    #[test]
    fn should_skip_query_strings_when_href_starts_with_question_mark() {
        let html = r#"
            <html>
            <body>
            <a href="?C=N;O=D">Name</a>
            <a href="file.txt">file.txt</a>
            </body>
            </html>
        "#;

        let entries = HttpSource::parse_index(html);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "file.txt");
    }

    #[test]
    fn should_skip_absolute_urls_when_href_is_absolute() {
        let html = r#"
            <html>
            <body>
            <a href="https://example.com">External</a>
            <a href="/absolute/path">Absolute</a>
            <a href="file.txt">file.txt</a>
            </body>
            </html>
        "#;

        let entries = HttpSource::parse_index(html);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "file.txt");
    }

    #[test]
    fn should_use_default_values_when_config_created_with_default() {
        let config = HttpSourceConfig::default();
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.source_id, "http");
    }

    #[test]
    fn should_use_configured_source_id_when_source_created() {
        let config = HttpSourceConfig {
            base_url: "https://example.com".to_string(),
            source_id: "my-http".to_string(),
            timeout_secs: 60,
        };

        let source = HttpSource::new(config);
        assert_eq!(source.source_id().as_str(), "my-http");
    }
}
