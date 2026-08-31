//! What makes native build evidence trustworthy.
//!
//! An archive and a manifest that agree with each other prove that whoever
//! produced one produced the other. They say nothing about where either came
//! from, which is the only question that matters once the build happens
//! somewhere nobody in this repository can watch. So a release archive is
//! believed only when a provider attests that a named workflow built it at a
//! named source revision, and only when that attestation verifies against a
//! snapshot of the provider's trust root that a reviewer looked at.
//!
//! # The order is the security
//!
//! The root is authenticated against its committed digest before a bundle is
//! parsed at all. A bundle read first and checked afterwards has already been
//! given the chance to influence what checks it; and a verifier that fell back
//! to its own built-in root, to an operating-system trust store, or to a
//! network lookup would be letting the thing being verified choose what
//! verifies it.
//!
//! # What this half does and does not establish
//!
//! This half is the policy: which identity, which workflow, which builder,
//! which statement and provenance versions, and which subjects a bundle has to
//! carry. The cryptographic half - signature, certificate chain, log inclusion,
//! integrated time - belongs to the pinned offline verifier, which is named and
//! version-pinned here because a verifier is a program whose behaviour changes
//! between versions. Neither half is evidence without the other, and a
//! verification failure makes every assertion the bundle carries untrusted
//! rather than merely unconfirmed.

use std::collections::BTreeSet;
use std::path::Path;

use base64::Engine as _;
use serde::Deserialize;
use sha2::Digest as _;

/// Where the policy lives.
pub const POLICY_PATH: &str = "support/release-attestation-policy.toml";

/// The format the policy declares.
pub const POLICY_FORMAT: &str = "slingshot.release-attestation-policy/1";

/// How many identity fields a statement is compared on.
const COMPARED_IDENTITY_FIELDS: usize = 5;

/// Repository visibilities eligible for hosted attestation.
pub const ELIGIBLE_VISIBILITIES: &[&str] = &["public", "private-enterprise-cloud"];

/// What the owner approved about native evidence.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ReleaseAttestationPolicy {
    /// The format this document declares.
    pub format: String,
    /// Which certificate identity a bundle must carry.
    pub identity: IdentityPolicy,
    /// Which identity provider issued the signing certificate.
    pub issuer: IssuerPolicy,
    /// Whether this repository is eligible at all.
    pub repository: RepositoryEligibility,
    /// Which statement and provenance versions this build reads.
    pub statement: StatementPolicy,
    /// The reviewed snapshot of the provider's trust root.
    pub trusted_root: TrustedRoot,
    /// What a certificate's validity has to satisfy.
    pub validity: ValidityPolicy,
    /// The offline verifier and its exact version.
    pub verifier: VerifierPolicy,
}

/// Which certificate identity a bundle must carry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct IdentityPolicy {
    /// The builder the provenance names.
    pub builder_identity: String,
    /// The runner class the build ran on.
    pub runner_environment: String,
    /// The repository the source came from.
    pub source_repository_uri: String,
    /// The account that owns it.
    pub source_repository_owner_uri: String,
    /// The workflow that built it.
    pub workflow_path: String,
}

/// Which identity provider issued the signing certificate.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct IssuerPolicy {
    /// The Sigstore instance whose root is committed.
    pub instance: String,
    /// The token issuer the certificate was requested against.
    pub oidc_issuer: String,
}

/// Whether this repository is eligible at all.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RepositoryEligibility {
    /// Which hosted arrangement it is.
    pub eligibility: String,
    /// Whether it is public or an eligible private repository.
    pub visibility: String,
}

/// Which statement and provenance versions this build reads.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StatementPolicy {
    /// The digest algorithm every subject is named by.
    pub digest_algorithm: String,
    /// The statement version.
    pub in_toto_version: String,
    /// The provenance version.
    pub predicate_type: String,
}

/// The reviewed snapshot of the provider's trust root.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TrustedRoot {
    /// The media type the snapshot declares.
    pub media_type: String,
    /// Where the snapshot lives.
    pub path: String,
    /// Where it came from, exactly.
    pub provenance: String,
    /// What its bytes digest to.
    pub sha256: String,
}

/// What a certificate's validity has to satisfy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ValidityPolicy {
    /// Whether an expired certificate is ever accepted.
    pub accept_expired_certificate: bool,
    /// Whether trust material may be advanced by a run.
    pub accept_unpinned_root_update: bool,
    /// Whether the log's integrated time must fall inside validity.
    pub require_integrated_time_within_certificate_validity: bool,
}

