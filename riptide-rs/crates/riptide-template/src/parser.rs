//! Template parser.

use crate::{
    ExplicitFormat, ExplicitMode, ExtraFieldName, FieldSegment, FormatSpec, IntFill,
    ParsedDateTimeFormat, PathSegment, TemplateError, TemplateField, TemplateNamespace,
    TemplateNode, TemplateResult, ValueNamespace,
};

/// Parsed template ready for repeated rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledTemplate {
    segments: Vec<PathSegment>,
}

impl CompiledTemplate {
    /// Parses and validates a template.
    pub fn parse(template: &str) -> TemplateResult<Self> {
        if template.is_empty() {
            return Err(TemplateError::EmptyTemplate);
        }

        let segments = template
            .split('/')
            .map(parse_segment)
            .collect::<TemplateResult<Vec<_>>>()?;
        Ok(Self { segments })
    }

    pub(crate) fn segments(&self) -> &[PathSegment] {
        &self.segments
    }
}

fn parse_segment(segment: &str) -> TemplateResult<PathSegment> {
    let mut nodes = Vec::new();
    let mut rest = segment;
    let mut offset = 0usize;

    while let Some(start) = rest.find('{') {
        if start > 0 {
            nodes.push(TemplateNode::Literal(rest[..start].to_owned()));
        }
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('}') else {
            return Err(TemplateError::UnclosedPlaceholder {
                position: offset + start,
            });
        };
        let placeholder = &after_start[..end];
        nodes.push(parse_placeholder(placeholder)?);
        let consumed = start + 1 + end + 1;
        offset += consumed;
        rest = &rest[consumed..];
    }

    if !rest.is_empty() {
        nodes.push(TemplateNode::Literal(rest.to_owned()));
    }
    Ok(nodes)
}

fn parse_placeholder(placeholder: &str) -> TemplateResult<TemplateNode> {
    let (path, spec) = match placeholder.split_once(':') {
        Some((path, spec)) => (path, Some(spec)),
        None => (placeholder, None),
    };
    if path.trim().is_empty() {
        return Err(TemplateError::InvalidPlaceholder {
            placeholder: placeholder.to_owned(),
            message: "field cannot be empty".to_owned(),
        });
    }

    if let Some((namespace, field)) = path.split_once('.') {
        let namespace = parse_namespace(namespace, placeholder)?;
        let Some(field) = FieldSegment::new(field) else {
            return Err(TemplateError::InvalidPlaceholder {
                placeholder: placeholder.to_owned(),
                message: "field cannot be empty".to_owned(),
            });
        };
        validate_field(namespace, field.as_str(), placeholder)?;
        let format = parse_format_for_field(namespace, field.as_str(), spec)?;
        return Ok(TemplateNode::Field {
            field: TemplateField { namespace, field },
            format,
        });
    }

    let namespace = if path == "now" {
        ValueNamespace::Now
    } else {
        let Some(name) = ExtraFieldName::new(path) else {
            return Err(TemplateError::InvalidPlaceholder {
                placeholder: placeholder.to_owned(),
                message: "extra name cannot be empty".to_owned(),
            });
        };
        ValueNamespace::Extra(name)
    };
    let format = match &namespace {
        ValueNamespace::Now => parse_datetime_format("now", spec)?,
        ValueNamespace::Extra(_) => {
            if let Some(spec) = spec {
                return Err(TemplateError::InvalidFormatSpec {
                    field: path.to_owned(),
                    spec: spec.to_owned(),
                    message: "extra fields do not support format specs".to_owned(),
                });
            }
            None
        }
    };
    Ok(TemplateNode::NamespaceValue { namespace, format })
}

fn parse_namespace(namespace: &str, placeholder: &str) -> TemplateResult<TemplateNamespace> {
    match namespace {
        "item" => Ok(TemplateNamespace::Item),
        "album" => Ok(TemplateNamespace::Album),
        "playlist" => Ok(TemplateNamespace::Playlist),
        _ => Err(TemplateError::InvalidPlaceholder {
            placeholder: placeholder.to_owned(),
            message: "unknown namespace".to_owned(),
        }),
    }
}

fn validate_field(
    namespace: TemplateNamespace,
    field: &str,
    placeholder: &str,
) -> TemplateResult<()> {
    let valid = match namespace {
        TemplateNamespace::Item => matches!(
            field,
            "id" | "title"
                | "title_version"
                | "number"
                | "volume"
                | "version"
                | "copyright"
                | "bpm"
                | "isrc"
                | "quality"
                | "artist"
                | "artists"
                | "features"
                | "artists_with_features"
                | "explicit"
                | "dolby"
        ),
        TemplateNamespace::Album => matches!(
            field,
            "id" | "title" | "artist" | "artists" | "date" | "explicit" | "master" | "release"
        ),
        TemplateNamespace::Playlist => {
            matches!(field, "uuid" | "title" | "index" | "created" | "updated")
        }
    };

    if valid {
        Ok(())
    } else {
        Err(TemplateError::InvalidPlaceholder {
            placeholder: placeholder.to_owned(),
            message: "unknown field".to_owned(),
        })
    }
}

