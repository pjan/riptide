//! Template error types.

use thiserror::Error;

/// Convenient result alias for template operations.
pub type TemplateResult<T> = Result<T, TemplateError>;

/// Errors produced while compiling or rendering templates.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// The template string was empty.
    #[error("template cannot be empty")]
    EmptyTemplate,

    /// A placeholder was not closed.
    #[error("unclosed placeholder at byte {position}")]
    UnclosedPlaceholder {
        /// Byte offset where the placeholder started.
        position: usize,
    },

    /// A placeholder had invalid syntax.
    #[error("invalid placeholder `{placeholder}`: {message}")]
    InvalidPlaceholder {
        /// Placeholder content.
        placeholder: String,
        /// Explanation.
        message: String,
    },

    /// A format specification is not supported for the target field.
    #[error("invalid format spec `{spec}` for `{field}`: {message}")]
    InvalidFormatSpec {
        /// Field being formatted.
        field: String,
        /// Format specification.
        spec: String,
        /// Explanation.
        message: String,
    },

    /// A template references data that was not provided.
    #[error("missing template context for `{namespace}`")]
    MissingContext {
        /// Missing namespace.
        namespace: &'static str,
    },

    /// A format specification was used with an incompatible value.
    #[error("template type mismatch for `{field}`: expected {expected}, got {actual}")]
    TypeMismatch {
        /// Field being formatted.
        field: String,
        /// Expected value kind.
        expected: &'static str,
        /// Actual value kind.
        actual: &'static str,
    },

    /// A fixture JSON document could not be decoded.
    #[error("invalid template fixture: {message}")]
    FixtureDecode {
        /// Explanation.
        message: String,
    },
}
