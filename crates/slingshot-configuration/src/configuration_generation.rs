//! One committed generation of the configuration root.
//!
//! A configuration is several files that must agree: profiles, an optional
//! selection, the credential and certificate documents those profiles name, and
//! a commit inventory listing every one of them with the digest of its bytes. A
//! writer publishes the inventory last, so an inventory that matches everything
//! it lists is a complete commit.
//!
//! Reading one file stably proves that file did not change while it was read.
//! It does not prove the set is one generation: a writer can replace a source
//! between two of these reads, and each read would still be perfectly stable.
//! The commit inventory closes that gap. It is read before the sources and
//! again afterwards, both readings must be identical, every listed source must
//! hash to what the inventory says, and the set of sources discovered and
//! referenced must equal the set listed - no missing source, no surplus one.
//!
//! The coordinator knows nothing about what a profile means. It hands the
//! profile and selection documents to an injected inspector and asks only for
//! the references they reach, so the same coordinator is proved against a fake
//! inspector with no parser, no credential vocabulary, and no key handling
//! anywhere in the loop.

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};
use slingshot_domain::configuration_snapshot::{
    ConfigurationReference, ConfigurationSnapshot, SourceDigest,
};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};
use slingshot_domain::secret_value::SensitiveConfigurationDocument;

use crate::credential_filesystem::{
    ConfigurationFilesystemAuthority, CredentialFilesystemFailure, StableSource,
};

/// Structural location every generation decision is reported at.
const GENERATION_LOCATION: &str = "configuration_snapshot";

/// Reason one configuration generation could not be accepted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code} at {structural_location}")]
pub struct ConfigurationGenerationFailure {
    /// Stable code from the contract registry.
    pub code: ConfigurationFailureCode,
    /// Manifest vocabulary naming where the failure was found.
    pub structural_location: &'static str,
}

impl ConfigurationGenerationFailure {
    /// Returns one failure at a named structural location.
    #[must_use]
    pub fn at(code: ConfigurationFailureCode, structural_location: &'static str) -> Self {
        Self { code, structural_location }
    }

    /// Returns the one failure every mixed or incomplete generation produces.
    ///
    /// Missing, surplus, changed, unlisted, over-limit, and digest-mismatched
    /// state all report this and nothing else, so no caller can learn which
    /// source disagreed or what its digest was.
    #[must_use]
    pub fn inconsistent() -> Self {
        Self::at(ConfigurationFailureCode::ConfigurationSnapshotInconsistent, GENERATION_LOCATION)
    }
}

impl From<CredentialFilesystemFailure> for ConfigurationGenerationFailure {
    fn from(failure: CredentialFilesystemFailure) -> Self {
        Self { code: failure.code, structural_location: failure.structural_location }
    }
}

/// The role one source plays in a generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceRole {
    /// A profile document below the profile directory.
    Profile,
    /// The optional selection document.
    Selection,
    /// A service-credential document a profile names.
    ServiceCredentials,
    /// A certificate-authority document a profile names.
    AdditionalCertificateAuthority,
}

/// Every role, in the order the manifest's own inventory lists them.
const ROLES_IN_MANIFEST_ORDER: &[SourceRole] = &[
    SourceRole::Profile,
    SourceRole::Selection,
    SourceRole::ServiceCredentials,
    SourceRole::AdditionalCertificateAuthority,
];

impl SourceRole {
    /// Returns the manifest literal this role is written as.
    ///
    /// # Panics
    ///
    /// Panics when the manifest lists fewer roles than this enumeration has,
    /// which the contract's own inventory assertions rule out.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        let roles = &ProfileAuthenticationContract::embedded().literals.source_roles;
        let position = ROLES_IN_MANIFEST_ORDER
            .iter()
            .position(|role| *role == self)
            .expect("every role is in the order");
        roles[position].as_str()
    }

    /// Returns the byte bound a document in this role may occupy.
    #[must_use]
    pub fn document_bound(self) -> u64 {
        let limits = &ProfileAuthenticationContract::embedded().limits;
        match self {
            Self::Profile => limits.maximum_profile_document_bytes,
            Self::Selection => limits.maximum_selection_document_bytes,
            Self::ServiceCredentials => limits.maximum_service_credential_document_bytes,
            Self::AdditionalCertificateAuthority => {
                limits.maximum_additional_certificate_authority_document_bytes
            }
        }
    }
}

