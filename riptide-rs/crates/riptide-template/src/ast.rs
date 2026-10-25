//! Template abstract syntax tree types.

use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt};

/// A compiled template node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateNode {
    /// Literal text.
    Literal(String),
    /// Placeholder without a dotted field, such as `{now}` or `{type}`.
    NamespaceValue {
        /// Value namespace.
        namespace: ValueNamespace,
        /// Optional typed format spec.
        format: Option<FormatSpec>,
    },
    /// Placeholder with a namespace and field, such as `{item.number}`.
    Field {
        /// Field path.
        field: TemplateField,
        /// Optional typed format spec.
        format: Option<FormatSpec>,
    },
}

/// One path segment compiled into nodes.
pub type PathSegment = Vec<TemplateNode>;

/// A dotted template field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateField {
    /// Namespace.
    pub namespace: TemplateNamespace,
    /// Field.
    pub field: FieldSegment,
}

/// Single field segment.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FieldSegment(String);

impl FieldSegment {
    /// Creates a non-empty field segment.
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            None
        } else {
            Some(Self(value))
        }
    }

    /// Returns the field name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for FieldSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("FieldSegment").field(&self.0).finish()
    }
}

/// Top-level extra field name.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ExtraFieldName(String);

impl ExtraFieldName {
    /// Creates a non-empty extra field name.
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            None
        } else {
            Some(Self(value))
        }
    }

    /// Returns the field name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ExtraFieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ExtraFieldName").field(&self.0).finish()
    }
}

/// Dotted field namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateNamespace {
    /// Item fields.
    Item,
    /// Album fields.
    Album,
    /// Playlist fields.
    Playlist,
}

/// Standalone value namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueNamespace {
    /// Current timestamp.
    Now,
    /// Top-level extra field.
    Extra(ExtraFieldName),
}

/// Parsed format specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatSpec {
    /// Integer padding.
    IntPad {
        /// Minimum width.
        width: u8,
        /// Fill mode.
        fill: IntFill,
    },
    /// Date or datetime format.
    DateTime(ParsedDateTimeFormat),
    /// Explicit content formatting.
    Explicit(ExplicitFormat),
    /// Conditional text for boolean flags.
    ConditionalText(String),
}

/// Parsed datetime format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDateTimeFormat {
    /// Original user-provided format string.
    pub raw: String,
    /// Parsed owned format items.
    pub item: time::format_description::OwnedFormatItem,
}

/// Integer fill mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntFill {
    /// Fill with zeroes.
    Zero,
    /// Fill with spaces.
    Space,
}

/// Explicit base mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplicitMode {
    /// `E` for explicit.
    Short,
    /// `explicit` for explicit.
    Long,
    /// `explicit` or `clean`.
    Full,
}

/// Explicit format flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplicitFormat {
    /// Base mode.
    pub mode: ExplicitMode,
    /// Uppercase output.
    pub upper: bool,
}

/// Runtime template value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateValue<'a> {
    /// Text value.
    Text(Cow<'a, str>),
    /// Integer value.
    Integer(i64),
    /// Explicit flag.
    Explicit(Option<bool>),
    /// Conditional flag.
    Conditional(bool),
    /// Date value.
    Date(time::Date),
    /// Datetime value.
    DateTime(time::OffsetDateTime),
    /// Structurally absent field.
    None,
}

impl TemplateValue<'_> {
    /// Returns a short kind label for diagnostics.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Integer(_) => "integer",
            Self::Explicit(_) => "explicit",
            Self::Conditional(_) => "conditional",
            Self::Date(_) => "date",
            Self::DateTime(_) => "datetime",
            Self::None => "none",
        }
    }
}
