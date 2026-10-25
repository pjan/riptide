//! Compiled path template parsing and rendering.

mod ast;
mod context;
mod error;
mod parser;
mod render;
mod sanitize;

pub use ast::{
    ExplicitFormat, ExplicitMode, ExtraFieldName, FieldSegment, FormatSpec, IntFill,
    ParsedDateTimeFormat, PathSegment, TemplateField, TemplateNamespace, TemplateNode,
    TemplateValue, ValueNamespace,
};
pub use context::{
    context_from_fixture_str, AlbumTemplateInput, FixtureContext, ItemTemplateInput, PlaylistIndex,
    PlaylistTemplateInput, TemplateContext,
};
pub use error::{TemplateError, TemplateResult};
pub use parser::CompiledTemplate;
pub use render::{RenderedPath, TemplateRenderOptions};
pub use sanitize::clean_segment;
