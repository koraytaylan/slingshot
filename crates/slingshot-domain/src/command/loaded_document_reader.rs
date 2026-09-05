//! Incremental closed loaded-resource validation; completed subtrees are dropped.

use super::canonical_json_reader::{Bounds, Reader, Refusal};
use super::load_content_as_javascript_object_notation::{
    DECLARED_PROPERTY_TYPES, LoadContentAsJavaScriptObjectNotationCommand,
    maximum_load_document_bytes, read_scalar,
};
use super::repository_path::{RepositoryName, RepositoryPath};
use std::io::Read;

/// Checks canonical field order, closed document/property shapes, exact typed
/// scalar values and request-relative tree semantics without collecting child
/// or property-value arrays. Only the current scalar and ancestor path/order
/// state are retained. The source must separately prove artifact length/digest.
pub fn require_loaded_document_reader(
    source: impl Read,
    command: &LoadContentAsJavaScriptObjectNotationCommand,
) -> Result<(), Refusal> {
    let maximum = maximum_load_document_bytes();
    let mut reader = Reader::new(
        source,
        Bounds {
            bytes: maximum,
            token_bytes: usize::try_from(maximum).map_err(|_| Refusal)?,
            depth: 128,
        },
    );
    let path = resource(&mut reader, 0, command.resolved_depth().edges())?;
    if path != command.path || reader.peek()?.is_some() {
        return Err(Refusal);
    }
    Ok(())
}

fn field<R: Read>(reader: &mut Reader<R>, name: &str) -> Result<(), Refusal> {
    if reader.string()? != name {
        return Err(Refusal);
    }
    reader.expect(b':')
}

fn boolean<R: Read>(reader: &mut Reader<R>) -> Result<bool, Refusal> {
    let (spelling, value): (&[u8], bool) = match reader.peek()? {
        Some(b't') => (b"true", true),
        Some(b'f') => (b"false", false),
        _ => return Err(Refusal),
    };
    for byte in spelling {
        reader.expect(*byte)?;
    }
    Ok(value)
}

fn resource<R: Read>(
    reader: &mut Reader<R>,
    depth: u64,
    maximum: u64,
) -> Result<RepositoryPath, Refusal> {
    reader.expect(b'{')?;
    field(reader, "children")?;
    reader.expect(b'[')?;
    let mut parent = None;
    let mut previous = None;
    if reader.peek()? != Some(b']') {
        if depth >= maximum {
            return Err(Refusal);
        }
        loop {
            let child = resource(reader, depth + 1, maximum)?;
            let owner = child.parent().ok_or(Refusal)?;
            if parent.as_ref().is_some_and(|prior| prior != &owner) {
                return Err(Refusal);
            }
            parent = Some(owner);
            let segments = child.segments();
            let segment = segments.last().ok_or(Refusal)?;
            let index = match segment.as_text().rsplit_once('[') {
                Some((_, suffix)) => {
                    suffix.strip_suffix(']').and_then(|n| n.parse::<u64>().ok()).ok_or(Refusal)?
                }
                None => 1,
            };
            let key = (segment.name().as_text().to_owned(), index);
            if previous.as_ref().is_some_and(|prior| prior >= &key) {
                return Err(Refusal);
            }
            previous = Some(key);
            if reader.peek()? == Some(b']') {
                break;
            }
            reader.expect(b',')?;
        }
    }
    reader.expect(b']')?;
    reader.expect(b',')?;
    field(reader, "children_truncated")?;
    if boolean(reader)? && depth < maximum {
        return Err(Refusal);
    }
    reader.expect(b',')?;
    field(reader, "path")?;
    let path = RepositoryPath::parse(&reader.string()?).map_err(|_| Refusal)?;
    if parent.as_ref().is_some_and(|owner| owner != &path) {
        return Err(Refusal);
    }
    reader.expect(b',')?;
    field(reader, "properties")?;
    properties(reader)?;
    reader.expect(b'}')?;
    Ok(path)
}

fn properties<R: Read>(reader: &mut Reader<R>) -> Result<(), Refusal> {
    reader.expect(b'{')?;
    let mut previous: Option<String> = None;
    if reader.peek()? != Some(b'}') {
        loop {
            let name = reader.string()?;
            RepositoryName::parse(&name).map_err(|_| Refusal)?;
            if previous.as_ref().is_some_and(|prior| prior.as_bytes() >= name.as_bytes()) {
                return Err(Refusal);
            }
            previous = Some(name);
            reader.expect(b':')?;
            property(reader)?;
            if reader.peek()? == Some(b'}') {
                break;
            }
            reader.expect(b',')?;
        }
    }
    reader.expect(b'}')
}

fn property<R: Read>(reader: &mut Reader<R>) -> Result<(), Refusal> {
    reader.expect(b'{')?;
    field(reader, "cardinality")?;
    let cardinality = reader.string()?;
    if cardinality != "single" && cardinality != "multiple" {
        return Err(Refusal);
    }
    reader.expect(b',')?;
    field(reader, "property_type")?;
    let property_type = reader.string()?;
    if !DECLARED_PROPERTY_TYPES.contains(&property_type.as_str()) {
        return Err(Refusal);
    }
    reader.expect(b',')?;
    if cardinality == "single" {
        field(reader, "value")?;
        scalar(reader, &property_type)?;
    } else {
        field(reader, "values")?;
        reader.expect(b'[')?;
        if reader.peek()? != Some(b']') {
            loop {
                scalar(reader, &property_type)?;
                if reader.peek()? == Some(b']') {
                    break;
                }
                reader.expect(b',')?;
            }
        }
        reader.expect(b']')?;
    }
    reader.expect(b'}')
}

fn scalar<R: Read>(reader: &mut Reader<R>, property_type: &str) -> Result<(), Refusal> {
    let value = match reader.peek()? {
        Some(b'"') => serde_json::Value::String(reader.string()?),
        Some(b't' | b'f') => serde_json::Value::Bool(boolean(reader)?),
        Some(b'{') if property_type == "binary" => {
            reader.expect(b'{')?;
            field(reader, "byte_length")?;
            let length = reader.string()?;
            reader.expect(b'}')?;
            serde_json::json!({"byte_length":length})
        }
        _ => return Err(Refusal),
    };
    read_scalar(property_type, value).map_err(|_| Refusal)?;
    Ok(())
}