/// The offline verifier and its exact version.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct VerifierPolicy {
    /// What the verifier is called.
    pub name: String,
    /// Exactly which version of it.
    pub version: String,
}

/// Why an attestation, or the policy behind it, is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AttestationRefusal {
    /// A document could not be read.
    #[error("the attestation policy could not be read: {0}")]
    Unreadable(String),
    /// The policy declares another format.
    #[error("an attestation policy declares {POLICY_FORMAT}, and this declares {0}")]
    ForeignFormat(String),
    /// A value is not one this build admits.
    #[error("{field} is {held}, which is not one this policy admits")]
    ValueUnacceptable {
        /// Which field.
        field: &'static str,
        /// What it holds.
        held: String,
    },
    /// The committed root is not the bytes the policy names.
    #[error("the trusted root digests to {held} and the policy names {expected}")]
    TrustedRootDrift {
        /// What the policy names.
        expected: String,
        /// What the file holds.
        held: String,
    },
    /// The policy would let a run choose its own trust material.
    #[error("{0} would let the thing being verified choose what verifies it")]
    TrustWouldBeChosenByTheVerified(&'static str),
    /// A bundle could not be read.
    #[error("the bundle could not be read: {0}")]
    BundleUnreadable(String),
    /// A bundle's identity is not the one the policy names.
    #[error("this bundle's {field} is {held}, and the policy names {expected}")]
    IdentityUnauthorized {
        /// What the policy names.
        expected: String,
        /// Which field.
        field: &'static str,
        /// What the bundle carries.
        held: String,
    },
    /// The subjects a bundle carries are not the ones expected.
    #[error("this bundle attests {held}, and exactly {expected} was expected")]
    SubjectsUnexpected {
        /// What was expected.
        expected: String,
        /// What it attests.
        held: String,
    },
}

/// Returns the policy one document carries.
///
/// # Errors
///
/// Returns [`AttestationRefusal`] naming the first thing that stops it being a
/// policy this build may verify under.
pub fn parse_policy(text: &str) -> Result<ReleaseAttestationPolicy, AttestationRefusal> {
    let held: ReleaseAttestationPolicy = toml::from_str(text)
        .map_err(|failure| AttestationRefusal::Unreadable(failure.to_string()))?;
    if held.format != POLICY_FORMAT {
        return Err(AttestationRefusal::ForeignFormat(held.format));
    }
    if !ELIGIBLE_VISIBILITIES.contains(&held.repository.visibility.as_str()) {
        return Err(AttestationRefusal::ValueUnacceptable {
            field: "visibility",
            held: held.repository.visibility.clone(),
        });
    }
    if held.validity.accept_expired_certificate {
        return Err(AttestationRefusal::TrustWouldBeChosenByTheVerified(
            "accepting an expired certificate",
        ));
    }
    if held.validity.accept_unpinned_root_update {
        return Err(AttestationRefusal::TrustWouldBeChosenByTheVerified(
            "advancing trust material during a run",
        ));
    }
    if !held.validity.require_integrated_time_within_certificate_validity {
        return Err(AttestationRefusal::TrustWouldBeChosenByTheVerified(
            "ignoring when a certificate was used",
        ));
    }
    Ok(held)
}

/// Requires the committed root to be exactly the snapshot the policy names.
///
/// Done before a bundle is parsed. A bundle read first has already been given
/// the chance to influence what checks it.
///
/// # Errors
///
/// Returns [`AttestationRefusal::Unreadable`] when the snapshot is absent and
/// [`AttestationRefusal::TrustedRootDrift`] when its bytes differ.
pub fn require_trusted_root(
    policy: &ReleaseAttestationPolicy,
    workspace_root: &Path,
) -> Result<(), AttestationRefusal> {
    let held = std::fs::read(workspace_root.join(&policy.trusted_root.path))
        .map_err(|failure| AttestationRefusal::Unreadable(failure.to_string()))?;
    let digest = hex::encode(sha2::Sha256::digest(&held));
    if digest != policy.trusted_root.sha256 {
        return Err(AttestationRefusal::TrustedRootDrift {
            expected: policy.trusted_root.sha256.clone(),
            held: digest,
        });
    }
    let text = String::from_utf8(held)
        .map_err(|failure| AttestationRefusal::Unreadable(failure.to_string()))?;
    let snapshot: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|failure| AttestationRefusal::Unreadable(failure.to_string()))?;
    let media = snapshot["mediaType"].as_str().unwrap_or_default();
    if media != policy.trusted_root.media_type {
        return Err(AttestationRefusal::ValueUnacceptable {
            field: "trusted-root media type",
            held: media.to_owned(),
        });
    }
    Ok(())
}

