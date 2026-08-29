//! Deterministic enumeration and parsing of profile documents.
//!
//! A directory hands out its entries in whatever order it likes, and that order
//! must not reach anything a caller can see. Two machines that hold the same
//! files therefore load the same profiles, in the same order, and report the
//! same failures, because everything here is keyed and sorted by value rather
//! than by the order a source was read in.
//!
//! Failures become a closed diagnostic before they leave: a manifest source
//! class, a stage, a structural location from the manifest's own vocabulary, a
//! stable code, and how many times that exact tuple occurred. Nothing else
//! survives. A parser message quotes source bytes, an unknown key is source
//! bytes, and a source reference orders those bytes - each would turn a
//! diagnostic into a way to read a file the reader was never given.
//!
//! The inspector this module supplies parses profile and selection documents
//! and nothing else. It reports which credential and certificate documents the
//! profiles reach, and leaves those documents opaque; whether the generation is
//! whole is the coordinator's decision, and it is made after this runs.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use slingshot_domain::configuration_snapshot::ConfigurationReference;
use slingshot_domain::profile::{
    Environment, EnvironmentAuthentication, InsecureAuthorTransportWarning, Profile, ProfileName,
    SelectionDocument,
};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};

use crate::configuration_generation::{
    ConfigurationGenerationCoordinator, ConfigurationGenerationFailure,
    ConfigurationSourceInventoryInspector, InspectedDocuments, InspectionOutcome,
    RoleTaggedReference, SourceRole,
};
use crate::credential_filesystem::ConfigurationFilesystemAuthority;

/// Source class a public diagnostic may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticSourceClass {
    /// The configuration root itself.
    ConfigurationRoot,
    /// The commit inventory.
    ConfigurationSnapshot,
    /// A profile document.
    Profile,
    /// The selection document.
    Selection,
    /// A service-credential document.
    ServiceCredentials,
    /// A certificate-authority document.
    AdditionalCertificateAuthority,
    /// The platform trust store.
    PlatformTrust,
}

/// Every source class, in the order the manifest's own inventory lists them.
const CLASSES_IN_MANIFEST_ORDER: &[DiagnosticSourceClass] = &[
    DiagnosticSourceClass::ConfigurationRoot,
    DiagnosticSourceClass::ConfigurationSnapshot,
    DiagnosticSourceClass::Profile,
    DiagnosticSourceClass::Selection,
    DiagnosticSourceClass::ServiceCredentials,
    DiagnosticSourceClass::AdditionalCertificateAuthority,
    DiagnosticSourceClass::PlatformTrust,
];

impl DiagnosticSourceClass {
    /// Returns the manifest literal this class is written as.
    ///
    /// # Panics
    ///
    /// Panics when the manifest lists fewer classes than this enumeration has,
    /// which the contract's own inventory assertions rule out.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        let classes = &ProfileAuthenticationContract::embedded().literals.diagnostic_source_classes;
        let position = CLASSES_IN_MANIFEST_ORDER
            .iter()
            .position(|class| *class == self)
            .expect("every class is in the order");
        classes[position].as_str()
    }
}

/// Stage a public diagnostic may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticStage {
    /// Resolving the configuration root.
    RootResolution,
    /// Deciding whether a source may be opened at all.
    FilesystemAuthority,
    /// Tokenizing a document.
    DocumentSyntax,
    /// Matching a document against its closed member inventory.
    DocumentShape,
    /// Deciding whether the values a document holds are usable.
    DocumentSemantics,
    /// Deciding whether the set of sources is one generation.
    SourceInventory,
    /// Resolving one profile and environment.
    Selection,
    /// Building the immutable selected-environment snapshot.
    SnapshotConstruction,
}

/// Every stage, in the order the manifest's own inventory lists them.
const STAGES_IN_MANIFEST_ORDER: &[DiagnosticStage] = &[
    DiagnosticStage::RootResolution,
    DiagnosticStage::FilesystemAuthority,
    DiagnosticStage::DocumentSyntax,
    DiagnosticStage::DocumentShape,
    DiagnosticStage::DocumentSemantics,
    DiagnosticStage::SourceInventory,
    DiagnosticStage::Selection,
    DiagnosticStage::SnapshotConstruction,
];