/// One reference and the role the inspector assigned it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RoleTaggedReference {
    /// Reference the role was assigned to.
    pub reference: ConfigurationReference,
    /// Role the reference plays.
    pub role: SourceRole,
}

/// One retained source and the role it plays.
#[derive(Debug)]
pub struct RoleTaggedSource {
    /// Reference the source was read from.
    pub reference: ConfigurationReference,
    /// Role the source plays.
    pub role: SourceRole,
    /// The source bytes, still opaque to everything but its own parser.
    pub document: SensitiveConfigurationDocument,
}

/// The profile and selection documents the inspector is given.
#[derive(Debug)]
pub struct InspectedDocuments<'documents> {
    /// Every profile document, in reference order.
    pub profiles:
        Vec<(&'documents ConfigurationReference, &'documents SensitiveConfigurationDocument)>,
    /// The selection document, when the generation carries one.
    pub selection:
        Option<(&'documents ConfigurationReference, &'documents SensitiveConfigurationDocument)>,
}

/// What one inspection produced.
#[derive(Debug)]
pub struct InspectionOutcome<Inspection> {
    /// Typed values the inspector derived.
    pub inspection: Inspection,
    /// Every reference reachable from those documents, with its role.
    pub references: Vec<RoleTaggedReference>,
}

/// Derives the role inventory from the profile and selection documents.
///
/// The coordinator holds this as an injected value so it never learns how a
/// profile is written. A fake implementation proves that seam is real.
pub trait ConfigurationSourceInventoryInspector<Inspection> {
    /// Returns the typed inspection and the complete role inventory.
    ///
    /// # Errors
    ///
    /// Returns the contract code the document itself failed with.
    fn inspect(
        &self,
        documents: &InspectedDocuments<'_>,
    ) -> Result<InspectionOutcome<Inspection>, ConfigurationGenerationFailure>;
}

/// One complete committed generation.
#[derive(Debug)]
pub struct VerifiedConfigurationGeneration<Inspection> {
    /// Typed values the inspector derived from the profiles and selection.
    pub inspection: Inspection,
    /// Every retained source, ordered by reference bytes.
    pub sources: Vec<RoleTaggedSource>,
}

impl<Inspection> VerifiedConfigurationGeneration<Inspection> {
    /// Returns the sources playing one role.
    #[must_use]
    pub fn sources_in_role(&self, role: SourceRole) -> Vec<&RoleTaggedSource> {
        self.sources.iter().filter(|source| source.role == role).collect()
    }
}

/// Reads one complete committed generation, or nothing.
#[derive(Debug)]
pub struct ConfigurationGenerationCoordinator<Authority> {
    /// Authority every source is read through.
    authority: Authority,
}

impl<Authority: ConfigurationFilesystemAuthority> ConfigurationGenerationCoordinator<Authority> {
    /// Returns a coordinator reading through `authority`.
    #[must_use]
    pub fn new(authority: Authority) -> Self {
        Self { authority }
    }

    /// Reads one complete committed generation.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationSnapshotInconsistent`]
    /// when every attempt found a missing, surplus, changed, unlisted,
    /// over-limit, or digest-mismatched state, and the filesystem authority's
    /// own code when a source could not be read safely at all.
    pub fn read_generation<Inspection>(
        &self,
        inspector: &dyn ConfigurationSourceInventoryInspector<Inspection>,
    ) -> Result<VerifiedConfigurationGeneration<Inspection>, ConfigurationGenerationFailure> {
        let attempts = ProfileAuthenticationContract::embedded()
            .limits
            .maximum_configuration_generation_attempts;
        let mut refusal = ConfigurationGenerationFailure::inconsistent();
        for _ in 0..attempts {
            match self.attempt_generation(inspector) {
                Ok(generation) => return Ok(generation),
                Err(failure)
                    if failure.code
                        == ConfigurationFailureCode::ConfigurationSnapshotInconsistent =>
                {
                    refusal = failure;
                }
                Err(failure) => return Err(failure),
            }
        }
        Err(refusal)
    }

