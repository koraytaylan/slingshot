//! Assertions for loading profiles independently of enumeration order.
//!
//! A directory hands out its entries in whatever order it likes. The fixture is
//! built so that order matters if anything depends on it: the file names sort in
//! the opposite order to the names the documents declare. The same root is then
//! loaded twice, once through an authority that reverses every listing, and both
//! loads must produce the same profiles, in the same order, with the same
//! warnings.
//!
//! The other half is what a failure is allowed to say. Every diagnostic here is
//! scanned for the sentinels the fixtures carry - a password, a credential
//! reference, a profile name - because a diagnostic that named any of them
//! would be a way to read a file the reader was never given.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use slingshot_configuration::credential_filesystem::{
    ConfigurationFilesystemAuthority, CredentialFilesystemFailure, DirectoryEntry, StableSource,
};
use slingshot_configuration::profile_loader::{
    ConfigurationDiagnostic, DiagnosticSourceClass, DiagnosticStage, load_profiles, summarize,
};
use slingshot_configuration::testing::credential_filesystem::ScriptedFilesystem;
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};

/// Directory holding the committed profile directories.
const DIRECTORY_FIXTURES: &str = "../slingshot-test-support/fixtures/profile-directories";

/// Characters of a rendered line that hold the profile name.
const NAME_WIDTH: usize = "alpha".len();

/// Times the coalescing assertion repeats one diagnostic.
const REPEATED_OCCURRENCES: u32 = 3;

/// Sentinels no diagnostic may carry.
const SENTINELS: &[&str] =
    &["not-a-real-password", "alpha-site", "zulu-site", "credentials/alpha.json", "admin"];

/// An authority that hands out every listing in the opposite order.
///
/// Nothing a caller sees may depend on this, which is exactly what the
/// comparison below is for.
struct ReversedListing<Inner> {
    /// Authority every decision is delegated to.
    inner: Inner,
}

impl<Inner: ConfigurationFilesystemAuthority> ConfigurationFilesystemAuthority
    for ReversedListing<Inner>
{
    fn verify_root(&self) -> Result<(), CredentialFilesystemFailure> {
        self.inner.verify_root()
    }

    fn list_directory(
        &self,
        components: &[&str],
        maximum_entries: u64,
    ) -> Result<Vec<DirectoryEntry>, CredentialFilesystemFailure> {
        let mut entries = self.inner.list_directory(components, maximum_entries)?;
        entries.reverse();
        Ok(entries)
    }

    fn observe_presence(&self, components: &[&str]) -> Result<bool, CredentialFilesystemFailure> {
        self.inner.observe_presence(components)
    }

    fn read_source(
        &self,
        components: &[&str],
        maximum_bytes: u64,
    ) -> Result<StableSource, CredentialFilesystemFailure> {
        self.inner.read_source(components, maximum_bytes)
    }
}

/// Returns the files one committed profile directory holds.
fn fixture_files(case: &str) -> BTreeMap<String, Vec<u8>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DIRECTORY_FIXTURES).join(case);
    let mut files = BTreeMap::new();
    collect(&directory, &directory, &mut files);
    files
}

/// Collects every file below `directory`, keyed by its root-relative spelling.
fn collect(root: &Path, directory: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    for entry in std::fs::read_dir(directory).expect("the fixture directory reads") {
        let path = entry.expect("the entry reads").path();
        if path.is_dir() {
            collect(root, &path, files);
            continue;
        }
        let relative = path.strip_prefix(root).expect("the file is inside the fixture");
        let spelling = relative.to_str().expect("the path is text").replace('\\', "/");
        files.insert(spelling, std::fs::read(&path).expect("the file reads"));
    }
}

/// Returns a scripted root holding one committed profile directory.
fn scripted(case: &str) -> ScriptedFilesystem {
    let mut authority = ScriptedFilesystem::new();
    for (reference, bytes) in fixture_files(case) {
        authority = authority.with_source(&reference, &bytes);
    }
    authority.with_directory("profiles")
}