impl DiagnosticStage {
    /// Returns the manifest literal this stage is written as.
    ///
    /// # Panics
    ///
    /// Panics when the manifest lists fewer stages than this enumeration has,
    /// which the contract's own inventory assertions rule out.
    #[must_use]
    pub fn as_text(self) -> &'static str {
        let stages = &ProfileAuthenticationContract::embedded().literals.diagnostic_stages;
        let position = STAGES_IN_MANIFEST_ORDER
            .iter()
            .position(|stage| *stage == self)
            .expect("every stage is in the order");
        stages[position].as_str()
    }
}

/// One public configuration failure.
///
/// The tuple is the whole of it. A caller learns which class of source failed,
/// at which stage, at which structural location the manifest already names, why,
/// and how often - and nothing that would let them read a file they were not
/// given.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConfigurationDiagnostic {
    /// Class of source the failure was found in.
    pub source_class: DiagnosticSourceClass,
    /// Stage the failure was found at.
    pub stage: DiagnosticStage,
    /// Manifest vocabulary naming where in the document it was found.
    pub structural_location: &'static str,
    /// Stable code from the contract registry.
    pub code: ConfigurationFailureCode,
    /// How many times this exact tuple occurred.
    pub occurrences: u32,
}

impl ConfigurationDiagnostic {
    /// Returns one occurrence of a diagnostic.
    #[must_use]
    pub fn once(
        source_class: DiagnosticSourceClass,
        stage: DiagnosticStage,
        structural_location: &'static str,
        code: ConfigurationFailureCode,
    ) -> Self {
        Self { source_class, stage, structural_location, code, occurrences: 1 }
    }
}

/// Coalesces, orders, and bounds a set of diagnostics.
///
/// Identical tuples become one entry with a checked count, distinct tuples are
/// ordered by the manifest's own class and stage order and then by location and
/// code, and a set larger than the contract allows keeps its first entries and
/// reports how many it dropped. Ordering by value rather than by the order
/// failures were found is what keeps the result from revealing which source was
/// read first.
///
/// # Panics
///
/// Panics when one tuple occurs more times than a counter can hold, which no
/// bounded generation can reach.
#[must_use]
pub fn summarize(found: Vec<ConfigurationDiagnostic>) -> Vec<ConfigurationDiagnostic> {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let mut counted: BTreeMap<ConfigurationDiagnostic, u32> = BTreeMap::new();
    for diagnostic in found {
        let occurrences = diagnostic.occurrences;
        let key = ConfigurationDiagnostic { occurrences: 1, ..diagnostic };
        let total = counted.entry(key).or_insert(0);
        *total = total.checked_add(occurrences).expect("a bounded generation cannot overflow");
    }
    let mut distinct: Vec<ConfigurationDiagnostic> = counted
        .into_iter()
        .map(|(diagnostic, occurrences)| ConfigurationDiagnostic { occurrences, ..diagnostic })
        .collect();
    let retained = usize::try_from(limits.retained_configuration_diagnostics).unwrap_or(usize::MAX);
    let maximum = usize::try_from(limits.maximum_configuration_diagnostics).unwrap_or(usize::MAX);
    if distinct.len() <= maximum {
        return distinct;
    }
    let dropped = u32::try_from(distinct.len() - retained).expect("the count fits");
    distinct.truncate(retained);
    let marker = &ProfileAuthenticationContract::embedded().literals.diagnostic_truncation_marker;
    distinct.push(ConfigurationDiagnostic {
        source_class: DiagnosticSourceClass::ConfigurationSnapshot,
        stage: DiagnosticStage::SourceInventory,
        structural_location: "diagnostics",
        code: ConfigurationFailureCode::ConfigurationDiagnosticsTruncated,
        occurrences: dropped,
    });
    debug_assert_eq!(marker.structural_location, "diagnostics");
    distinct
}

/// Profile and selection documents one generation carried.
#[derive(Debug)]
pub struct InspectedProfiles {
    /// Each profile document, by the reference it was read from.
    profiles: Vec<(ConfigurationReference, Profile)>,
    /// The selection document, when the generation carried one.
    selection: Option<(ConfigurationReference, SelectionDocument)>,
}

/// Parses profile and selection documents and reports what they reach.
///
/// It parses nothing else. A credential and a certificate stay opaque bytes
/// here, because deciding they are well formed is a decision about a selected
/// environment, and no environment has been selected yet.
#[derive(Debug, Default)]
pub struct ProfileDocumentInspector {
    /// Failures found while parsing, in the order they were found.
    found: RefCell<Vec<ConfigurationDiagnostic>>,
}