/// What one bundle's statement says about what it attests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttestedStatement {
    /// The builder the provenance names.
    pub builder_identity: String,
    /// The provenance version.
    pub predicate_type: String,
    /// The repository the source came from.
    pub source_repository_uri: String,
    /// Every subject it attests, by name and digest.
    pub subjects: Vec<(String, String)>,
    /// The statement version.
    pub in_toto_version: String,
    /// The workflow that built it.
    pub workflow_path: String,
}

/// Returns the statement one bundle carries.
///
/// # Errors
///
/// Returns [`AttestationRefusal::BundleUnreadable`] for a bundle whose envelope
/// or statement this cannot read.
pub fn read_statement(
    bundle: &str,
    digest_algorithm: &str,
) -> Result<AttestedStatement, AttestationRefusal> {
    let held: serde_json::Value = serde_json::from_str(bundle)
        .map_err(|failure| AttestationRefusal::BundleUnreadable(failure.to_string()))?;
    let payload = held["dsseEnvelope"]["payload"]
        .as_str()
        .ok_or_else(|| AttestationRefusal::BundleUnreadable("no envelope payload".to_owned()))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|failure| AttestationRefusal::BundleUnreadable(failure.to_string()))?;
    let statement: serde_json::Value = serde_json::from_slice(&decoded)
        .map_err(|failure| AttestationRefusal::BundleUnreadable(failure.to_string()))?;
    let named = |pointer: &str| {
        statement
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let subjects = statement["subject"]
        .as_array()
        .map(|held| {
            held.iter()
                .map(|subject| {
                    (
                        subject["name"].as_str().unwrap_or_default().to_owned(),
                        subject["digest"][digest_algorithm].as_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(AttestedStatement {
        builder_identity: named("/predicate/runDetails/builder/id"),
        predicate_type: named("/predicateType"),
        source_repository_uri: named(
            "/predicate/buildDefinition/externalParameters/workflow/repository",
        ),
        subjects,
        in_toto_version: named("/_type"),
        workflow_path: named("/predicate/buildDefinition/externalParameters/workflow/path"),
    })
}

/// Requires one statement to be the evidence this policy admits.
///
/// # Errors
///
/// Returns [`AttestationRefusal::IdentityUnauthorized`] naming the first field
/// that differs, and [`AttestationRefusal::SubjectsUnexpected`] when the
/// attested subjects are not exactly the expected set.
pub fn require_admissible(
    policy: &ReleaseAttestationPolicy,
    statement: &AttestedStatement,
    expected_subjects: &BTreeSet<String>,
) -> Result<(), AttestationRefusal> {
    let compared: [(&'static str, &str, &str); COMPARED_IDENTITY_FIELDS] = [
        ("statement version", &policy.statement.in_toto_version, &statement.in_toto_version),
        ("provenance version", &policy.statement.predicate_type, &statement.predicate_type),
        (
            "source repository",
            &policy.identity.source_repository_uri,
            &statement.source_repository_uri,
        ),
        ("workflow", &policy.identity.workflow_path, &statement.workflow_path),
        ("builder", &policy.identity.builder_identity, &statement.builder_identity),
    ];
    for (field, expected, held) in compared {
        if expected != held {
            return Err(AttestationRefusal::IdentityUnauthorized {
                expected: expected.to_owned(),
                field,
                held: held.to_owned(),
            });
        }
    }
    let attested: BTreeSet<String> =
        statement.subjects.iter().map(|(name, _)| name.clone()).collect();
    if &attested != expected_subjects {
        return Err(AttestationRefusal::SubjectsUnexpected {
            expected: expected_subjects.iter().cloned().collect::<Vec<String>>().join(", "),
            held: attested.iter().cloned().collect::<Vec<String>>().join(", "),
        });
    }
    if statement.subjects.iter().any(|(_, digest)| digest.is_empty()) {
        return Err(AttestationRefusal::SubjectsUnexpected {
            expected: format!("every subject named by {}", policy.statement.digest_algorithm),
            held: "a subject with no digest under that algorithm".to_owned(),
        });
    }
    Ok(())
}
