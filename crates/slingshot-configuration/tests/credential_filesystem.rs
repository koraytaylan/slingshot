//! Assertions for the authority every configuration source is read through.
//!
//! The rules are proved twice. The scripted authority states each one as a
//! condition a caller asks for - another account's file, a widened list, a
//! second hard link, a source that changes while it is read - so every
//! supported row's decision is provable on any host and the rows are comparable
//! to one another. The current row then runs its real policy over a tree this
//! test builds, which is the only part of the arrangement this machine can
//! actually produce, and that observation is for this environment alone.

use std::collections::BTreeMap;
use std::path::PathBuf;

use slingshot_configuration::configuration_root::{AccountIdentity, ConfigurationRoot};
use slingshot_configuration::credential_filesystem::{
    ConfigurationFilesystemAuthority, CredentialFilesystemFailure,
};
use slingshot_configuration::testing::credential_filesystem::{
    EntrySafety, Instability, ScriptedEntry, ScriptedFilesystem,
};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};

/// Root-relative path of the source most assertions here read.
const CREDENTIAL_PATH: &str = "credentials/production.json";

/// Components of that path.
const CREDENTIAL_COMPONENTS: &[&str] = &["credentials", "production.json"];

/// Bytes that source holds.
const CREDENTIAL_BYTES: &[u8] = b"{\"ok\":true}";

/// Profile documents the listing assertion writes.
const LISTED_PROFILES: &[&str] = &["b.toml", "a.toml", "c.toml"];

/// Every safety state a scripted entry can refuse with.
const REFUSED_SAFETY: &[EntrySafety] = &[
    EntrySafety::Link,
    EntrySafety::ForeignOwner,
    EntrySafety::WidenedAccess,
    EntrySafety::SecondLink,
    EntrySafety::NotOrdinary,
];

/// Returns a scripted root holding one safe credential.
fn scripted_root() -> ScriptedFilesystem {
    ScriptedFilesystem::new().with_source(CREDENTIAL_PATH, CREDENTIAL_BYTES)
}

/// Returns the bytes one scripted read produced.
fn read_bytes(
    authority: &ScriptedFilesystem,
    components: &[&str],
) -> Result<Vec<u8>, CredentialFilesystemFailure> {
    let bound = ProfileAuthenticationContract::embedded()
        .limits
        .maximum_configuration_source_document_bytes;
    authority
        .read_source(components, bound)
        .map(|source| source.document.lend_bytes_for_inspection(<[u8]>::to_vec))
}

#[test]
fn a_safe_source_is_read_through_one_verified_handle() {
    let authority = scripted_root();
    assert_eq!(read_bytes(&authority, CREDENTIAL_COMPONENTS).expect("it reads"), CREDENTIAL_BYTES);
    assert!(authority.observe_presence(CREDENTIAL_COMPONENTS).expect("it is present"));
    assert!(!authority.observe_presence(&["credentials", "absent.json"]).expect("it is absent"));
}

#[test]
fn every_unsafe_state_refuses_the_source_without_naming_it() {
    for safety in REFUSED_SAFETY {
        let entry = ScriptedEntry::safe(CREDENTIAL_BYTES).with_safety(*safety);
        let authority = ScriptedFilesystem::new().with_entry(CREDENTIAL_PATH, entry);
        let failure = read_bytes(&authority, CREDENTIAL_COMPONENTS).expect_err("it is refused");
        assert_eq!(failure.code, ConfigurationFailureCode::ConfigurationFileUnsafe, "{safety:?}");
        assert!(!format!("{failure}").contains("production"), "the failure names the source");
    }
}

#[test]
fn an_unsafe_root_refuses_before_any_source_is_opened() {
    let authority = scripted_root().with_unsafe_root();
    assert_eq!(
        authority.verify_root().expect_err("the root is refused").code,
        ConfigurationFailureCode::ConfigurationRootUnsafe
    );
    assert_eq!(
        read_bytes(&authority, CREDENTIAL_COMPONENTS).expect_err("nothing is read").code,
        ConfigurationFailureCode::ConfigurationRootUnsafe
    );
    assert_eq!(authority.reads(), 0, "an unsafe root still read a source");
}

