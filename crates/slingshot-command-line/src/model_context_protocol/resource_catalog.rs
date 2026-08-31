//! Which resources this server publishes and how each is addressed.
//!
//! A resource is addressed by target and identifier, so reading one needs no
//! operation to have been invented for it. That matters most for a maintenance
//! result: it belongs to a target rather than to any command, and an address
//! that demanded an operation would force this server to make one up.
//!
//! # An address is read, never assembled from a client's pieces
//!
//! Every address is parsed back into the parts it names, and every part is
//! checked against what that part may be. A maintenance identifier that carried
//! an operation, a slot, or a path separator would be a caller reaching
//! somewhere the address space does not go.
//!
//! # A read is authenticated against what the lookup said
//!
//! Metadata is asked for first, by target and identifier alone, and its answer
//! is what the read is checked against: same target, same identifier, same
//! kind, same reviewed source, same digest, same length, same media type. Only
//! one thing may differ between the two calls, and only in one direction - a
//! current preview becoming an application receipt at the next revision, which
//! is what an apply committing between the calls looks like from here.

use serde_json::Value;

use crate::machine_outcome_envelope::{ACCESS_SCHEME, encoded_segment};

/// The address of one operation's state and results.
pub const OPERATION_TEMPLATE: &str = "slingshot://profiles/{profile}/environments/{environment}\
     /targets/{author_target_identity_digest}/operations/{operation_identifier}";

/// The address of one artifact an operation produced.
pub const ARTIFACT_TEMPLATE: &str = "slingshot://profiles/{profile}/environments/{environment}\
     /targets/{author_target_identity_digest}/operations/{operation_identifier}\
     /artifacts/{artifact_identifier}";

/// The address of one maintenance result, which belongs to no operation.
pub const MAINTENANCE_TEMPLATE: &str = "slingshot://profiles/{profile}/environments/{environment}\
     /targets/{author_target_identity_digest}/maintenance/results/{maintenance_result_identifier}";

/// What every maintenance result is served as.
pub const MAINTENANCE_MEDIA_TYPE: &str = "application/json";

/// How many characters a maintenance-result identifier carries.
pub const MAINTENANCE_IDENTIFIER_CHARACTERS: usize = 64;

/// How many operations one listing page carries at most.
pub const MAXIMUM_LISTED_OPERATIONS: usize = 200;

/// What one address names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceAddress {
    /// One operation's state and results.
    Operation {
        /// Which namespace.
        namespace: Namespace,
        /// Which operation.
        operation_identifier: String,
    },
    /// One artifact an operation produced, described and never streamed.
    Artifact {
        /// Which namespace.
        namespace: Namespace,
        /// Which operation produced it.
        operation_identifier: String,
        /// Which artifact.
        artifact_identifier: String,
    },
    /// One maintenance result, which belongs to a target and to no operation.
    MaintenanceResult {
        /// Which namespace.
        namespace: Namespace,
        /// Which result.
        maintenance_result_identifier: String,
    },
}

/// Which profile, environment, and partition an address names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Namespace {
    /// Which partition.
    pub author_target_identity_digest: String,
    /// Which environment.
    pub environment: String,
    /// Which profile.
    pub profile: String,
}

/// Why one address or one read is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResourceRefusal {
    /// The address does not begin where this server's addresses begin.
    #[error("a resource address begins {ACCESS_SCHEME}://")]
    ForeignScheme,
    /// The address names parts this server does not publish.
    #[error("this server publishes no resource shaped like that")]
    UnknownShape,
    /// A maintenance identifier is not one Plan 0004 produces.
    #[error(
        "a maintenance result is named by {MAINTENANCE_IDENTIFIER_CHARACTERS} lowercase \
         hexadecimal characters, and this is named otherwise"
    )]
    IdentifierUnusable,
    /// What the read started differs from what the lookup described.
    #[error("{0} differs between the lookup and the read, so the two are not one document")]
    ReadDiverged(String),
    /// The resource holds something no resource may carry.
    #[error("{0} is not something a resource says")]
    Undisclosable(String),
}

