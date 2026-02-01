//! Fourtou Application
//!
//! This crate contains application-level services and use cases for Fourtou.
//! It orchestrates domain operations and provides higher-level abstractions
//! for the binary crate.

pub mod errors;
pub mod services;

pub use errors::AppError;
pub use services::{AggregatedFile, FileAggregatorService};
