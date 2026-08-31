//! Which provider, which repository, and which machine each row runs on.
//!
//! None of it is inferred. A Git remote is a file anybody can edit, a branch
//! name moves, and a repository display name can be renamed or reused, so a
//! hosted job that trusted any of them would be trusting whatever the machine
//! it happened to run on was configured with. What this reads instead is one
//! committed document the owner wrote, and every hosted run compares what the
//! provider reports about itself against it.
//!
//! # An identity is a number, not a name
//!
//! Two things separate this repository from a fork or a rename: the immutable
//! numeric identity the provider assigned the account, and the one it assigned
//! the repository. A display name matches across a rename and across a fork,
//! which is exactly when it matters that it does not.
//!
//! The repository identity is assigned when the repository is created. Until
//! one is recorded here this build refuses every hosted evidence claim rather
//! than accepting one on the strength of a name, and says which value is
//! missing. That refusal is the point: an authority with a hole in it that
//! still admitted evidence would be worse than no authority at all.
//!
//! # A claim nobody probed is not a claim
//!
//! Each row declares how its build source is protected and how its build's
//! network is denied, and each declaration names the probe that establishes it.
//! `digest_observation_only` records permission bits and before-and-after
//! digests and says outright that a malicious tool running as the same
//! principal could restore the bytes it changed; calling that immutable or
//! sandboxed would be describing a different mechanism.

use std::collections::BTreeSet;

use serde::Deserialize;

/// Where the authority lives.
pub const AUTHORITY_PATH: &str = "support/github-automation-authority.toml";

/// The format this document declares.
pub const AUTHORITY_FORMAT: &str = "slingshot.github-automation-authority/1";

/// The one provider this authority admits.
pub const PROVIDER: &str = "github-actions";

/// What an unassigned repository identity is written as.
pub const UNASSIGNED: &str = "unassigned";

/// The canonical scheme and host a repository address is written under.
pub const CANONICAL_PREFIX: &str = "https://github.com/";

/// Source-protection claims a row may make.
pub const SOURCE_PROTECTIONS: &[&str] = &["operating_system_enforced", "digest_observation_only"];

/// Runner classes this authority admits.
pub const RUNNER_CLASSES: &[&str] = &["github-hosted"];

/// Repository visibilities eligible for hosted attestation.
pub const ELIGIBLE_VISIBILITIES: &[&str] = &["public", "private-enterprise-cloud"];

/// Field names that would put a credential in a committed document.
pub const CREDENTIAL_SHAPED_NAMES: &[&str] =
    &["token", "secret", "password", "private_key", "private-key", "credential", "signing_key"];

/// What the owner confirmed about hosted automation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct GithubAutomationAuthority {
    /// The format this document declares.
    pub format: String,
    /// The provider the owner confirmed.
    pub provider: String,
    /// Where the workflows live.
    pub workflow_root: String,
    /// Which repository this is.
    pub repository: RepositoryIdentity,
    /// How a per-release RustSec review is authorized.
    pub release_review: ReleaseReview,
    /// One row per abstract supported target.
    pub row: Vec<AutomationRow>,
}

/// Which repository the provider has to report itself as.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RepositoryIdentity {
    /// The canonical address, in one spelling.
    pub canonical_address: String,
    /// The immutable numeric identity of the account.
    pub owner_identifier: u64,
    /// The immutable numeric identity of the repository, when it has one.
    pub identifier: String,
    /// The account name.
    pub owner: String,
    /// The repository name.
    pub name: String,
    /// Whether it is public or an eligible private repository.
    pub visibility: String,
}

/// How a per-release RustSec review is authorized.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ReleaseReview {
    /// The protected environment the review runs in.
    pub environment: String,
    /// The format the record it emits declares.
    pub record_format: String,
    /// The policy that decides whose approval counts.
    pub reviewer_policy: String,
}

