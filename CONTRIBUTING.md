# Contributing to Fourtou

Thank you for your interest in contributing to Fourtou! This document provides guidelines and instructions for contributing.

## Development Setup

### Prerequisites

- Rust 1.75 or later (for async fn in traits)
- Cargo

### Building

```bash
cargo build
```

### Running Tests

```bash
cargo test
```

### Code Coverage

Install `cargo-llvm-cov` and run:

```bash
cargo llvm-cov --html
```

The coverage report will be available in `target/llvm-cov/html/index.html`.

## Code Style

### Formatting

All code must be formatted with `rustfmt`:

```bash
cargo fmt
```

Check formatting without modifying files:

```bash
cargo fmt --check
```

### Linting

All code must pass `clippy` lints:

```bash
cargo clippy -- -D warnings
```

## Architecture

Fourtou follows hexagonal architecture principles:

```
crates/
├── fourtou/           # Binary - wires everything together
├── fourtou-domain/    # Core domain: entities, ports (traits), errors
├── fourtou-adapters/  # Adapter implementations (sources, exports)
├── fourtou-app/       # Application services (use cases)
└── fourtou-config/    # Configuration parsing
```

### Key Principles

1. **No `dyn` trait objects** - Use generics and enum dispatch instead
2. **Domain independence** - The domain crate has no external dependencies
3. **Ports and Adapters** - Domain defines traits (ports), adapters implement them
4. **Error handling** - Use `thiserror` for typed errors, `anyhow` for unexpected errors

### Design Principles

1. **Trait-based abstractions**: All major components have trait definitions to enable testing
2. **Minimize allocations**: Use buffer pools and avoid unnecessary heap allocations
3. **Testability**: Every module should be testable without network access
4. **Clear error handling**: Use `thiserror` for typed errors
5. **Debug formatting**: In error messages and logs, prefer `{:?}` over `{}` for interpolating values. This ensures consistent debug output, proper escaping of special characters, and avoids potential issues with Display implementations.

   ```rust
   // Good
   tracing::error!(name = ?source.name, "failed to load blocklist");
   return Err(ValidationError::InvalidUrl { url: url.clone() });
   
   // Avoid
   tracing::error!("failed to load blocklist: {}", source.name);
   ```

6. **String interpolation**: When interpolating values in format strings, use debug formatting (`{value:?}`) instead of wrapping with quotes (`'{value}'`). This ensures proper escaping if the value contains quotes or special characters.

   ```rust
   // Good
   format!("source not found: {source_id:?}")
   format!("invalid socket address for export {name:?}")
   
   // Avoid
   format!("source not found: '{source_id}'")
   format!("invalid socket address for export '{name}'")
   ```

7. **Error variable naming**: Never use single-letter variable names for errors. Use `err` or `error` instead of `e`. This improves readability and makes the code more searchable.

   ```rust
   // Good
   .map_err(|err| DomainError::ConnectionFailed { source: err })
   if let Err(error) = connection.close() { /* ... */ }
   
   // Forbidden
   .map_err(|e| DomainError::ConnectionFailed { source: e })
   if let Err(e) = connection.close() { /* ... */ }
   ```

### Error Message Formatting

When using `thiserror`, do **not** include `#[source]` errors in the `#[error]` message. This avoids duplication when the error chain is displayed.

**Wrong:**

```rust
#[derive(Error, Debug)]
pub enum MyError {
    #[error("failed to read config file: {0}")]
    IoError(#[from] std::io::Error),
}
```

**Correct:**

```rust
#[derive(Error, Debug)]
pub enum MyError {
    #[error("failed to read config file")]
    IoError(#[from] std::io::Error),
}
```

You may include additional context that is **not** derived from the source error:

```rust
#[derive(Error, Debug)]
pub enum MyError {
    #[error("connection failed to {url} with status {status}")]
    ConnectionFailed {
        url: String,
        status: u16,
        #[source]
        cause: reqwest::Error,
    },
}
```

### Adding a New Source

1. Create a new file in `crates/fourtou-adapters/src/sources/`
2. Implement the `SourceReader` trait from `fourtou-domain`
3. Add the variant to `AnySource` enum in `crates/fourtou-adapters/src/sources/mod.rs`
4. Add configuration support in `fourtou-config`
5. Write tests (target 80% coverage)

### Adding a New Export

1. Create a new file in `crates/fourtou-adapters/src/exports/`
2. Implement the `Exporter` trait from `fourtou-domain`
3. Add the variant to `AnyExporter` enum in `crates/fourtou-adapters/src/exports/mod.rs`
4. Add configuration support in `fourtou-config`
5. Write tests (target 80% coverage)

## Testing

### Test Coverage Target

We aim for 80% code coverage. All new code should include tests.

### Test Naming Convention

Test function names must follow the pattern `should_x_when_y`:

- `x` describes the expected behavior/outcome
- `y` describes the condition or context

**Examples:**

```rust
#[test]
fn should_return_file_entries_when_path_exists() { /* ... */ }

#[test]
fn should_return_error_when_source_not_found() { /* ... */ }

#[test]
fn should_parse_directories_when_href_ends_with_slash() { /* ... */ }
```

This naming convention makes tests self-documenting and clearly expresses the expected behavior.

### Test Organization

- **Unit tests**: In the same file as the code, under `#[cfg(test)] mod tests`
- **Integration tests**: In the `tests/` directory

### Test Doubles

Use manual test doubles defined in `test_support` modules rather than mocking frameworks:

```rust
#[cfg(test)]
pub mod test_support {
    pub struct InMemorySource { /* ... */ }
    impl SourceReader for InMemorySource { /* ... */ }
}
```

## Pull Request Process

1. Ensure all tests pass: `cargo test`
2. Ensure code is formatted: `cargo fmt --check`
3. Ensure clippy passes: `cargo clippy -- -D warnings`
4. Update documentation if needed
5. Add tests for new functionality
6. Create a pull request with a clear description

## Commit Messages

Use clear, descriptive commit messages:

- `feat: add pCloud source adapter`
- `fix: handle connection timeout in HTTP source`
- `docs: update README with new configuration options`
- `test: add integration tests for Samba export`
- `refactor: extract common HTTP parsing logic`

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
