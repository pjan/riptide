//! Template rendering.

use crate::{
    clean_segment, CompiledTemplate, ExplicitMode, FormatSpec, IntFill, TemplateContext,
    TemplateError, TemplateField, TemplateNamespace, TemplateNode, TemplateResult, TemplateValue,
    ValueNamespace,
};
use std::borrow::Cow;
use time::format_description::well_known::Rfc3339;

/// Render options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateRenderOptions {
    /// Append `.*` wildcard extension.
    pub with_asterisk_ext: bool,
}

impl Default for TemplateRenderOptions {
    fn default() -> Self {
        Self {
            with_asterisk_ext: true,
        }
    }
}

/// Rendered relative path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPath(String);

impl RenderedPath {
    /// Returns the rendered path.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl CompiledTemplate {
    /// Renders the template using default options.
    pub fn render(&self, context: &TemplateContext) -> TemplateResult<RenderedPath> {
        self.render_with_options(context, TemplateRenderOptions::default())
    }

    /// Renders the template using explicit options.
    pub fn render_with_options(
        &self,
        context: &TemplateContext,
        options: TemplateRenderOptions,
    ) -> TemplateResult<RenderedPath> {
        let mut segments = Vec::with_capacity(self.segments().len());
        for segment in self.segments() {
            let mut rendered = String::new();
            for node in segment {
                rendered.push_str(&render_node(node, context)?);
            }
            segments.push(clean_segment(&rendered));
        }
        let mut path = segments.join("/");
        if options.with_asterisk_ext {
            path.push_str(".*");
        }
        Ok(RenderedPath(path))
    }
}

fn render_node(node: &TemplateNode, context: &TemplateContext) -> TemplateResult<String> {
    match node {
        TemplateNode::Literal(value) => Ok(value.clone()),
        TemplateNode::NamespaceValue { namespace, format } => {
            let (field, value) = match namespace {
                ValueNamespace::Now => ("now".to_owned(), TemplateValue::DateTime(context.now())),
                ValueNamespace::Extra(name) => (name.as_str().to_owned(), context.extra(name)),
            };
            render_value(&field, value, format.as_ref())
        }
        TemplateNode::Field { field, format } => {
            let value = resolve_field(field, context)?;
            render_value(
                &format!(
                    "{}.{}",
                    namespace_name(field.namespace),
                    field.field.as_str()
                ),
                value,
                format.as_ref(),
            )
        }
    }
}

fn resolve_field<'a>(
    field: &TemplateField,
    context: &'a TemplateContext,
) -> TemplateResult<TemplateValue<'a>> {
    match field.namespace {
        TemplateNamespace::Item => context
            .item()
            .map(|item| item.field(field.field.as_str()))
            .ok_or(TemplateError::MissingContext { namespace: "item" }),
        TemplateNamespace::Album => Ok(context
            .album()
            .map_or(TemplateValue::None, |album| album.field(field.field.as_str(), context.effective_quality()))),
        TemplateNamespace::Playlist => Ok(context
            .playlist()
            .map_or(TemplateValue::None, |playlist| playlist.field(field.field.as_str()))),
    }
}

fn render_value(
    field: &str,
    value: TemplateValue<'_>,
    format: Option<&FormatSpec>,
) -> TemplateResult<String> {
    match format {
        Some(FormatSpec::IntPad { width, fill }) => render_int(field, &value, *width, *fill),
        Some(FormatSpec::DateTime(format)) => render_datetime(field, &value, format),
        Some(FormatSpec::Explicit(format)) => render_explicit(&value, *format),
        Some(FormatSpec::ConditionalText(text)) => render_conditional(field, &value, text),
        None => render_default(field, value),
    }
}

fn render_default(field: &str, value: TemplateValue<'_>) -> TemplateResult<String> {
    match value {
        TemplateValue::Text(value) => Ok(value.into_owned()),
        TemplateValue::Integer(value) => Ok(value.to_string()),
        TemplateValue::Explicit(value) => render_explicit(
            &TemplateValue::Explicit(value),
            crate::ExplicitFormat {
                mode: ExplicitMode::Short,
                upper: false,
            },
        ),
        TemplateValue::Conditional(value) => Ok(if value { "true" } else { "" }.to_owned()),
        TemplateValue::Date(value) => Ok(value.to_string()),
        TemplateValue::DateTime(value) => {
            value
                .format(&Rfc3339)
                .map_err(|error| TemplateError::InvalidFormatSpec {
                    field: field.to_owned(),
                    spec: "rfc3339".to_owned(),
                    message: error.to_string(),
                })
        }
        TemplateValue::None => Ok(String::new()),
    }
}

fn render_int(
    field: &str,
    value: &TemplateValue<'_>,
    width: u8,
    fill: IntFill,
) -> TemplateResult<String> {
    let TemplateValue::Integer(value) = *value else {
        return Err(type_mismatch(field, "integer", value));
    };
    let width = usize::from(width);
    let rendered = match fill {
        IntFill::Zero => format!("{value:0width$}"),
        IntFill::Space => format!("{value:width$}"),
    };
    Ok(rendered)
}