/// One abstract target mapped to one exact machine.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AutomationRow {
    /// The architecture it runs on.
    pub architecture: String,
    /// Whether this row is the OCI-capable coordinator.
    pub coordinator: bool,
    /// Whether this row runs the pinned compatibility gate.
    pub finite_state_machine: bool,
    /// The linker its builds use.
    pub linker: String,
    /// How its build's network is denied.
    pub network_denial: String,
    /// The probe that establishes the denial.
    pub network_denial_probe: String,
    /// The image the runner is observed to be.
    pub observed_image: String,
    /// Which runner label selects it.
    pub runner_selector: String,
    /// Which class of runner it is.
    pub runner_class: String,
    /// How its build source is protected.
    pub source_protection: String,
    /// The probe that establishes the protection.
    pub source_protection_probe: String,
    /// The system root or software development kit its builds link against.
    pub system_root_or_software_development_kit: String,
    /// The Rust toolchain its builds use.
    pub toolchain: String,
    /// The exact target triple this row is.
    pub triple: String,
}

/// Why an authority, or a run claiming to match one, is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthorityRefusal {
    /// The document could not be read.
    #[error("the automation authority could not be read: {0}")]
    Unreadable(String),
    /// It declares another format.
    #[error("an automation authority declares {AUTHORITY_FORMAT}, and this declares {0}")]
    ForeignFormat(String),
    /// It names another provider.
    #[error("this build adapts to {PROVIDER}, and this authority names {0}")]
    ProviderUnacceptable(String),
    /// One value is not the one thing it may be.
    #[error("{field} is {held}, which is not one this authority admits")]
    ValueUnacceptable {
        /// Which field.
        field: &'static str,
        /// What it holds.
        held: String,
    },
    /// The document carries something that looks like a credential.
    #[error("{0} is a name a credential would be written under, and this document is committed")]
    CredentialShaped(String),
    /// The repository has no immutable identity yet.
    #[error(
        "this repository has no immutable identifier recorded, so no hosted run authenticates as it"
    )]
    RepositoryUnassigned,
    /// The rows and the supported matrix disagree.
    #[error("{0} is a supported target this authority maps to no environment")]
    RowMissing(String),
    /// One row is mapped twice, or two rows share a machine.
    #[error("{0} is assigned to more than one row")]
    RowRepeated(String),
    /// The count of rows carrying one exclusive role is wrong.
    #[error("exactly one row is the {role}, and {held} are")]
    ExclusiveRole {
        /// How many carry it.
        held: usize,
        /// Which role.
        role: &'static str,
    },
    /// What the provider reported is not what the authority says.
    #[error("this run reports {held} as its {field}, and the authority says {expected}")]
    RunUnauthorized {
        /// What the authority says.
        expected: String,
        /// Which field.
        field: &'static str,
        /// What the run reports.
        held: String,
    },
}

/// Returns the authority one document carries.
///
/// # Errors
///
/// Returns [`AuthorityRefusal`] naming the first thing that stops it being one
/// this build adapts to.
pub fn parse_authority(text: &str) -> Result<GithubAutomationAuthority, AuthorityRefusal> {
    let loose: toml::Value = toml::from_str(text)
        .map_err(|failure| AuthorityRefusal::Unreadable(failure.to_string()))?;
    require_no_credential_shaped_key(&loose)?;
    let held: GithubAutomationAuthority = loose
        .try_into()
        .map_err(|failure: toml::de::Error| AuthorityRefusal::Unreadable(failure.to_string()))?;
    if held.format != AUTHORITY_FORMAT {
        return Err(AuthorityRefusal::ForeignFormat(held.format));
    }
    if held.provider != PROVIDER {
        return Err(AuthorityRefusal::ProviderUnacceptable(held.provider));
    }
    require_repository_shape(&held.repository)?;
    require_rows_shape(&held.row)?;
    Ok(held)
}

