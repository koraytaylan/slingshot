//! Finding assets by metadata, or by the page that references them.
//!
//! Two commands, sharing the predicate grammar and the window rule with the
//! page and path searches. What is particular here is the set-valued filters:
//! media formats and tags are sets on the wire, canonical and ascending,
//! because the set participates in the digest a continuation token is bound to
//! and two spellings of one set would be two queries that could not resume each
//! other.
//!
//! # A set is sorted once and a duplicate is refused
//!
//! Every accepted permutation of the same values produces the same request, so
//! a caller may type them in any order and the set is sorted here, once, before
//! the domain sees it. A repeat is not the same request, though: collapsing it
//! would accept a set the caller did not describe, so the domain refuses it and
//! nothing here deduplicates first.
//!
//! # A range is checked as two values before it is checked as a range
//!
//! A minimum that is not a length and a maximum that is not a length are
//! separate mistakes from a minimum above a maximum, and telling them apart is
//! what lets a caller fix the right one.

use slingshot_domain::command::catalog::Command;
use slingshot_domain::command::find_assets_by_metadata::{
    AssetByteLength, AssetTag, FindAssetsByMetadataCommand, MediaFormat, RequestedAssetTags,
    RequestedMediaFormats, TagMatchMode,
};
use slingshot_domain::command::find_assets_referenced_by_page::FindAssetsReferencedByPageCommand;
use slingshot_domain::command::repository_path::RepositoryPath;

use crate::commands::content::{RequestRefusal, require_key, required};
use crate::commands::package::LIST_SEPARATOR;
use crate::commands::path_query::{predicates, window};
use crate::invocation::{
    Invocation, MATCH_ALL_OPTION, MAXIMUM_BYTES_OPTION, MEDIA_FORMATS_OPTION, MINIMUM_BYTES_OPTION,
    PATH_OPTION, TAGS_OPTION,
};

/// The wire name of the metadata search.
pub const FIND_BY_METADATA: &str = "find_assets_by_metadata";

/// The wire name of the page-reference search.
pub const FIND_REFERENCED_BY_PAGE: &str = "find_assets_referenced_by_page";

/// Every command this family builds.
const NAMES: &[&str] = &[FIND_BY_METADATA, FIND_REFERENCED_BY_PAGE];

/// Returns the typed request one invocation describes.
///
/// # Errors
///
/// Returns [`RequestRefusal`] naming the first thing that is wrong.
pub fn build(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    // The verb is answered before anything else is asked, including the
    // key rule: a family that refuses another family's command for any
    // reason but "another command" stops the assembler on it.
    if !NAMES.contains(&invocation.verb.as_str()) {
        return Err(RequestRefusal::AnotherCommand { named: invocation.verb.clone() });
    }
    require_key(invocation)?;
    match invocation.verb.as_str() {
        FIND_BY_METADATA => build_metadata(invocation),
        FIND_REFERENCED_BY_PAGE => {
            let page_path = RepositoryPath::parse(required(invocation, PATH_OPTION)?)
                .map_err(|_| RequestRefusal::ValueUnusable { named: PATH_OPTION.to_owned() })?;
            Ok(Command::FindAssetsReferencedByPage(FindAssetsReferencedByPageCommand {
                page_path,
                result_window: window(invocation)?,
            }))
        }
        named => Err(RequestRefusal::AnotherCommand { named: named.to_owned() }),
    }
}

/// Returns the metadata search one invocation describes.
fn build_metadata(invocation: &Invocation) -> Result<Command, RequestRefusal> {
    let root_path = RepositoryPath::parse(required(invocation, PATH_OPTION)?)
        .map_err(|_| RequestRefusal::ValueUnusable { named: PATH_OPTION.to_owned() })?;
    let minimum_byte_length = byte_length(invocation, MINIMUM_BYTES_OPTION)?;
    let maximum_byte_length = byte_length(invocation, MAXIMUM_BYTES_OPTION)?;
    require_ordered_range(minimum_byte_length, maximum_byte_length)?;
    let tags = tags(invocation)?;
    Ok(Command::FindAssetsByMetadata(FindAssetsByMetadataCommand {
        maximum_byte_length,
        media_formats: media_formats(invocation)?,
        minimum_byte_length,
        property_predicates: predicates(invocation)?,
        result_window: window(invocation)?,
        root_path,
        tag_match_mode: tags.as_ref().map(|_| tag_match_mode(invocation)),
        tags,
    }))
}