fn render_datetime(
    field: &str,
    value: &TemplateValue<'_>,
    format: &crate::ParsedDateTimeFormat,
) -> TemplateResult<String> {
    match *value {
        TemplateValue::Date(value) => {
            value
                .format(&format.item)
                .map_err(|error| TemplateError::InvalidFormatSpec {
                    field: field.to_owned(),
                    spec: format.raw.clone(),
                    message: error.to_string(),
                })
        }
        TemplateValue::DateTime(value) => {
            value
                .format(&format.item)
                .map_err(|error| TemplateError::InvalidFormatSpec {
                    field: field.to_owned(),
                    spec: format.raw.clone(),
                    message: error.to_string(),
                })
        }
        ref other => Err(type_mismatch(field, "date or datetime", other)),
    }
}

fn render_explicit(
    value: &TemplateValue<'_>,
    format: crate::ExplicitFormat,
) -> TemplateResult<String> {
    let TemplateValue::Explicit(value) = *value else {
        return Err(type_mismatch("explicit", "explicit", value));
    };
    let mut output = match (value, format.mode) {
        (None, _) | (Some(false), ExplicitMode::Short | ExplicitMode::Long) => "",
        (Some(true), ExplicitMode::Short) => "E",
        (Some(true), ExplicitMode::Long | ExplicitMode::Full) => "explicit",
        (Some(false), ExplicitMode::Full) => "clean",
    }
    .to_owned();
    if format.upper {
        output = output.to_uppercase();
    }
    Ok(output)
}

fn render_conditional(field: &str, value: &TemplateValue<'_>, text: &str) -> TemplateResult<String> {
    let TemplateValue::Conditional(value) = *value else {
        return Err(type_mismatch(field, "conditional", value));
    };
    Ok(if value { text } else { "" }.to_owned())
}

fn type_mismatch(field: &str, expected: &'static str, value: &TemplateValue<'_>) -> TemplateError {
    TemplateError::TypeMismatch {
        field: field.to_owned(),
        expected,
        actual: value.kind(),
    }
}

const fn namespace_name(namespace: TemplateNamespace) -> &'static str {
    match namespace {
        TemplateNamespace::Item => "item",
        TemplateNamespace::Album => "album",
        TemplateNamespace::Playlist => "playlist",
    }
}

impl<'a> From<&'a str> for TemplateValue<'a> {
    fn from(value: &'a str) -> Self {
        Self::Text(Cow::Borrowed(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AlbumTemplateInput, ItemTemplateInput};

    fn context() -> TemplateContext {
        let item = ItemTemplateInput {
            kind: "track".to_owned(),
            id: "1".to_owned(),
            title: "Song".to_owned(),
            number: 2,
            volume: 1,
            version: "Live".to_owned(),
            copyright: String::new(),
            bpm: 0,
            isrc: String::new(),
            quality: "LOSSLESS".to_owned(),
            artist: "Artist".to_owned(),
            artists: "Artist".to_owned(),
            features: String::new(),
            artists_with_features: "Artist".to_owned(),
            explicit: Some(true),
            dolby: true,
        };
        let album = AlbumTemplateInput {
            id: "2".to_owned(),
            title: "Album".to_owned(),
            artist: "Artist".to_owned(),
            artists: "Artist".to_owned(),
            date: Some(
                time::Date::from_calendar_date(2024, time::Month::January, 1)
                    .unwrap_or(time::Date::MIN),
            ),
            explicit: Some(false),
            master: true,
            release: "ALBUM".to_owned(),
        };
        TemplateContext::new(time::OffsetDateTime::UNIX_EPOCH)
            .with_item_input(item)
            .with_album_input(album)
            .with_effective_quality_label(riptide_core::ProviderQualityLabel::new("MAX").ok())
    }

    #[test]
    fn renders_integer_padding() -> TemplateResult<()> {
        let template = CompiledTemplate::parse("{item.number:02d} - {item.title}")?;
        let rendered = template.render_with_options(
            &context(),
            TemplateRenderOptions {
                with_asterisk_ext: false,
            },
        )?;
        assert_eq!(rendered.as_str(), "02 - Song");
        Ok(())
    }

    #[test]
    fn renders_explicit_upper() -> TemplateResult<()> {
        let template = CompiledTemplate::parse("{item.explicit:upper}")?;
        let rendered = template.render_with_options(
            &context(),
            TemplateRenderOptions {
                with_asterisk_ext: false,
            },
        )?;
        assert_eq!(rendered.as_str(), "EXPLICIT");
        Ok(())
    }

    #[test]
    fn computes_title_version() -> TemplateResult<()> {
        let template = CompiledTemplate::parse("{item.title_version}")?;
        let rendered = template.render_with_options(
            &context(),
            TemplateRenderOptions {
                with_asterisk_ext: false,
            },
        )?;
        assert_eq!(rendered.as_str(), "Song (Live)");
        Ok(())
    }

    #[test]
    fn missing_album_fields_render_empty() -> TemplateResult<()> {
        let template = CompiledTemplate::parse("{album.title}")?;
        let rendered = template.render_with_options(
            &TemplateContext::new(time::OffsetDateTime::UNIX_EPOCH),
            TemplateRenderOptions {
                with_asterisk_ext: false,
            },
        )?;
        assert_eq!(rendered.as_str(), "_");
        Ok(())
    }
}