impl ProfileDocumentInspector {
    /// Returns an inspector that has found nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the coalesced, ordered, bounded diagnostics found so far.
    #[must_use]
    pub fn diagnostics(&self) -> Vec<ConfigurationDiagnostic> {
        summarize(self.found.borrow().clone())
    }

    /// Records one failure and returns the generation failure it implies.
    fn record(
        &self,
        source_class: DiagnosticSourceClass,
        stage: DiagnosticStage,
        structural_location: &'static str,
        code: ConfigurationFailureCode,
    ) -> ConfigurationGenerationFailure {
        self.found.borrow_mut().push(ConfigurationDiagnostic::once(
            source_class,
            stage,
            structural_location,
            code,
        ));
        ConfigurationGenerationFailure::at(code, structural_location)
    }

    /// Returns the stage one document failure belongs to.
    fn stage_of(code: ConfigurationFailureCode) -> DiagnosticStage {
        match code {
            ConfigurationFailureCode::ConfigurationDocumentSyntaxInvalid => {
                DiagnosticStage::DocumentSyntax
            }
            ConfigurationFailureCode::ConfigurationDocumentShapeInvalid => {
                DiagnosticStage::DocumentShape
            }
            _ => DiagnosticStage::DocumentSemantics,
        }
    }
}

impl ConfigurationSourceInventoryInspector<InspectedProfiles> for ProfileDocumentInspector {
    fn inspect(
        &self,
        documents: &InspectedDocuments<'_>,
    ) -> Result<InspectionOutcome<InspectedProfiles>, ConfigurationGenerationFailure> {
        let mut profiles = Vec::with_capacity(documents.profiles.len());
        let mut references = Vec::new();
        for (reference, document) in &documents.profiles {
            let parsed = document
                .lend_text_for_parsing(Profile::parse)
                .map_err(|_| {
                    self.record(
                        DiagnosticSourceClass::Profile,
                        DiagnosticStage::DocumentSyntax,
                        "profile",
                        ConfigurationFailureCode::ConfigurationDocumentNotUtf8,
                    )
                })?
                .map_err(|failure| {
                    self.record(
                        DiagnosticSourceClass::Profile,
                        Self::stage_of(failure.code),
                        failure.structural_location,
                        failure.code,
                    )
                })?;
            references.extend(reachable_references(&parsed));
            profiles.push(((*reference).clone(), parsed));
        }
        let selection = self.inspect_selection(documents)?;
        deduplicate(&mut references);
        Ok(InspectionOutcome { inspection: InspectedProfiles { profiles, selection }, references })
    }
}

impl ProfileDocumentInspector {
    /// Parses the optional selection document.
    fn inspect_selection(
        &self,
        documents: &InspectedDocuments<'_>,
    ) -> Result<Option<(ConfigurationReference, SelectionDocument)>, ConfigurationGenerationFailure>
    {
        let Some((reference, document)) = documents.selection else {
            return Ok(None);
        };
        let parsed = document
            .lend_text_for_parsing(SelectionDocument::parse)
            .map_err(|_| {
                self.record(
                    DiagnosticSourceClass::Selection,
                    DiagnosticStage::DocumentSyntax,
                    "selection",
                    ConfigurationFailureCode::ConfigurationDocumentNotUtf8,
                )
            })?
            .map_err(|failure| {
                self.record(
                    DiagnosticSourceClass::Selection,
                    Self::stage_of(failure.code),
                    failure.structural_location,
                    failure.code,
                )
            })?;
        Ok(Some((reference.clone(), parsed)))
    }
}

/// Every profile one complete generation yielded, with what selected them.
#[derive(Debug)]
pub struct LoadedProfiles {
    /// Profiles by the name each declared, which no two may share.
    profiles: BTreeMap<ProfileName, Profile>,
    /// The reference each profile was read from.
    sources: BTreeMap<ProfileName, ConfigurationReference>,
    /// The selection document, when the generation carried one.
    selection: Option<(ConfigurationReference, SelectionDocument)>,
}

impl LoadedProfiles {
    /// Returns the profiles, ordered by the name each declared.
    #[must_use]
    pub fn profiles(&self) -> &BTreeMap<ProfileName, Profile> {
        &self.profiles
    }

    /// Returns the reference `name` was read from.
    #[must_use]
    pub fn source_of(&self, name: &ProfileName) -> Option<&ConfigurationReference> {
        self.sources.get(name)
    }

    /// Returns the selection document, when the generation carried one.
    #[must_use]
    pub fn selection(&self) -> Option<&SelectionDocument> {
        self.selection.as_ref().map(|(_, document)| document)
    }