/// Requires no key in the document to be one a credential would be written under.
///
/// Keys rather than raw text, so the comment explaining that credentials do not
/// belong here can say the word without being refused for saying it.
fn require_no_credential_shaped_key(held: &toml::Value) -> Result<(), AuthorityRefusal> {
    match held {
        toml::Value::Table(table) => {
            for (named, value) in table {
                let lowered = named.to_lowercase();
                if let Some(shaped) =
                    CREDENTIAL_SHAPED_NAMES.iter().find(|held| lowered.contains(*held))
                {
                    return Err(AuthorityRefusal::CredentialShaped((*shaped).to_owned()));
                }
                require_no_credential_shaped_key(value)?;
            }
            Ok(())
        }
        toml::Value::Array(values) => {
            for value in values {
                require_no_credential_shaped_key(value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Requires the repository identity to be spelled the one way it may be.
fn require_repository_shape(held: &RepositoryIdentity) -> Result<(), AuthorityRefusal> {
    let canonical = format!("{CANONICAL_PREFIX}{}/{}", held.owner, held.name);
    if held.canonical_address != canonical {
        return Err(AuthorityRefusal::ValueUnacceptable {
            field: "canonical-address",
            held: held.canonical_address.clone(),
        });
    }
    if !ELIGIBLE_VISIBILITIES.contains(&held.visibility.as_str()) {
        return Err(AuthorityRefusal::ValueUnacceptable {
            field: "visibility",
            held: held.visibility.clone(),
        });
    }
    if held.identifier != UNASSIGNED && held.identifier.parse::<u64>().is_err() {
        return Err(AuthorityRefusal::ValueUnacceptable {
            field: "identifier",
            held: held.identifier.clone(),
        });
    }
    Ok(())
}

/// Requires every row to be one machine, named once, with probed claims.
fn require_rows_shape(rows: &[AutomationRow]) -> Result<(), AuthorityRefusal> {
    let mut triples = BTreeSet::new();
    let mut selectors = BTreeSet::new();
    for row in rows {
        if !RUNNER_CLASSES.contains(&row.runner_class.as_str()) {
            return Err(AuthorityRefusal::ValueUnacceptable {
                field: "runner-class",
                held: row.runner_class.clone(),
            });
        }
        if !SOURCE_PROTECTIONS.contains(&row.source_protection.as_str()) {
            return Err(AuthorityRefusal::ValueUnacceptable {
                field: "source-protection",
                held: row.source_protection.clone(),
            });
        }
        for probe in [&row.source_protection_probe, &row.network_denial_probe] {
            if probe.trim().is_empty() {
                return Err(AuthorityRefusal::ValueUnacceptable {
                    field: "probe",
                    held: row.triple.clone(),
                });
            }
        }
        if !triples.insert(row.triple.clone()) {
            return Err(AuthorityRefusal::RowRepeated(row.triple.clone()));
        }
        if !selectors.insert(row.runner_selector.clone()) {
            return Err(AuthorityRefusal::RowRepeated(row.runner_selector.clone()));
        }
    }
    require_exactly_one(rows.iter().filter(|row| row.coordinator).count(), "coordinator")?;
    require_exactly_one(
        rows.iter().filter(|row| row.finite_state_machine).count(),
        "compatible row",
    )
}

/// How many rows may carry one exclusive role.
const EXACTLY_ONE: usize = 1;

/// How many identity fields a reported run is compared on.
const COMPARED_IDENTITY_FIELDS: usize = 3;

/// Requires one exclusive role to be carried exactly once.
fn require_exactly_one(held: usize, role: &'static str) -> Result<(), AuthorityRefusal> {
    if held == EXACTLY_ONE { Ok(()) } else { Err(AuthorityRefusal::ExclusiveRole { held, role }) }
}

/// Requires the authority to cover exactly the supported targets.
///
/// # Errors
///
/// Returns [`AuthorityRefusal::RowMissing`] for a supported target this maps to
/// no machine, and [`AuthorityRefusal::RowRepeated`] for a machine it maps that
/// the matrix does not declare.
pub fn require_covers(
    authority: &GithubAutomationAuthority,
    supported: &[String],
) -> Result<(), AuthorityRefusal> {
    let mapped: BTreeSet<&str> = authority.row.iter().map(|row| row.triple.as_str()).collect();
    for triple in supported {
        if !mapped.contains(triple.as_str()) {
            return Err(AuthorityRefusal::RowMissing(triple.clone()));
        }
    }
    for triple in mapped {
        if !supported.iter().any(|held| held == triple) {
            return Err(AuthorityRefusal::RowRepeated(triple.to_owned()));
        }
    }
    Ok(())
}

/// What one hosted run says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedRun {
    /// The workflow path the provider says is running.
    pub workflow_path: String,
    /// The repository the provider says this is.
    pub repository: String,
    /// The immutable repository identity the provider reports.
    pub repository_identifier: String,
    /// The immutable owner identity the provider reports.
    pub repository_owner_identifier: String,
    /// The runner label the job selected.
    pub runner_selector: String,
}

/// Requires one hosted run to be authorized by this authority.
///
/// Nothing about the machine this runs on is consulted: an ambient Git remote
/// and a process-environment value are both things a caller controls, and the
/// question here is whether the provider's own report matches what the owner
/// wrote down.
///
/// # Errors
///
/// Returns [`AuthorityRefusal::RepositoryUnassigned`] while no immutable
/// identity is recorded, and [`AuthorityRefusal::RunUnauthorized`] naming the
/// first field that differs.
pub fn require_authorized(
    authority: &GithubAutomationAuthority,
    reported: &ReportedRun,
) -> Result<(), AuthorityRefusal> {
    let expected_identifier = &authority.repository.identifier;
    if expected_identifier == UNASSIGNED {
        return Err(AuthorityRefusal::RepositoryUnassigned);
    }
    let expected_name = format!("{}/{}", authority.repository.owner, authority.repository.name);
    let owner_identifier = authority.repository.owner_identifier.to_string();
    let compared: [(&'static str, &str, &str); COMPARED_IDENTITY_FIELDS] = [
        ("repository", expected_name.as_str(), reported.repository.as_str()),
        ("repository identifier", expected_identifier, reported.repository_identifier.as_str()),
        (
            "owner identifier",
            owner_identifier.as_str(),
            reported.repository_owner_identifier.as_str(),
        ),
    ];
    for (field, expected, held) in compared {
        if expected != held {
            return Err(AuthorityRefusal::RunUnauthorized {
                expected: expected.to_owned(),
                field,
                held: held.to_owned(),
            });
        }
    }
    if !reported.workflow_path.starts_with(authority.workflow_root.as_str()) {
        return Err(AuthorityRefusal::RunUnauthorized {
            expected: authority.workflow_root.clone(),
            field: "workflow path",
            held: reported.workflow_path.clone(),
        });
    }
    require_selector_mapped(authority, &reported.runner_selector)
}

/// Requires the runner a job selected to be one this authority maps.
fn require_selector_mapped(
    authority: &GithubAutomationAuthority,
    selector: &str,
) -> Result<(), AuthorityRefusal> {
    let mapped = authority.row.iter().any(|row| row.runner_selector == selector);
    if mapped {
        return Ok(());
    }
    Err(AuthorityRefusal::RunUnauthorized {
        expected: "a runner this authority maps".to_owned(),
        field: "runner",
        held: selector.to_owned(),
    })
}

/// Returns the bytes a reviewed repository identity is committed as.
///
/// Nonmutating on purpose: it reads one provider response somebody named,
/// takes the identity out of it, and prints what to commit. Nothing is written,
/// so the value that ends up in the authority is one a reviewer put there.
///
/// # Errors
///
/// Returns [`AuthorityRefusal::Unreadable`] for a response this cannot read and
/// [`AuthorityRefusal::ValueUnacceptable`] for one describing another
/// repository.
pub fn propose_repository_identifier(
    authority: &GithubAutomationAuthority,
    response: &str,
) -> Result<String, AuthorityRefusal> {
    let held: serde_json::Value = serde_json::from_str(response)
        .map_err(|failure| AuthorityRefusal::Unreadable(failure.to_string()))?;
    let full_name = held["full_name"].as_str().unwrap_or_default();
    let expected = format!("{}/{}", authority.repository.owner, authority.repository.name);
    if full_name != expected {
        return Err(AuthorityRefusal::ValueUnacceptable {
            field: "full_name",
            held: full_name.to_owned(),
        });
    }
    let identifier = held["id"].as_u64().ok_or_else(|| AuthorityRefusal::ValueUnacceptable {
        field: "id",
        held: held["id"].to_string(),
    })?;
    Ok(format!("identifier = \"{identifier}\"\n"))
}
