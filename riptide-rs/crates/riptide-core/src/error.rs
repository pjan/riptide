//! Structured error types for riptide-core.

use crate::{ProviderId, ResourceKind};
use thiserror::Error;

/// Convenient result alias for core operations.
pub type CoreResult<T> = Result<T, CoreError>;

/// Errors produced by provider-neutral core types and registries.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// A newtype rejected an empty string.
    #[error("{type_name} cannot be empty")]
    EmptyValue {
        /// The type whose constructor rejected the value.
        type_name: &'static str,
    },

    /// A newtype rejected zero where the domain requires a non-zero value.
    #[error("{type_name} must be greater than zero")]
    ZeroValue {
        /// The type whose constructor rejected the value.
        type_name: &'static str,
    },

    /// A provider registry lookup failed.
    #[error("provider `{provider}` is not registered")]
    MissingProvider {
        /// The provider that was requested.
        provider: ProviderId,
    },

    /// A provider registry insertion used an ID already present.
    #[error("provider `{provider}` is already registered")]
    DuplicateProvider {
        /// The duplicated provider ID.
        provider: ProviderId,
    },

    /// A provider does not implement the requested capability.
    #[error("provider `{provider}` does not support {capability}")]
    UnsupportedCapability {
        /// The provider that rejected the capability.
        provider: ProviderId,
        /// The unsupported capability.
        capability: &'static str,
    },

    /// A provider cannot handle a resource kind for a requested operation.
    #[error("provider `{provider}` does not support {kind:?} resources for {operation}")]
    UnsupportedResourceKind {
        /// The provider that rejected the resource.
        provider: ProviderId,
        /// The resource kind that was rejected.
        kind: ResourceKind,
        /// The operation being attempted.
        operation: &'static str,
    },
}