fn parse_format_for_field(
    namespace: TemplateNamespace,
    field: &str,
    spec: Option<&str>,
) -> TemplateResult<Option<FormatSpec>> {
    match (namespace, field) {
        (TemplateNamespace::Item, "number" | "volume" | "bpm")
        | (TemplateNamespace::Playlist, "index") => parse_int_format(field, spec),
        (TemplateNamespace::Album, "date")
        | (TemplateNamespace::Playlist, "created" | "updated") => {
            parse_datetime_format(field, spec)
        }
        (TemplateNamespace::Item | TemplateNamespace::Album, "explicit") => {
            parse_explicit_format(field, spec)
        }
        (TemplateNamespace::Item, "dolby") | (TemplateNamespace::Album, "master") => {
            Ok(spec.map(|spec| FormatSpec::ConditionalText(spec.to_owned())))
        }
        _ => {
            if let Some(spec) = spec {
                return Err(TemplateError::InvalidFormatSpec {
                    field: field.to_owned(),
                    spec: spec.to_owned(),
                    message: "field does not support format specs".to_owned(),
                });
            }
            Ok(None)
        }
    }
}

fn parse_int_format(field: &str, spec: Option<&str>) -> TemplateResult<Option<FormatSpec>> {
    let Some(spec) = spec else {
        return Ok(None);
    };
    let Some(width_part) = spec.strip_suffix('d') else {
        return Err(invalid_spec(
            field,
            spec,
            "integer format must end with `d`",
        ));
    };
    let (fill, width) = width_part.strip_prefix('0').map_or_else(
        || (IntFill::Space, width_part),
        |width| (IntFill::Zero, width),
    );
    if width.is_empty() || !width.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_spec(field, spec, "integer width must be numeric"));
    }
    let width = width
        .parse::<u8>()
        .map_err(|_| invalid_spec(field, spec, "integer width is too large"))?;
    Ok(Some(FormatSpec::IntPad { width, fill }))
}

fn parse_datetime_format(field: &str, spec: Option<&str>) -> TemplateResult<Option<FormatSpec>> {
    let Some(spec) = spec else {
        return Ok(None);
    };
    let description = strftime_to_time_description(spec)
        .ok_or_else(|| invalid_spec(field, spec, "unsupported datetime format"))?;
    let item = time::format_description::parse_owned::<2>(&description)
        .map_err(|error| invalid_spec(field, spec, &error.to_string()))?;
    Ok(Some(FormatSpec::DateTime(ParsedDateTimeFormat {
        raw: spec.to_owned(),
        item,
    })))
}

fn parse_explicit_format(field: &str, spec: Option<&str>) -> TemplateResult<Option<FormatSpec>> {
    let Some(spec) = spec else {
        return Ok(Some(FormatSpec::Explicit(ExplicitFormat {
            mode: ExplicitMode::Short,
            upper: false,
        })));
    };

    let mut mode = None;
    let mut upper = false;
    for feature in spec.split(',') {
        match feature {
            "long" => set_mode(field, spec, &mut mode, ExplicitMode::Long)?,
            "full" => set_mode(field, spec, &mut mode, ExplicitMode::Full)?,
            "upper" => upper = true,
            _ => return Err(invalid_spec(field, spec, "unknown explicit format feature")),
        }
    }
    let mode = mode.unwrap_or(if upper {
        ExplicitMode::Long
    } else {
        ExplicitMode::Short
    });
    Ok(Some(FormatSpec::Explicit(ExplicitFormat { mode, upper })))
}

fn set_mode(
    field: &str,
    spec: &str,
    current: &mut Option<ExplicitMode>,
    next: ExplicitMode,
) -> TemplateResult<()> {
    if current.is_some() {
        return Err(invalid_spec(
            field,
            spec,
            "only one explicit mode feature is allowed",
        ));
    }
    *current = Some(next);
    Ok(())
}

fn invalid_spec(field: &str, spec: &str, message: &str) -> TemplateError {
    TemplateError::InvalidFormatSpec {
        field: field.to_owned(),
        spec: spec.to_owned(),
        message: message.to_owned(),
    }
}

fn strftime_to_time_description(spec: &str) -> Option<String> {
    let mut output = String::new();
    let mut chars = spec.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }
        let directive = chars.next()?;
        match directive {
            'Y' => output.push_str("[year]"),
            'm' => output.push_str("[month]"),
            'd' => output.push_str("[day]"),
            'H' => output.push_str("[hour]"),
            'M' => output.push_str("[minute]"),
            'S' => output.push_str("[second]"),
            '%' => output.push('%'),
            _ => return None,
        }
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_template_fails() {
        assert!(matches!(
            CompiledTemplate::parse(""),
            Err(TemplateError::EmptyTemplate)
        ));
    }

    #[test]
    fn parses_integer_padding() -> TemplateResult<()> {
        let template = CompiledTemplate::parse("{item.number:02d}")?;
        assert_eq!(template.segments().len(), 1);
        Ok(())
    }

    #[test]
    fn invalid_datetime_format_fails_during_parse() {
        assert!(matches!(
            CompiledTemplate::parse("{album.date:%Q}"),
            Err(TemplateError::InvalidFormatSpec { .. })
        ));
    }

    #[test]
    fn explicit_upper_alone_uses_long_mode() -> TemplateResult<()> {
        let template = CompiledTemplate::parse("{item.explicit:upper}")?;
        let TemplateNode::Field {
            format: Some(FormatSpec::Explicit(format)),
            ..
        } = &template.segments()[0][0]
        else {
            return Err(TemplateError::InvalidPlaceholder {
                placeholder: "item.explicit".to_owned(),
                message: "unexpected node".to_owned(),
            });
        };
        assert_eq!(format.mode, ExplicitMode::Long);
        assert!(format.upper);
        Ok(())
    }

    #[test]
    fn conflicting_explicit_modes_fail() {
        assert!(matches!(
            CompiledTemplate::parse("{item.explicit:long,full}"),
            Err(TemplateError::InvalidFormatSpec { .. })
        ));
    }
}
