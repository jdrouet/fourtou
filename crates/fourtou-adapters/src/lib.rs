//! Fourtou Adapters
//!
//! This crate provides adapter implementations for various source and export
//! protocols. Each adapter implements the traits defined in `fourtou-domain`.

pub mod exports;
pub mod sources;

pub use exports::AnyExporter;
pub use sources::AnySource;
