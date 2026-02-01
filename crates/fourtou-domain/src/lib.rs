//! Fourtou Domain
//!
//! Core domain types, ports (traits), and errors for Fourtou.
//!
//! This crate defines the domain model and interfaces (ports) that adapters
//! must implement. It has no knowledge of specific implementations like HTTP,
//! S3, or Samba.

pub mod entities;
pub mod errors;
pub mod ports;

pub use entities::{FileEntry, FileMetadata, FileStream, FileType, SourceId};
pub use errors::DomainError;
pub use ports::{Exporter, FileAggregator, SourceReader};
