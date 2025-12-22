use thiserror::Error;

/// Error types for the deps-lsp server.
///
/// All errors in the application are represented by this enum, which provides
/// structured error handling with source error tracking via `thiserror`.
///
/// # Examples
///
/// ```
/// use deps_lsp::error::{DepsError, Result};
///
/// fn parse_file(path: &str) -> Result<()> {
///     // Parsing errors are automatically wrapped
///     let _doc = "invalid toml".parse::<toml_edit::DocumentMut>()
///         .map_err(|e| DepsError::ParseError {
///             file_type: "Cargo.toml".into(),
///             source: e,
///         })?;
///     Ok(())
/// }
/// ```
#[derive(Error, Debug)]
pub enum DepsError {
    #[error("failed to parse {file_type}: {source}")]
    ParseError {
        file_type: String,
        #[source]
        source: toml_edit::TomlError,
    },

    #[error("registry request failed for {package}: {source}")]
    RegistryError {
        package: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("cache error: {0}")]
    CacheError(String),

    #[error("invalid version requirement: {0}")]
    InvalidVersionReq(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("semver parse error: {0}")]
    SemverParse(#[from] semver::Error),
}

/// Convenience type alias for `Result<T, DepsError>`.
///
/// This is the standard `Result` type used throughout the deps-lsp codebase.
/// It simplifies function signatures by defaulting the error type to `DepsError`.
///
/// # Examples
///
/// ```
/// use deps_lsp::error::Result;
///
/// fn get_version(name: &str) -> Result<String> {
///     if name.is_empty() {
///         return Err(deps_lsp::error::DepsError::CacheError("empty name".into()));
///     }
///     Ok("1.0.0".into())
/// }
/// ```
pub type Result<T> = std::result::Result<T, DepsError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = DepsError::CacheError("test error".into());
        assert_eq!(error.to_string(), "cache error: test error");
    }

    #[test]
    fn test_invalid_version_req() {
        let error = DepsError::InvalidVersionReq("invalid".into());
        assert_eq!(
            error.to_string(),
            "invalid version requirement: invalid"
        );
    }

    #[test]
    fn test_parse_error() {
        let toml_err = "invalid toml".parse::<toml_edit::DocumentMut>().unwrap_err();
        let error = DepsError::ParseError {
            file_type: "Cargo.toml".into(),
            source: toml_err,
        };
        assert!(error.to_string().contains("failed to parse Cargo.toml"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error: DepsError = io_err.into();
        assert!(error.to_string().contains("I/O error"));
    }
}
