//! Naming how a caller reaches a result that is not inline.
//!
//! What a command produced may be larger than a terminal should hold, so the
//! answer carries a reference rather than the bytes. Printing megabytes into a
//! pipe is not the same as making them available, and a caller who wanted them
//! on disk would then have to capture standard output to get there.
//!
//! # A reference names the daemon's own address space
//!
//! Never a local path. The bytes may not be on this machine yet, and a
//! reference that named where they would go if they were fetched would be a
//! reference to a file that does not exist. The URI names what to ask the
//! daemon for, and asking is a separate command a caller runs when they want it.

use crate::machine_outcome_envelope::{
    ArtifactAccess, MaintenanceResultAccess, artifact_uri, maintenance_result_uri,
};

/// What a descriptor says about one artifact the daemon holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactDescriptor {
    /// Which artifact.
    pub artifact_identifier: String,
    /// How many bytes it holds.
    pub byte_length: u64,
    /// What it digests to.
    pub content_digest: String,
    /// What it is.
    pub media_type: String,
}

/// Which target and operation one access entry belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessContext {
    /// Which partition.
    pub author_target_identity_digest: String,
    /// Which environment.
    pub environment: String,
    /// Which operation.
    pub operation_identifier: String,
    /// Which profile.
    pub profile: String,
}

/// Returns the access entry one descriptor produces.
///
/// The complete descriptor is carried through rather than summarized: a caller
/// deciding whether to fetch needs the length and the digest, and a reference
/// without them is one they have to fetch to evaluate.
#[must_use]
pub fn access_entry(context: &AccessContext, descriptor: &ArtifactDescriptor) -> ArtifactAccess {
    ArtifactAccess {
        artifact_identifier: descriptor.artifact_identifier.clone(),
        author_target_identity_digest: context.author_target_identity_digest.clone(),
        byte_length: descriptor.byte_length,
        content_digest: descriptor.content_digest.clone(),
        media_type: descriptor.media_type.clone(),
        operation_identifier: context.operation_identifier.clone(),
        uri: artifact_uri(
            &context.profile,
            &context.environment,
            &context.author_target_identity_digest,
            &context.operation_identifier,
            &descriptor.artifact_identifier,
        ),
    }
}

/// What one maintenance association says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceAssociation {
    /// Which revision of the association this is.
    pub association_revision: u64,
    /// How many bytes it holds.
    pub byte_length: u64,
    /// What it digests to.
    pub content_digest: String,
    /// What kind of result it is.
    pub kind: String,
    /// Which result.
    pub maintenance_result_identifier: String,
    /// What it is.
    pub media_type: String,
    /// What the reviewer approved.
    pub reviewed_source_digest: String,
}

/// Returns the access entry one maintenance association produces.
#[must_use]
pub fn maintenance_entry(
    profile: &str,
    environment: &str,
    author_target_identity_digest: &str,
    association: &MaintenanceAssociation,
) -> MaintenanceResultAccess {
    MaintenanceResultAccess {
        association_revision: association.association_revision,
        author_target_identity_digest: author_target_identity_digest.to_owned(),
        byte_length: association.byte_length,
        content_digest: association.content_digest.clone(),
        kind: association.kind.clone(),
        maintenance_result_identifier: association.maintenance_result_identifier.clone(),
        media_type: association.media_type.clone(),
        reviewed_source_digest: association.reviewed_source_digest.clone(),
        uri: maintenance_result_uri(
            profile,
            environment,
            author_target_identity_digest,
            &association.maintenance_result_identifier,
        ),
    }
}