#[test]
fn a_source_that_settles_is_accepted_and_one_that_never_settles_is_refused() {
    let settling =
        ScriptedEntry::safe(CREDENTIAL_BYTES).with_instability(Instability::SettlesAfterOneAttempt);
    let authority = ScriptedFilesystem::new().with_entry(CREDENTIAL_PATH, settling);
    assert_eq!(
        read_bytes(&authority, CREDENTIAL_COMPONENTS).expect("it settles"),
        CREDENTIAL_BYTES
    );

    let never = ScriptedEntry::safe(CREDENTIAL_BYTES).with_instability(Instability::NeverSettles);
    let authority = ScriptedFilesystem::new().with_entry(CREDENTIAL_PATH, never);
    let failure = read_bytes(&authority, CREDENTIAL_COMPONENTS).expect_err("it never settles");
    assert_eq!(failure.code, ConfigurationFailureCode::ConfigurationFileChangedDuringRead);
    let attempts =
        ProfileAuthenticationContract::embedded().limits.maximum_configuration_stable_read_attempts;
    assert_eq!(authority.reads(), attempts, "the attempts are not the contract's");
}

#[test]
fn a_source_larger_than_its_bound_is_refused_before_it_is_retained() {
    let authority = scripted_root();
    let bound = u64::try_from(CREDENTIAL_BYTES.len() - 1).expect("the length fits");
    let failure = authority.read_source(CREDENTIAL_COMPONENTS, bound).expect_err("it is too large");
    assert_eq!(failure.code, ConfigurationFailureCode::ConfigurationDocumentTooLarge);
}

#[test]
fn a_directory_listing_is_bounded_and_ordered() {
    let mut authority = ScriptedFilesystem::new().with_directory("profiles");
    for name in LISTED_PROFILES {
        authority = authority.with_source(&format!("profiles/{name}"), b"format_version = 1\n");
    }
    let room = u64::try_from(LISTED_PROFILES.len()).expect("the count fits");
    let listed = authority.list_directory(&["profiles"], room).expect("the listing fits");
    let names: Vec<&str> = listed.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, vec!["a.toml", "b.toml", "c.toml"]);
    assert!(listed.iter().all(|entry| entry.ordinary_file));

    let failure =
        authority.list_directory(&["profiles"], room - 1).expect_err("the listing is bounded");
    assert_eq!(failure.code, ConfigurationFailureCode::ConfigurationDirectoryLimitExceeded);
}

#[test]
fn a_publication_between_two_reads_is_visible_to_the_later_one() {
    let replacement = BTreeMap::from([(CREDENTIAL_PATH.to_owned(), b"{\"ok\":false}".to_vec())]);
    let authority = scripted_root().publishing_after(1, replacement);
    assert_eq!(read_bytes(&authority, CREDENTIAL_COMPONENTS).expect("it reads"), CREDENTIAL_BYTES);
    assert_eq!(
        read_bytes(&authority, CREDENTIAL_COMPONENTS).expect("it reads again"),
        b"{\"ok\":false}",
        "the second read did not see the published generation"
    );
}

#[cfg(unix)]
mod current_row {
    //! What this environment's real policy does over a tree this test builds.
    //!
    //! The observation covers only the row this machine is, and claims nothing
    //! about another. Plan 0009 owns the authenticated evidence for every row.

    use std::fs::File;
    use std::io::Write;
    use std::os::unix::fs::{PermissionsExt, symlink};

    use slingshot_configuration::credential_filesystem::UnixConfigurationFilesystem;

    use super::{
        AccountIdentity, ConfigurationFailureCode, ConfigurationFilesystemAuthority,
        ConfigurationRoot, PathBuf, ProfileAuthenticationContract,
    };

    /// Bytes every source this row writes holds.
    const SOURCE_BYTES: &[u8] = b"{\"ok\":true}";

    /// Permission bits of a file only its owner may read or write.
    const OWNER_ONLY_FILE: u32 = 0o600;

    /// Permission bits of a file a group may also read.
    const GROUP_READABLE_FILE: u32 = 0o640;