/// Returns the address one uniform resource identifier names.
///
/// # Errors
///
/// Returns [`ResourceRefusal`] naming the first rule the address breaks.
pub fn parse(uri: &str) -> Result<ResourceAddress, ResourceRefusal> {
    let prefix = format!("{ACCESS_SCHEME}://");
    let rest = uri.strip_prefix(&prefix).ok_or(ResourceRefusal::ForeignScheme)?;
    let segments: Vec<String> = rest.split('/').map(decoded_segment).collect();
    let namespace = namespace_of(&segments)?;
    match segments.get(NAMESPACE_SEGMENTS..) {
        Some([kind, identifier]) if kind == "operations" => {
            Ok(ResourceAddress::Operation { namespace, operation_identifier: identifier.clone() })
        }
        Some([kind, operation, slot, artifact]) if kind == "operations" && slot == "artifacts" => {
            Ok(ResourceAddress::Artifact {
                namespace,
                operation_identifier: operation.clone(),
                artifact_identifier: artifact.clone(),
            })
        }
        Some([kind, held, identifier]) if kind == "maintenance" && held == "results" => {
            require_maintenance_identifier(identifier)?;
            Ok(ResourceAddress::MaintenanceResult {
                namespace,
                maintenance_result_identifier: identifier.clone(),
            })
        }
        _ => Err(ResourceRefusal::UnknownShape),
    }
}

/// How many segments name the namespace before the resource kind.
const NAMESPACE_SEGMENTS: usize = 6;

/// Returns the namespace the leading segments name.
fn namespace_of(segments: &[String]) -> Result<Namespace, ResourceRefusal> {
    let Some([profiles, profile, environments, environment, targets, target]) =
        segments.get(..NAMESPACE_SEGMENTS)
    else {
        return Err(ResourceRefusal::UnknownShape);
    };
    if profiles != "profiles" || environments != "environments" || targets != "targets" {
        return Err(ResourceRefusal::UnknownShape);
    }
    Ok(Namespace {
        author_target_identity_digest: target.clone(),
        environment: environment.clone(),
        profile: profile.clone(),
    })
}

/// Returns one address segment with its escapes read back.
fn decoded_segment(segment: &str) -> String {
    let mut written = String::new();
    let mut characters = segment.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            written.push(character);
            continue;
        }
        let held: String = characters.by_ref().take(ESCAPE_DIGITS).collect();
        match u8::from_str_radix(&held, HEXADECIMAL) {
            Ok(byte) => written.push(char::from(byte)),
            Err(_) => written.push('%'),
        }
    }
    written
}

/// How many digits one escape carries.
const ESCAPE_DIGITS: usize = 2;

/// The base an escape is written in.
const HEXADECIMAL: u32 = 16;

/// Requires one maintenance identifier to be one Plan 0004 produces.
///
/// # Errors
///
/// Returns [`ResourceRefusal::IdentifierUnusable`], which is also what an
/// identifier carrying an operation, a slot, or a separator receives: none of
/// those are sixty-four hexadecimal characters, and the check needs no special
/// case to refuse them.
pub fn require_maintenance_identifier(identifier: &str) -> Result<(), ResourceRefusal> {
    let shaped = identifier.len() == MAINTENANCE_IDENTIFIER_CHARACTERS
        && identifier.chars().all(|held| held.is_ascii_digit() || ('a'..='f').contains(&held));
    if shaped { Ok(()) } else { Err(ResourceRefusal::IdentifierUnusable) }
}

/// Returns the address one maintenance result is published at.
#[must_use]
pub fn maintenance_address(namespace: &Namespace, identifier: &str) -> String {
    format!(
        "{ACCESS_SCHEME}://profiles/{}/environments/{}/targets/{}/maintenance/results/{}",
        encoded_segment(&namespace.profile),
        encoded_segment(&namespace.environment),
        encoded_segment(&namespace.author_target_identity_digest),
        encoded_segment(identifier)
    )
}

/// What a metadata lookup said about one maintenance result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceFacts {
    /// Which revision of the association this is.
    pub association_revision: u64,
    /// Which partition it belongs to.
    pub author_target_identity_digest: String,
    /// How many bytes it holds.
    pub byte_length: u64,
    /// What it digests to.
    pub content_digest: String,
    /// Whether it previews maintenance or receipts it.
    pub kind: String,
    /// Which result.
    pub maintenance_result_identifier: String,
    /// What it is.
    pub media_type: String,
    /// Whether a current preview or an application receipt retains it.
    pub retention_owner: String,
    /// What the reviewer approved.
    pub reviewed_source_digest: String,
}

