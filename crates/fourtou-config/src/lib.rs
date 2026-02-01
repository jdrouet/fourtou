//! Fourtou Configuration
//!
//! This crate handles loading and parsing of Fourtou's TOML configuration files.

mod types;

pub use types::{
    Config, ExportConfig, ExportType, HttpExportConfig, HttpSourceConfig, NfsExportConfig,
    SambaExportConfig, SambaShareConfig, SourceConfig, SourceMapping, SourceType,
};

use std::path::Path;
use thiserror::Error;

/// Configuration errors.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Failed to read the configuration file.
    #[error("failed to read config file")]
    IoError(#[from] std::io::Error),

    /// Failed to parse the configuration file.
    #[error("failed to parse config")]
    ParseError(#[from] toml::de::Error),

    /// The configuration is invalid.
    #[error("invalid configuration: {0}")]
    ValidationError(String),
}

impl Config {
    /// Loads configuration from a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Parses configuration from a TOML string.
    ///
    /// # Errors
    ///
    /// Returns an error if the TOML is invalid.
    pub fn parse(content: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(content)?;
        config.validate()?;
        Ok(config)
    }

    /// Validates the configuration.
    fn validate(&self) -> Result<(), ConfigError> {
        // Check for duplicate source names
        let mut source_names = std::collections::HashSet::new();
        for name in self.sources.keys() {
            if !source_names.insert(name) {
                return Err(ConfigError::ValidationError(format!(
                    "duplicate source name: {name}"
                )));
            }
        }

        // Check for duplicate export names
        let mut export_names = std::collections::HashSet::new();
        for name in self.exports.keys() {
            if !export_names.insert(name) {
                return Err(ConfigError::ValidationError(format!(
                    "duplicate export name: {name}"
                )));
            }
        }

        // Check that all source references in exports are valid
        for (export_name, export) in &self.exports {
            match export {
                ExportConfig::Http(http) => {
                    for mapping in &http.sources {
                        if !self.sources.contains_key(&mapping.name) {
                            return Err(ConfigError::ValidationError(format!(
                                "export {export_name:?} references unknown source: {:?}",
                                mapping.name
                            )));
                        }
                    }
                }
                ExportConfig::Samba(samba) => {
                    for (share_name, share) in &samba.shares {
                        if !self.sources.contains_key(&share.source) {
                            return Err(ConfigError::ValidationError(format!(
                                "samba share {share_name:?} references unknown source: {:?}",
                                share.source
                            )));
                        }
                    }
                }
                ExportConfig::Nfs(_) => {
                    // NFS validation will be added when implemented
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_empty_config_when_minimal_toml() {
        let toml = r"
            [sources]

            [exports]
        ";

        let config = Config::parse(toml).unwrap();
        assert!(config.sources.is_empty());
        assert!(config.exports.is_empty());
    }

    #[test]
    fn should_parse_http_source_when_type_is_http() {
        let toml = r#"
            [sources.ubuntu-images]
            type = "http"
            base_url = "https://ubuntu.mirrors.ovh.net/ubuntu-releases/"

            [exports]
        "#;

        let config = Config::parse(toml).unwrap();
        assert_eq!(config.sources.len(), 1);

        let source = config.sources.get("ubuntu-images").unwrap();
        match source {
            SourceConfig::Http(http) => {
                assert_eq!(
                    http.base_url,
                    "https://ubuntu.mirrors.ovh.net/ubuntu-releases/"
                );
            }
            _ => panic!("Expected HTTP source"),
        }
    }

    #[test]
    fn should_parse_http_export_when_type_is_http() {
        let toml = r#"
            [sources.my-source]
            type = "http"
            base_url = "https://example.com/"

            [exports.public-http]
            type = "http"
            socket = "0.0.0.0:4321"
            prefix = "/public"
            sources = [{ name = "my-source", alias = "files" }]
        "#;

        let config = Config::parse(toml).unwrap();
        assert_eq!(config.exports.len(), 1);

        let export = config.exports.get("public-http").unwrap();
        match export {
            ExportConfig::Http(http) => {
                assert_eq!(http.socket, "0.0.0.0:4321");
                assert_eq!(http.prefix, "/public");
                assert_eq!(http.sources.len(), 1);
                assert_eq!(http.sources[0].name, "my-source");
                assert_eq!(http.sources[0].alias, Some("files".to_string()));
            }
            _ => panic!("Expected HTTP export"),
        }
    }

    #[test]
    fn should_parse_samba_export_when_type_is_samba() {
        let toml = r#"
            [sources.family-pictures]
            type = "http"
            base_url = "https://example.com/"

            [exports.private-samba]
            type = "samba"

            [exports.private-samba.shares.family]
            source = "family-pictures"
        "#;

        let config = Config::parse(toml).unwrap();

        let export = config.exports.get("private-samba").unwrap();
        match export {
            ExportConfig::Samba(samba) => {
                assert_eq!(samba.shares.len(), 1);
                let share = samba.shares.get("family").unwrap();
                assert_eq!(share.source, "family-pictures");
            }
            _ => panic!("Expected Samba export"),
        }
    }

    #[test]
    fn should_return_validation_error_when_source_reference_unknown() {
        let toml = r#"
            [sources]

            [exports.public-http]
            type = "http"
            socket = "0.0.0.0:8080"
            sources = [{ name = "nonexistent" }]
        "#;

        let result = Config::parse(toml);
        assert!(matches!(result, Err(ConfigError::ValidationError(_))));
    }

    #[test]
    fn should_load_config_when_file_exists() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.toml");

        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "[sources]\n[exports]").unwrap();

        let config = Config::load(&file_path).unwrap();
        assert!(config.sources.is_empty());
    }

    #[test]
    fn should_return_io_error_when_file_not_found() {
        let result = Config::load("/nonexistent/path/config.toml");
        assert!(matches!(result, Err(ConfigError::IoError(_))));
    }
}