    /// Makes one complete attempt at reading a generation.
    fn attempt_generation<Inspection>(
        &self,
        inspector: &dyn ConfigurationSourceInventoryInspector<Inspection>,
    ) -> Result<VerifiedConfigurationGeneration<Inspection>, ConfigurationGenerationFailure> {
        let first = self.read_inventory()?;
        let listed = ConfigurationSnapshot::parse(&first).map_err(|failure| {
            ConfigurationGenerationFailure {
                code: failure.code,
                structural_location: failure.structural_location,
            }
        })?;
        let discovered = self.discover_profiles()?;
        let selection = self.observe_selection()?;
        let mut retained = self.retain_listed_sources(&listed)?;
        let outcome =
            inspector.inspect(&inspected_documents(&retained, &discovered, &selection))?;
        let expected = expected_inventory(&discovered, selection.as_ref(), &outcome.references)?;
        verify_inventory(&listed, &expected, &retained)?;
        if self.read_inventory()? != first {
            return Err(ConfigurationGenerationFailure::inconsistent());
        }
        let mut sources = Vec::with_capacity(expected.len());
        for tagged in expected {
            let source = retained
                .remove(&tagged.reference)
                .ok_or_else(ConfigurationGenerationFailure::inconsistent)?;
            sources.push(RoleTaggedSource {
                reference: tagged.reference,
                role: tagged.role,
                document: source.document,
            });
        }
        Ok(VerifiedConfigurationGeneration { inspection: outcome.inspection, sources })
    }

    /// Reads the commit inventory as text.
    fn read_inventory(&self) -> Result<String, ConfigurationGenerationFailure> {
        let contract = ProfileAuthenticationContract::embedded();
        let name = contract.literals.configuration_snapshot_file_name.as_str();
        let source = self
            .authority
            .read_source(&[name], contract.limits.maximum_configuration_snapshot_document_bytes)?;
        source.document.lend_text_for_parsing(str::to_owned).map_err(|_| {
            ConfigurationGenerationFailure::at(
                ConfigurationFailureCode::ConfigurationDocumentNotUtf8,
                GENERATION_LOCATION,
            )
        })
    }

    /// Returns every profile reference the profile directory holds.
    fn discover_profiles(
        &self,
    ) -> Result<BTreeSet<ConfigurationReference>, ConfigurationGenerationFailure> {
        let literals = &ProfileAuthenticationContract::embedded().literals;
        let limits = &ProfileAuthenticationContract::embedded().limits;
        let directory = literals.profile_directory_name.as_str();
        let entries = self
            .authority
            .list_directory(&[directory], limits.maximum_profile_directory_entries)?;
        if u64::try_from(entries.len()).unwrap_or(u64::MAX) > limits.maximum_profile_documents {
            return Err(ConfigurationGenerationFailure::at(
                ConfigurationFailureCode::ConfigurationDirectoryLimitExceeded,
                GENERATION_LOCATION,
            ));
        }
        let mut discovered = BTreeSet::new();
        for entry in entries {
            if !entry.ordinary_file
                || u64::try_from(entry.name.len()).unwrap_or(u64::MAX)
                    > limits.maximum_profile_file_name_bytes
            {
                return Err(ConfigurationGenerationFailure::inconsistent());
            }
            let reference = ConfigurationReference::parse(&format!("{directory}/{}", entry.name))
                .map_err(|_| ConfigurationGenerationFailure::inconsistent())?;
            if !entry.name.ends_with(literals.profile_file_name_suffix.as_str()) {
                return Err(ConfigurationGenerationFailure::inconsistent());
            }
            discovered.insert(reference);
        }
        Ok(discovered)
    }

    /// Observes whether the optional selection document is present.
    fn observe_selection(
        &self,
    ) -> Result<Option<ConfigurationReference>, ConfigurationGenerationFailure> {
        let name = ProfileAuthenticationContract::embedded().literals.selection_file_name.clone();
        if !self.authority.observe_presence(&[name.as_str()])? {
            return Ok(None);
        }
        ConfigurationReference::parse(&name)
            .map(Some)
            .map_err(|_| ConfigurationGenerationFailure::inconsistent())
    }