    /// Permission bits of a directory only its owner may enter or change.
    const OWNER_ONLY_DIRECTORY: u32 = 0o700;

    /// Builds a configuration tree below one temporary home.
    fn build_tree() -> (tempfile::TempDir, UnixConfigurationFilesystem) {
        let home = tempfile::tempdir().expect("a temporary home is created");
        let mut root = home.path().to_path_buf();
        for component in ConfigurationRoot::root_components() {
            root.push(component);
            std::fs::create_dir(&root).expect("the component is created");
            std::fs::set_permissions(&root, PermissionsExt::from_mode(OWNER_ONLY_DIRECTORY))
                .expect("the component is protected");
        }
        let directory = root.join("credentials");
        std::fs::create_dir(&directory).expect("the directory is created");
        std::fs::set_permissions(&directory, PermissionsExt::from_mode(OWNER_ONLY_DIRECTORY))
            .expect("the directory is protected");
        write_source(&directory.join("production.json"), OWNER_ONLY_FILE);
        let identity = AccountIdentity::UnixUser(uzers::get_effective_uid());
        let configuration =
            ConfigurationRoot::at_explicit_home(identity, home.path().to_path_buf());
        let authority =
            UnixConfigurationFilesystem::new(configuration).expect("this row is supported");
        (home, authority)
    }

    /// Writes one source with exactly the permissions asked for.
    fn write_source(path: &PathBuf, mode: u32) {
        let mut file = File::create(path).expect("the source is created");
        file.write_all(SOURCE_BYTES).expect("the source is written");
        std::fs::set_permissions(path, PermissionsExt::from_mode(mode))
            .expect("the source is protected");
    }

    /// Returns the generic bound one source is read under.
    fn source_bound() -> u64 {
        ProfileAuthenticationContract::embedded().limits.maximum_configuration_source_document_bytes
    }

    #[test]
    fn this_row_reads_a_safe_source_and_refuses_every_unsafe_one() {
        let (home, authority) = build_tree();
        let root = home.path().join(".config").join("slingshot");
        authority.verify_root().expect("the tree is safe");
        let source = authority
            .read_source(&["credentials", "production.json"], source_bound())
            .expect("the safe source reads");
        assert_eq!(source.length, u64::try_from(SOURCE_BYTES.len()).expect("the length fits"));

        symlink("/etc/passwd", root.join("credentials").join("escape.json"))
            .expect("the link is created");
        let failure = authority
            .read_source(&["credentials", "escape.json"], source_bound())
            .expect_err("a link is refused");
        assert_eq!(failure.code, ConfigurationFailureCode::ConfigurationFileUnsafe);

        let aliased = root.join("credentials").join("aliased.json");
        write_source(&aliased, OWNER_ONLY_FILE);
        std::fs::hard_link(&aliased, home.path().join("outside.json"))
            .expect("the second name is created");
        let failure = authority
            .read_source(&["credentials", "aliased.json"], source_bound())
            .expect_err("a second name is refused");
        assert_eq!(failure.code, ConfigurationFailureCode::ConfigurationFileUnsafe);

        let widened = root.join("credentials").join("widened.json");
        write_source(&widened, GROUP_READABLE_FILE);
        let failure = authority
            .read_source(&["credentials", "widened.json"], source_bound())
            .expect_err("a group-readable source is refused");
        assert_eq!(failure.code, ConfigurationFailureCode::ConfigurationFileUnsafe);
    }

    #[test]
    fn this_row_refuses_a_root_component_that_is_a_link() {
        let home = tempfile::tempdir().expect("a temporary home is created");
        let elsewhere = home.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).expect("the target is created");
        symlink(&elsewhere, home.path().join(".config")).expect("the component is a link");
        let identity = AccountIdentity::UnixUser(uzers::get_effective_uid());
        let authority = UnixConfigurationFilesystem::new(ConfigurationRoot::at_explicit_home(
            identity,
            home.path().to_path_buf(),
        ))
        .expect("this row is supported");
        assert_eq!(
            authority.verify_root().expect_err("a linked component is refused").code,
            ConfigurationFailureCode::ConfigurationRootUnsafe
        );
    }
}