/// What a current preview's retention says.
pub const PREVIEW_OWNER: &str = "current_preview";

/// What an application receipt's retention says.
pub const RECEIPT_OWNER: &str = "application_receipt";

/// Requires a read's start to describe the document the lookup described.
///
/// Everything is compared, and only one difference is admitted: a current
/// preview becoming an application receipt at the next revision. That is what
/// an exact apply committing between the two calls looks like from here, and
/// refusing it would make a correct sequence fail for having been correct.
///
/// # Errors
///
/// Returns [`ResourceRefusal::ReadDiverged`] naming the first field that
/// differs.
pub fn require_same_document(
    lookup: &MaintenanceFacts,
    start: &MaintenanceFacts,
) -> Result<(), ResourceRefusal> {
    let compared = [
        ("the target", &lookup.author_target_identity_digest, &start.author_target_identity_digest),
        (
            "the identifier",
            &lookup.maintenance_result_identifier,
            &start.maintenance_result_identifier,
        ),
        ("the reviewed source", &lookup.reviewed_source_digest, &start.reviewed_source_digest),
        ("the content digest", &lookup.content_digest, &start.content_digest),
        ("the media type", &lookup.media_type, &start.media_type),
    ];
    for (named, held, started) in compared {
        if held != started {
            return Err(ResourceRefusal::ReadDiverged(named.to_owned()));
        }
    }
    if lookup.byte_length != start.byte_length {
        return Err(ResourceRefusal::ReadDiverged("the length".to_owned()));
    }
    require_same_ownership(lookup, start)
}

/// Requires ownership to be unchanged, or changed the one way it may change.
fn require_same_ownership(
    lookup: &MaintenanceFacts,
    start: &MaintenanceFacts,
) -> Result<(), ResourceRefusal> {
    if lookup.retention_owner == start.retention_owner {
        return match lookup.association_revision == start.association_revision {
            true => Ok(()),
            false => Err(ResourceRefusal::ReadDiverged("the association revision".to_owned())),
        };
    }
    let transferred = lookup.retention_owner == PREVIEW_OWNER
        && start.retention_owner == RECEIPT_OWNER
        && start.association_revision == lookup.association_revision + 1;
    if transferred { Ok(()) } else { Err(ResourceRefusal::ReadDiverged("the owner".to_owned())) }
}

/// Members no resource this server publishes may carry.
pub const UNDISCLOSABLE_MEMBERS: &[&str] = &[
    "authentication",
    "credentials_file",
    "password",
    "private_key",
    "publisher",
    "readiness_nonce",
    "path",
];

/// Requires one resource value to say nothing it may not say.
///
/// # Errors
///
/// Returns [`ResourceRefusal::Undisclosable`] naming the first member that a
/// resource may not carry.
pub fn require_disclosable(value: &Value) -> Result<(), ResourceRefusal> {
    match value {
        Value::Object(members) => {
            for (name, held) in members {
                if UNDISCLOSABLE_MEMBERS.contains(&name.as_str()) {
                    return Err(ResourceRefusal::Undisclosable(name.clone()));
                }
                require_disclosable(held)?;
            }
            Ok(())
        }
        Value::Array(held) => held.iter().try_for_each(require_disclosable),
        _ => Ok(()),
    }
}

/// One page of a target's operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingPage {
    /// What to quote for the next page, when there is one.
    pub continuation_token: Option<String>,
    /// The operations on this page, in the order the daemon listed them.
    pub operations: Vec<String>,
}

impl ListingPage {
    /// Returns one page of at most the bound, with what follows it.
    #[must_use]
    pub fn of(listed: &[String], next: Option<String>) -> Self {
        let held: Vec<String> = listed.iter().take(MAXIMUM_LISTED_OPERATIONS).cloned().collect();
        let overflowed = listed.len() > MAXIMUM_LISTED_OPERATIONS;
        Self {
            continuation_token: if overflowed { held.last().cloned() } else { next },
            operations: held,
        }
    }
}
