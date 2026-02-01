//! Configuration types for Fourtou.

use serde::Deserialize;
use std::collections::HashMap;

/// Root configuration structure.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    /// Configured data sources.
    #[serde(default)]
    pub sources: HashMap<String, SourceConfig>,

    /// Configured exports.
    #[serde(default)]
    pub exports: HashMap<String, ExportConfig>,
}

/// Source type discriminator.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SourceType {
    Http,
    S3,
    GoogleDrive,
    PCloud,
    Nfs,
}

/// Configuration for a data source.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SourceConfig {
    /// HTTP index source.
    Http(HttpSourceConfig),

    /// S3 source (not yet implemented).
    S3(S3SourceConfig),

    /// Google Drive source (not yet implemented).
    GoogleDrive(GoogleDriveSourceConfig),

    /// pCloud source (not yet implemented).
    PCloud(PCloudSourceConfig),

    /// NFS source (not yet implemented).
    Nfs(NfsSourceConfig),
}

/// Configuration for an HTTP source.
#[derive(Debug, Clone, Deserialize)]
pub struct HttpSourceConfig {
    /// The base URL of the HTTP index.
    pub base_url: String,

    /// Request timeout in seconds (default: 30).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

const fn default_timeout() -> u64 {
    30
}

/// Configuration for an S3 source (placeholder).
#[derive(Debug, Clone, Deserialize)]
pub struct S3SourceConfig {
    /// S3 bucket name.
    pub bucket: String,

    /// AWS region.
    pub region: Option<String>,

    /// Custom endpoint URL (for S3-compatible services).
    pub endpoint: Option<String>,
}

/// Configuration for a Google Drive source (placeholder).
#[derive(Debug, Clone, Deserialize)]
pub struct GoogleDriveSourceConfig {
    /// Path to credentials file.
    pub credentials_path: Option<String>,
}

/// Configuration for a pCloud source (placeholder).
#[derive(Debug, Clone, Deserialize)]
pub struct PCloudSourceConfig {
    /// pCloud username.
    pub username: Option<String>,
}

/// Configuration for an NFS source (placeholder).
#[derive(Debug, Clone, Deserialize)]
pub struct NfsSourceConfig {
    /// NFS server hostname or IP.
    pub server: String,

    /// Export path on the server.
    pub path: String,
}

/// Export type discriminator.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExportType {
    Http,
    Samba,
    Nfs,
}

/// Configuration for an export.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ExportConfig {
    /// HTTP server export.
    Http(HttpExportConfig),

    /// Samba export.
    Samba(SambaExportConfig),

    /// NFS export (not yet implemented).
    Nfs(NfsExportConfig),
}

/// Mapping from a source to an export path.
#[derive(Debug, Clone, Deserialize)]
pub struct SourceMapping {
    /// The name of the source to map.
    pub name: String,

    /// Optional alias to use in the export path.
    pub alias: Option<String>,
}

/// Configuration for an HTTP export.
#[derive(Debug, Clone, Deserialize)]
pub struct HttpExportConfig {
    /// Socket address to bind to (e.g., "0.0.0.0:8080").
    pub socket: String,

    /// URL prefix for all routes (e.g., "/public").
    #[serde(default)]
    pub prefix: String,

    /// Sources to expose through this export.
    #[serde(default)]
    pub sources: Vec<SourceMapping>,
}

/// Configuration for a Samba export.
#[derive(Debug, Clone, Deserialize)]
pub struct SambaExportConfig {
    /// Samba shares to expose.
    #[serde(default)]
    pub shares: HashMap<String, SambaShareConfig>,
}

/// Configuration for a single Samba share.
#[derive(Debug, Clone, Deserialize)]
pub struct SambaShareConfig {
    /// The source to expose as this share.
    pub source: String,

    /// Whether this share is read-only (default: true).
    #[serde(default = "default_read_only")]
    pub read_only: bool,
}

const fn default_read_only() -> bool {
    true
}

/// Configuration for an NFS export (placeholder).
#[derive(Debug, Clone, Deserialize)]
pub struct NfsExportConfig {
    /// Export path.
    pub path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_use_default_timeout_when_not_specified() {
        let toml = r#"
            base_url = "https://example.com/"
        "#;

        let config: HttpSourceConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn should_use_custom_timeout_when_specified() {
        let toml = r#"
            base_url = "https://example.com/"
            timeout_secs = 60
        "#;

        let config: HttpSourceConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.timeout_secs, 60);
    }

    #[test]
    fn should_have_no_alias_when_not_specified_in_mapping() {
        let toml = r#"
            name = "my-source"
        "#;

        let mapping: SourceMapping = toml::from_str(toml).unwrap();
        assert_eq!(mapping.name, "my-source");
        assert!(mapping.alias.is_none());
    }

    #[test]
    fn should_have_alias_when_specified_in_mapping() {
        let toml = r#"
            name = "my-source"
            alias = "files"
        "#;

        let mapping: SourceMapping = toml::from_str(toml).unwrap();
        assert_eq!(mapping.name, "my-source");
        assert_eq!(mapping.alias, Some("files".to_string()));
    }

    #[test]
    fn should_be_read_only_by_default_when_samba_share() {
        let toml = r#"
            source = "my-source"
        "#;

        let config: SambaShareConfig = toml::from_str(toml).unwrap();
        assert!(config.read_only);
    }

    #[test]
    fn should_be_writable_when_read_only_false() {
        let toml = r#"
            source = "my-source"
            read_only = false
        "#;

        let config: SambaShareConfig = toml::from_str(toml).unwrap();
        assert!(!config.read_only);
    }

    #[test]
    fn should_deserialize_source_types_correctly() {
        #[derive(Deserialize)]
        struct Wrapper {
            t: SourceType,
        }

        let w: Wrapper = toml::from_str(r#"t = "http""#).unwrap();
        assert_eq!(w.t, SourceType::Http);

        let w: Wrapper = toml::from_str(r#"t = "google-drive""#).unwrap();
        assert_eq!(w.t, SourceType::GoogleDrive);

        let w: Wrapper = toml::from_str(r#"t = "p-cloud""#).unwrap();
        assert_eq!(w.t, SourceType::PCloud);
    }

    #[test]
    fn should_deserialize_export_types_correctly() {
        #[derive(Deserialize)]
        struct Wrapper {
            t: ExportType,
        }

        let w: Wrapper = toml::from_str(r#"t = "http""#).unwrap();
        assert_eq!(w.t, ExportType::Http);

        let w: Wrapper = toml::from_str(r#"t = "samba""#).unwrap();
        assert_eq!(w.t, ExportType::Samba);

        let w: Wrapper = toml::from_str(r#"t = "nfs""#).unwrap();
        assert_eq!(w.t, ExportType::Nfs);
    }
}