/// Renders what one load produced, so two loads can be compared exactly.
fn rendered(case: &str, reversed: bool) -> String {
    let profiles = if reversed {
        load_profiles(ReversedListing { inner: scripted(case) })
    } else {
        load_profiles(scripted(case))
    }
    .expect("the committed root loads");
    let mut rendering = String::new();
    for (name, profile) in profiles.profiles() {
        let source = profiles.source_of(name).expect("every profile names its source");
        rendering.push_str(&format!("{name} <- {source}\n"));
        for (environment, definition) in profile.environments() {
            rendering.push_str(&format!(
                "  {environment} {} author={} publisher={} warning={}\n",
                definition.deployment().as_text(),
                definition.author_connection_target(),
                definition.publisher_metadata(),
                definition.insecure_author_transport_warning().is_some()
            ));
        }
    }
    let selection = profiles
        .selection()
        .map(|document| format!("{} {}", document.profile(), document.environment()));
    rendering.push_str(&format!("selection={selection:?}\n"));
    rendering.push_str(&format!("warnings={:?}\n", profiles.insecure_author_warnings()));
    rendering
}

#[test]
fn both_enumeration_orders_produce_one_identical_result() {
    let forward = rendered("ordered", false);
    let reversed = rendered("ordered", true);
    assert_eq!(forward, reversed, "the listing order reached the result");
    assert!(forward.contains("alpha-site <- profiles/zulu.toml"), "{forward}");
    assert!(forward.contains("zulu-site <- profiles/alpha.toml"), "{forward}");
    let names: Vec<&str> = forward
        .lines()
        .filter(|line| line.contains(" <- "))
        .map(|line| &line[..NAME_WIDTH])
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "the profiles are not ordered by declared name");
}

#[test]
fn an_opted_in_cleartext_author_carries_exactly_one_stable_warning() {
    let rendering = rendered("ordered", false);
    let warned: Vec<&str> =
        rendering.lines().filter(|line| line.contains("warning=true")).collect();
    assert_eq!(warned.len(), 1, "{rendering}");
    assert!(warned[0].contains("author=http://author.example.com"), "{warned:?}");
    let protected: Vec<&str> = rendering
        .lines()
        .filter(|line| line.contains("author=https://"))
        .filter(|line| line.contains("warning=true"))
        .collect();
    assert!(protected.is_empty(), "a protected author carried a warning");
}

#[test]
fn two_profiles_declaring_one_name_are_refused_without_naming_either() {
    let diagnostics = load_profiles(scripted("duplicate-name")).expect_err("two names are refused");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, ConfigurationFailureCode::ProfileNameDuplicate);
    assert_eq!(diagnostics[0].source_class, DiagnosticSourceClass::Profile);
    assert_eq!(diagnostics[0].stage, DiagnosticStage::SourceInventory);
    assert_eq!(diagnostics[0].occurrences, 1);
    refuse_sentinels(&diagnostics);
}

#[test]
fn a_credential_document_is_never_parsed_while_the_profiles_load() {
    let mut authority = ScriptedFilesystem::new();
    for (reference, bytes) in fixture_files("ordered") {
        let bytes = if reference.starts_with("credentials/") {
            b"this is not a credential document at all".to_vec()
        } else {
            bytes
        };
        authority = authority.with_source(&reference, &bytes);
    }
    let files = fixture_files("ordered");
    let inventory =
        rebuilt_inventory(&files, "credentials/", b"this is not a credential document at all");
    let authority = authority
        .with_source("configuration-snapshot.toml", inventory.as_bytes())
        .with_directory("profiles");
    load_profiles(authority).expect("an unselected credential stays opaque");
}

#[test]
fn diagnostics_coalesce_and_truncate_exactly_as_the_contract_says() {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let retained = usize::try_from(limits.retained_configuration_diagnostics).expect("it fits");
    let maximum = usize::try_from(limits.maximum_configuration_diagnostics).expect("it fits");

    let repeated = (0..REPEATED_OCCURRENCES).map(|_| one_diagnostic(0)).collect();
    let coalesced = summarize(repeated);
    assert_eq!(coalesced.len(), 1);
    assert_eq!(coalesced[0].occurrences, REPEATED_OCCURRENCES);

    for count in [retained, maximum] {
        let summarized = summarize(distinct_diagnostics(count));
        assert_eq!(summarized.len(), count, "{count} distinct were truncated");
        assert!(
            summarized.iter().all(|diagnostic| diagnostic.code
                != ConfigurationFailureCode::ConfigurationDiagnosticsTruncated),
            "{count} distinct produced a marker"
        );
    }
    let summarized = summarize(distinct_diagnostics(maximum + 1));
    assert_eq!(summarized.len(), retained + 1);
    let marker = summarized.last().expect("the result is not empty");
    assert_eq!(marker.code, ConfigurationFailureCode::ConfigurationDiagnosticsTruncated);
    assert_eq!(marker.structural_location, "diagnostics");
    assert_eq!(
        usize::try_from(marker.occurrences).expect("it fits"),
        maximum + 1 - retained,
        "the marker reports another count"
    );
}