/// Returns one byte length an option names, when it names one.
///
/// Canonical unsigned base ten and nothing else: no sign, no leading zero, no
/// fraction, no exponent. A spelling the repository could never report back is
/// a request that could never be answered, so it is refused here.
fn byte_length(
    invocation: &Invocation,
    named: &str,
) -> Result<Option<AssetByteLength>, RequestRefusal> {
    let Some(stated) = invocation.arguments.get(named) else {
        return Ok(None);
    };
    let unusable = || RequestRefusal::ValueUnusable { named: named.to_owned() };
    if !is_canonical_unsigned(stated) {
        return Err(unusable());
    }
    let value: u64 = stated.parse().map_err(|_| unusable())?;
    AssetByteLength::new(value).map(Some).map_err(|_| unusable())
}

/// Returns whether `stated` is a canonical unsigned base-ten integer.
#[must_use]
pub fn is_canonical_unsigned(stated: &str) -> bool {
    if stated.is_empty() || !stated.bytes().all(|octet| octet.is_ascii_digit()) {
        return false;
    }
    stated == "0" || !stated.starts_with('0')
}

/// Requires a range to run upwards, once both ends are lengths.
fn require_ordered_range(
    minimum: Option<AssetByteLength>,
    maximum: Option<AssetByteLength>,
) -> Result<(), RequestRefusal> {
    let (Some(minimum), Some(maximum)) = (minimum, maximum) else {
        return Ok(());
    };
    if minimum > maximum {
        return Err(RequestRefusal::ValueUnusable { named: MINIMUM_BYTES_OPTION.to_owned() });
    }
    Ok(())
}

/// Returns the media formats one invocation names, canonically.
fn media_formats(invocation: &Invocation) -> Result<Option<RequestedMediaFormats>, RequestRefusal> {
    let Some(stated) = invocation.arguments.get(MEDIA_FORMATS_OPTION) else {
        return Ok(None);
    };
    let unusable = || RequestRefusal::ValueUnusable { named: MEDIA_FORMATS_OPTION.to_owned() };
    let mut values = stated
        .split(LIST_SEPARATOR)
        .map(|part| MediaFormat::new(part.to_owned()).map_err(|_| unusable()))
        .collect::<Result<Vec<MediaFormat>, RequestRefusal>>()?;
    values.sort_by(|left, right| left.as_text().as_bytes().cmp(right.as_text().as_bytes()));
    RequestedMediaFormats::canonical(values).map(Some).map_err(|_| unusable())
}

/// Returns the tags one invocation names, canonically.
fn tags(invocation: &Invocation) -> Result<Option<RequestedAssetTags>, RequestRefusal> {
    let Some(stated) = invocation.arguments.get(TAGS_OPTION) else {
        return Ok(None);
    };
    let unusable = || RequestRefusal::ValueUnusable { named: TAGS_OPTION.to_owned() };
    let mut values = stated
        .split(LIST_SEPARATOR)
        .map(|part| AssetTag::new(part.to_owned()).map_err(|_| unusable()))
        .collect::<Result<Vec<AssetTag>, RequestRefusal>>()?;
    values.sort_by(|left, right| left.as_text().as_bytes().cmp(right.as_text().as_bytes()));
    RequestedAssetTags::canonical(values).map(Some).map_err(|_| unusable())
}

/// Returns how many of the named tags an asset must carry.
fn tag_match_mode(invocation: &Invocation) -> TagMatchMode {
    if invocation.arguments.contains_key(MATCH_ALL_OPTION) {
        TagMatchMode::All
    } else {
        TagMatchMode::Any
    }
}
