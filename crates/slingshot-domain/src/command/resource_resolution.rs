//! Which resource an address reaches, and which address reaches a resource.
//!
//! Two questions that answer opposite directions and share every value they
//! carry. They land in one leaf because the thing that makes the pair worth
//! having is a comparable trace, and two leaves would give them two traces that
//! nobody could line up.
//!
//! # Not resolving is an answer
//!
//! An address that reaches nothing is not a failure. The author looked, applied
//! the mapping, and found no resource - which is exactly what the caller asked
//! and exactly what a misconfiguration looks like. The result carries no
//! resolved path and the command succeeded.
//!
//! # A trace is the caller's decision
//!
//! The trace is present exactly when the request asked for one, and refused
//! otherwise. An author that volunteered a trace nobody asked for would make the
//! result's size depend on the deployment rather than on the request.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::component_resource_type::ComponentResourceType;
use crate::command::repository_path::RepositoryPath;
use crate::command::resource_mapping_entry::{RequestAddress, ResourceMappingFailure};

/// One request to resolve an address to a resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveResourcePathCommand {
    /// Whether the answer carries the entries that decided it.
    pub include_trace: bool,
    /// The address to resolve.
    pub request_address: RequestAddress,
}

/// One request to map a resource to the address that reaches it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapResourcePathCommand {
    /// Whether the answer carries the entries that decided it.
    pub include_trace: bool,
    /// The resource to map.
    pub repository_path: RepositoryPath,
    /// The authority the emitted address should be relative to, when the caller
    /// said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_authority: Option<String>,
}

/// What resolving an address found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolveResourcePathResult {
    /// The extension the address carried, when it carried one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    /// The address that was resolved.
    pub request_address: RequestAddress,
    /// The resource the address reached, when it reached one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<RepositoryPath>,
    /// The resource type that resource declares, when it declares one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<ComponentResourceType>,
    /// The selectors the address carried, in order.
    pub selectors: Vec<String>,
    /// The suffix the address carried, when it carried one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    /// The entries that decided it, when the request asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Vec<RepositoryPath>>,
}

/// What mapping a resource produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MapResourcePathResult {
    /// The address the author would emit.
    pub mapped_address: RequestAddress,
    /// The resource that was mapped.
    pub repository_path: RepositoryPath,
    /// The entries that decided it, when the request asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Vec<RepositoryPath>>,
}

impl ResolveResourcePathResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ResourceMappingFailure::NotThisRequest`] when it echoes another
    /// address, [`ResourceMappingFailure::TraceMisplaced`] when the trace's
    /// presence disagrees with the request, and
    /// [`ResourceMappingFailure::TraceTooLong`] above the contract's trace bound.
    pub fn require_answers(
        &self,
        command: &ResolveResourcePathCommand,
    ) -> Result<(), ResourceMappingFailure> {
        if self.request_address != command.request_address {
            return Err(ResourceMappingFailure::NotThisRequest);
        }
        require_trace(self.trace.as_deref(), command.include_trace, self.resolved_path.is_some())
    }
}

impl MapResourcePathResult {
    /// Requires this result to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ResourceMappingFailure::NotThisRequest`] when it echoes another
    /// resource, [`ResourceMappingFailure::TraceMisplaced`] when the trace's
    /// presence disagrees with the request, and
    /// [`ResourceMappingFailure::TraceTooLong`] above the contract's trace bound.
    pub fn require_answers(
        &self,
        command: &MapResourcePathCommand,
    ) -> Result<(), ResourceMappingFailure> {
        if self.repository_path != command.repository_path {
            return Err(ResourceMappingFailure::NotThisRequest);
        }
        require_trace(self.trace.as_deref(), command.include_trace, true)
    }
}

/// Why a resolution or a mapping could not be attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceResolutionFailure {
    /// The resolver could not be reached.
    ResolutionFailed,
    /// The mapping took more work than the contract permits.
    ResolutionBudgetExceeded,
    /// The address is not one this contract carries.
    RequestAddressRejected,
}

/// One refused resolution or mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceResolutionRefusal {
    /// Why it was refused.
    pub failure: ResourceResolutionFailure,
    /// The subject this request named, as it was written.
    pub subject: String,
}

/// Requires a trace to be present exactly when it was asked for.
///
/// The one exception is a resolution that found nothing: there were no entries
/// to report, so an absent trace is the honest answer even where one was asked
/// for.
fn require_trace(
    trace: Option<&[RepositoryPath]>,
    asked: bool,
    resolved: bool,
) -> Result<(), ResourceMappingFailure> {
    match (trace, asked) {
        (Some(entries), true) => {
            let bound = CommandContract::embedded().limit("maximum_resolution_trace_entries");
            if u64::try_from(entries.len()).unwrap_or(u64::MAX) > bound {
                return Err(ResourceMappingFailure::TraceTooLong);
            }
            Ok(())
        }
        (None, false) => Ok(()),
        (None, true) if !resolved => Ok(()),
        _ => Err(ResourceMappingFailure::TraceMisplaced),
    }
}

/// One resolution exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolutionDocument {
    /// The extension the address carried.
    #[serde(default)]
    extension: Option<String>,
    /// The address that was resolved.
    request_address: RequestAddress,
    /// The resource the address reached.
    #[serde(default)]
    resolved_path: Option<RepositoryPath>,
    /// The resource type that resource declares.
    #[serde(default)]
    resource_type: Option<ComponentResourceType>,
    /// The selectors the address carried.
    selectors: Vec<String>,
    /// The suffix the address carried.
    #[serde(default)]
    suffix: Option<String>,
    /// The entries that decided it.
    #[serde(default)]
    trace: Option<Vec<RepositoryPath>>,
}

impl<'de> Deserialize<'de> for ResolveResourcePathResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResolutionDocument::deserialize(deserializer)?;
        let bound = CommandContract::embedded().limit("maximum_resolution_trace_entries");
        if document
            .trace
            .as_ref()
            .is_some_and(|entries| u64::try_from(entries.len()).unwrap_or(u64::MAX) > bound)
        {
            return Err(Source::Error::custom(ResourceMappingFailure::TraceTooLong));
        }
        Ok(Self {
            extension: document.extension,
            request_address: document.request_address,
            resolved_path: document.resolved_path,
            resource_type: document.resource_type,
            selectors: document.selectors,
            suffix: document.suffix,
            trace: document.trace,
        })
    }
}

/// One mapping exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MappingDocument {
    /// The address the author would emit.
    mapped_address: RequestAddress,
    /// The resource that was mapped.
    repository_path: RepositoryPath,
    /// The entries that decided it.
    #[serde(default)]
    trace: Option<Vec<RepositoryPath>>,
}

impl<'de> Deserialize<'de> for MapResourcePathResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = MappingDocument::deserialize(deserializer)?;
        let bound = CommandContract::embedded().limit("maximum_resolution_trace_entries");
        if document
            .trace
            .as_ref()
            .is_some_and(|entries| u64::try_from(entries.len()).unwrap_or(u64::MAX) > bound)
        {
            return Err(Source::Error::custom(ResourceMappingFailure::TraceTooLong));
        }
        Ok(Self {
            mapped_address: document.mapped_address,
            repository_path: document.repository_path,
            trace: document.trace,
        })
    }
}