#[test]
fn every_diagnostic_class_and_stage_names_its_own_manifest_literal() {
    let literals = &ProfileAuthenticationContract::embedded().literals;
    let classes = [
        DiagnosticSourceClass::ConfigurationRoot,
        DiagnosticSourceClass::ConfigurationSnapshot,
        DiagnosticSourceClass::Profile,
        DiagnosticSourceClass::Selection,
        DiagnosticSourceClass::ServiceCredentials,
        DiagnosticSourceClass::AdditionalCertificateAuthority,
        DiagnosticSourceClass::PlatformTrust,
    ];
    let rendered: Vec<&str> = classes.iter().map(|class| class.as_text()).collect();
    assert_eq!(rendered, literals.diagnostic_source_classes.iter().collect::<Vec<&String>>());
    let stages = [
        DiagnosticStage::RootResolution,
        DiagnosticStage::FilesystemAuthority,
        DiagnosticStage::DocumentSyntax,
        DiagnosticStage::DocumentShape,
        DiagnosticStage::DocumentSemantics,
        DiagnosticStage::SourceInventory,
        DiagnosticStage::Selection,
        DiagnosticStage::SnapshotConstruction,
    ];
    let rendered: Vec<&str> = stages.iter().map(|stage| stage.as_text()).collect();
    assert_eq!(rendered, literals.diagnostic_stages.iter().collect::<Vec<&String>>());
}

/// Returns one diagnostic distinguished only by `index`.
///
/// The truncation marker's own code is excluded, because a generated
/// diagnostic carrying it would be indistinguishable from the marker the
/// contract adds and would make the assertion below prove nothing.
fn one_diagnostic(index: usize) -> ConfigurationDiagnostic {
    let codes: Vec<ConfigurationFailureCode> = ConfigurationFailureCode::REGISTRY
        .iter()
        .copied()
        .filter(|code| *code != ConfigurationFailureCode::ConfigurationDiagnosticsTruncated)
        .collect();
    ConfigurationDiagnostic::once(
        DiagnosticSourceClass::Profile,
        DiagnosticStage::DocumentShape,
        "name",
        codes[index % codes.len()],
    )
}

/// Returns `count` diagnostics that no coalescing can merge.
fn distinct_diagnostics(count: usize) -> Vec<ConfigurationDiagnostic> {
    (0..count).map(one_diagnostic).collect()
}

/// Refuses a diagnostic set carrying any fixture sentinel.
fn refuse_sentinels(diagnostics: &[ConfigurationDiagnostic]) {
    let rendered = format!("{diagnostics:?}");
    for sentinel in SENTINELS {
        assert!(!rendered.contains(sentinel), "{rendered} carries {sentinel}");
    }
}

/// Returns a commit inventory listing `files` with one class replaced.
fn rebuilt_inventory(
    files: &BTreeMap<String, Vec<u8>>,
    prefix: &str,
    replacement: &[u8],
) -> String {
    let mut inventory = String::from("format_version = 1\n");
    for (reference, bytes) in files {
        if reference == "configuration-snapshot.toml" {
            continue;
        }
        let bytes = if reference.starts_with(prefix) { replacement } else { bytes.as_slice() };
        inventory.push_str(&format!(
            "\n[[sources]]\nreference = \"{reference}\"\nsha256 = \"{}\"\n",
            hexadecimal_digest(bytes)
        ));
    }
    inventory
}

/// Returns the lowercase hexadecimal digest of `bytes`.
fn hexadecimal_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}