    /// Reads and verifies every source the inventory lists.
    fn retain_listed_sources(
        &self,
        listed: &ConfigurationSnapshot,
    ) -> Result<BTreeMap<ConfigurationReference, StableSource>, ConfigurationGenerationFailure>
    {
        let limits = &ProfileAuthenticationContract::embedded().limits;
        let mut retained = BTreeMap::new();
        let mut aggregate: u64 = 0;
        for entry in listed.sources() {
            let components: Vec<&str> = entry.reference.components().collect();
            let source = self
                .authority
                .read_source(&components, limits.maximum_configuration_source_document_bytes)?;
            aggregate = aggregate
                .checked_add(source.length)
                .filter(|total| *total <= limits.maximum_configuration_generation_source_bytes)
                .ok_or_else(ConfigurationGenerationFailure::inconsistent)?;
            let observed = source.document.lend_bytes_for_digest(digest_of);
            if !entry.digest.matches(&observed) {
                return Err(ConfigurationGenerationFailure::inconsistent());
            }
            retained.insert(entry.reference.clone(), source);
        }
        Ok(retained)
    }
}

/// Returns the profile and selection documents the inspector is given.
fn inspected_documents<'documents>(
    retained: &'documents BTreeMap<ConfigurationReference, StableSource>,
    discovered: &BTreeSet<ConfigurationReference>,
    selection: &'documents Option<ConfigurationReference>,
) -> InspectedDocuments<'documents> {
    InspectedDocuments {
        profiles: retained
            .iter()
            .filter(|(reference, _)| discovered.contains(*reference))
            .map(|(reference, source)| (reference, &source.document))
            .collect(),
        selection: selection.as_ref().and_then(|reference| {
            retained.get(reference).map(|source| (reference, &source.document))
        }),
    }
}

/// Returns the inventory the discovered and referenced sources imply.
fn expected_inventory(
    discovered: &BTreeSet<ConfigurationReference>,
    selection: Option<&ConfigurationReference>,
    referenced: &[RoleTaggedReference],
) -> Result<Vec<RoleTaggedReference>, ConfigurationGenerationFailure> {
    let mut expected: BTreeMap<ConfigurationReference, SourceRole> = BTreeMap::new();
    for reference in discovered {
        expected.insert(reference.clone(), SourceRole::Profile);
    }
    if let Some(reference) = selection
        && expected.insert(reference.clone(), SourceRole::Selection).is_some()
    {
        return Err(ConfigurationGenerationFailure::inconsistent());
    }
    for tagged in referenced {
        match expected.get(&tagged.reference) {
            Some(existing) if *existing != tagged.role => {
                return Err(ConfigurationGenerationFailure::inconsistent());
            }
            Some(_) => {}
            None => {
                expected.insert(tagged.reference.clone(), tagged.role);
            }
        }
    }
    Ok(expected
        .into_iter()
        .map(|(reference, role)| RoleTaggedReference { reference, role })
        .collect())
}

/// Requires the listed inventory and the expected one to be the same set, and
/// every retained source to fit the bound its role carries.
fn verify_inventory(
    listed: &ConfigurationSnapshot,
    expected: &[RoleTaggedReference],
    retained: &BTreeMap<ConfigurationReference, StableSource>,
) -> Result<(), ConfigurationGenerationFailure> {
    let listed_set: BTreeSet<&ConfigurationReference> =
        listed.sources().iter().map(|source| &source.reference).collect();
    let expected_set: BTreeSet<&ConfigurationReference> =
        expected.iter().map(|tagged| &tagged.reference).collect();
    if listed_set != expected_set {
        return Err(ConfigurationGenerationFailure::inconsistent());
    }
    for tagged in expected {
        let source = retained
            .get(&tagged.reference)
            .ok_or_else(ConfigurationGenerationFailure::inconsistent)?;
        if source.length > tagged.role.document_bound() {
            return Err(ConfigurationGenerationFailure::at(
                ConfigurationFailureCode::ConfigurationDocumentTooLarge,
                GENERATION_LOCATION,
            ));
        }
    }
    Ok(())
}

/// Returns the digest of one source's bytes.
///
/// The value is secret-adjacent: a source can carry a low-entropy secret, so
/// this digest never leaves the comparison it was computed for.
fn digest_of(bytes: &[u8]) -> SourceDigest {
    /// Bytes one digest occupies.
    const DIGEST_BYTES: usize = 32;

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let mut digest = [0; DIGEST_BYTES];
    digest.copy_from_slice(&hasher.finalize());
    SourceDigest::from_raw(digest)
}