    /// Returns the reference the selection was read from.
    #[must_use]
    pub fn selection_source(&self) -> Option<&ConfigurationReference> {
        self.selection.as_ref().map(|(reference, _)| reference)
    }

    /// Returns the cleartext-author warnings each profile carries.
    ///
    /// The result is keyed and ordered by profile name, so two machines holding
    /// the same files report the same warnings in the same order whatever order
    /// their directories enumerated in.
    #[must_use]
    pub fn insecure_author_warnings(
        &self,
    ) -> BTreeMap<ProfileName, Vec<InsecureAuthorTransportWarning>> {
        self.profiles
            .iter()
            .map(|(name, profile)| {
                let warnings = profile
                    .environments()
                    .values()
                    .filter_map(Environment::insecure_author_transport_warning)
                    .collect();
                (name.clone(), warnings)
            })
            .collect()
    }
}

/// Loads every profile of one complete committed generation.
///
/// # Errors
///
/// Returns the coalesced, ordered, bounded diagnostics of everything that
/// failed. A generation that is not whole reports that and nothing about which
/// source disagreed.
pub fn load_profiles<Authority: ConfigurationFilesystemAuthority>(
    authority: Authority,
) -> Result<LoadedProfiles, Vec<ConfigurationDiagnostic>> {
    let inspector = ProfileDocumentInspector::new();
    let coordinator = ConfigurationGenerationCoordinator::new(authority);
    let generation = match coordinator.read_generation(&inspector) {
        Ok(generation) => generation,
        Err(failure) => {
            let mut found = inspector.found.borrow().clone();
            if found.is_empty() {
                found.push(ConfigurationDiagnostic::once(
                    DiagnosticSourceClass::ConfigurationSnapshot,
                    stage_of_generation(failure.code),
                    failure.structural_location,
                    failure.code,
                ));
            }
            return Err(summarize(found));
        }
    };
    let mut profiles = BTreeMap::new();
    let mut sources = BTreeMap::new();
    let mut duplicates = Vec::new();
    for (reference, profile) in generation.inspection.profiles {
        let name = profile.name().clone();
        if profiles.insert(name.clone(), profile).is_some() {
            duplicates.push(ConfigurationDiagnostic::once(
                DiagnosticSourceClass::Profile,
                DiagnosticStage::SourceInventory,
                "name",
                ConfigurationFailureCode::ProfileNameDuplicate,
            ));
        }
        sources.insert(name, reference);
    }
    if !duplicates.is_empty() {
        return Err(summarize(duplicates));
    }
    Ok(LoadedProfiles { profiles, sources, selection: generation.inspection.selection })
}

/// Returns the stage one generation failure belongs to.
fn stage_of_generation(code: ConfigurationFailureCode) -> DiagnosticStage {
    match code {
        ConfigurationFailureCode::ConfigurationRootUnsafe => DiagnosticStage::RootResolution,
        ConfigurationFailureCode::ConfigurationFileUnsafe
        | ConfigurationFailureCode::ConfigurationFileChangedDuringRead
        | ConfigurationFailureCode::ConfigurationDocumentTooLarge => {
            DiagnosticStage::FilesystemAuthority
        }
        _ => DiagnosticStage::SourceInventory,
    }
}

/// Returns every credential and certificate reference one profile reaches.
fn reachable_references(profile: &Profile) -> Vec<RoleTaggedReference> {
    let mut reached = Vec::new();
    for environment in profile.environments().values() {
        if let EnvironmentAuthentication::DeveloperConsoleServiceCredentialsFile {
            credentials_file,
        } = environment.authentication()
        {
            reached.push(RoleTaggedReference {
                reference: credentials_file.clone(),
                role: SourceRole::ServiceCredentials,
            });
        }
        if let Some(certificate) = environment.additional_certificate_authority_file() {
            reached.push(RoleTaggedReference {
                reference: certificate.clone(),
                role: SourceRole::AdditionalCertificateAuthority,
            });
        }
    }
    reached
}

/// Removes a reference reused in the same role, keeping one entry.
///
/// Two profiles may legitimately name the same credential document. Naming it
/// in two different roles is not a duplicate but a contradiction, and the
/// coordinator refuses that rather than resolving it here.
fn deduplicate(references: &mut Vec<RoleTaggedReference>) {
    let mut seen = BTreeSet::new();
    references.retain(|tagged| seen.insert(tagged.clone()));
    references.sort();
}
