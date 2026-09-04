//! What an operator receives, and what is refused before they receive it.
//!
//! Every rule here is applied to an archive as an object rather than to a
//! filesystem it has already been written to. That is the whole point: an entry
//! that escapes its directory, a link that points anywhere it likes, and a
//! duplicate that silently replaces what came before all do their damage at the
//! moment they are written, so a check that ran during extraction would run
//! after some of the damage.
//!
//! The determinism assertions build the same archive twice under deliberately
//! different ambient conditions and compare the bytes. A release whose bytes
//! depended on a locale, a clock, an account, or the order a directory happened
//! to enumerate in would be a release nobody else could reproduce, and the only
//! way to know is to change those things and look.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::Value;
use slingshot_development::release_artifacts::{
    ArchiveEntry, ArchiveRefusal, CHECKSUM_MANIFEST, EVIDENCE_FORMAT, EntryKind,
    MAXIMUM_ENTRY_BYTES, TAR_PROFILE, ZIP_PROFILE, parse_checksum_manifest, parse_evidence,
    render_checksum_manifest, require_admissible, require_checksums, require_evidence_binds,
    require_name_admissible, survey_archive, write_archive,
};
use slingshot_development::supported_platform_matrix::{self, SupportedPlatformMatrix};

/// Where the fixtures live.
const FIXTURES: &str = "tests/fixtures/release-artifacts";

/// The executable member every row declares.
const EXECUTABLE: &str = "slingshot";

/// The commit a fixture release is built from.
const SOURCE_COMMIT: &str = "1111111111111111111111111111111111111111";

/// How many characters a digest is written in.
const DIGEST_CHARACTERS: usize = 64;

/// How many entries stand in for more than an archive may hold.
const TOO_MANY_ENTRIES: usize = 64;

/// Returns the workspace root.
fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate lives inside the workspace")
}

/// Returns one repository file's text.
fn read_repository_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Returns the rows one fixture states.
fn fixture_rows(name: &str) -> Vec<Value> {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES).join(name))
        .unwrap_or_else(|failure| panic!("{name} could not be read: {failure}"))
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every row reads"))
        .collect()
}

/// Returns the committed supported-platform matrix.
fn matrix() -> SupportedPlatformMatrix {
    supported_platform_matrix::parse_matrix(&read_repository_file("support/platforms.toml"))
        .expect("the committed matrix is valid")
}

/// Returns which refusal one failure is.
fn refusal_name(failure: &ArchiveRefusal) -> &'static str {
    membership_refusal_name(failure).unwrap_or_else(|| evidence_refusal_name(failure))
}

/// Returns the name of one refusal about what an archive holds.
fn membership_refusal_name(failure: &ArchiveRefusal) -> Option<&'static str> {
    let named = match failure {
        ArchiveRefusal::Unreadable(_) => "Unreadable",
        ArchiveRefusal::NameEscapes(_) => "NameEscapes",
        ArchiveRefusal::NameUnacceptable { .. } => "NameUnacceptable",
        ArchiveRefusal::NamesCollide { .. } => "NamesCollide",
        ArchiveRefusal::EntryNotOrdinary { .. } => "EntryNotOrdinary",
        ArchiveRefusal::MemberUndeclared(_) => "MemberUndeclared",
        ArchiveRefusal::MemberMissing(_) => "MemberMissing",
        _ => return None,
    };
    Some(named)
}

/// Returns the name of one refusal about what an archive claims.
fn evidence_refusal_name(failure: &ArchiveRefusal) -> &'static str {
    match failure {
        ArchiveRefusal::BeyondBounds { .. } => "BeyondBounds",
        ArchiveRefusal::ManifestUnacceptable(_) => "ManifestUnacceptable",
        ArchiveRefusal::ChecksumDrift { .. } => "ChecksumDrift",
        ArchiveRefusal::EvidenceUnbound(_) => "EvidenceUnbound",
        ArchiveRefusal::EvidenceDrift { .. } => "EvidenceDrift",
        _ => "unreachable",
    }
}

/// Returns one ordinary entry of the given name and size.
fn ordinary(name: &str, decoded_bytes: u64) -> ArchiveEntry {
    ArchiveEntry { decoded_bytes, kind: EntryKind::OrdinaryFile, name: name.to_owned() }
}

/// Returns every archive profile the committed matrix produces.
///
/// Taken from the matrix rather than written down, so these cases cover exactly
/// the archives a release of this revision writes. A profile no supported row
/// declares is a profile this release does not produce, and asserting over it
/// would be asserting over an archive nobody receives.
fn declared_profiles() -> Vec<String> {
    let mut profiles: Vec<String> =
        matrix().target.iter().map(|row| row.archive_profile.clone()).collect();
    profiles.sort_unstable();
    profiles.dedup();
    profiles
}

#[test]
fn every_profile_a_row_declares_is_one_this_build_writes() {
    // The packager knows both profiles whether or not a supported row currently
    // produces each of them. What must never happen is the reverse: a row naming
    // a profile nothing packs is a row whose archive never exists.
    for profile in declared_profiles() {
        assert!(
            [TAR_PROFILE, ZIP_PROFILE].contains(&profile.as_str()),
            "{profile} is not a profile this build writes"
        );
    }
}

/// Returns the members one row's archive holds.
fn members_for(profile: &str) -> Vec<String> {
    matrix()
        .target
        .iter()
        .find(|row| row.archive_profile == profile)
        .expect("a row declares that profile")
        .archive_members
        .clone()
}

/// Returns the members a fixture archive holds, with its checksum manifest.
fn fixture_members(profile: &str) -> BTreeMap<String, Vec<u8>> {
    use sha2::Digest as _;

    let mut held: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for name in members_for(profile) {
        if name == CHECKSUM_MANIFEST {
            continue;
        }
        held.insert(name.clone(), format!("the bytes of {name}").into_bytes());
    }
    let digests: BTreeMap<String, String> = held
        .iter()
        .map(|(name, bytes)| (name.clone(), hex::encode(sha2::Sha256::digest(bytes))))
        .collect();
    held.insert(CHECKSUM_MANIFEST.to_owned(), render_checksum_manifest(&digests).into_bytes());
    held
}

/// Returns one archive built into a fresh directory, with its bytes.
fn built(named: &str, profile: &str, members: &BTreeMap<String, Vec<u8>>) -> (PathBuf, Vec<u8>) {
    let root = std::env::temp_dir().join(format!("release-{named}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("a temporary root is created");
    let archive = root.join(format!("slingshot.{profile}"));
    write_archive(&archive, profile, members, EXECUTABLE).expect("the archive is written");
    let bytes = std::fs::read(&archive).expect("the archive reads back");
    (root, bytes)
}

#[test]
fn every_declared_entry_name_is_refused_before_anything_is_extracted() {
    let declared = fixture_rows("refused-entry-names.jsonl");
    assert!(!declared.is_empty());
    for row in declared {
        let name = row["name"].as_str().expect("a name");
        let entry = row["entry"].as_str().expect("an entry");
        let failure = require_name_admissible(entry).expect_err(&format!("{name} was accepted"));
        assert_eq!(
            refusal_name(&failure),
            row["refusal"].as_str().expect("a refusal"),
            "{name}: {failure}"
        );
    }
}

#[test]
fn every_entry_that_is_not_an_ordinary_file_is_refused() {
    let members = members_for(TAR_PROFILE);
    for row in fixture_rows("refused-entry-kinds.jsonl") {
        let kind = match row["kind"].as_str().expect("a kind") {
            "Link" => EntryKind::Link,
            "Directory" => EntryKind::Directory,
            _ => EntryKind::Special,
        };
        let entries = vec![ArchiveEntry { decoded_bytes: 1, kind, name: EXECUTABLE.to_owned() }];
        let failure = require_admissible(&entries, &members, 1)
            .expect_err("a release archive holds ordinary files");
        assert_eq!(refusal_name(&failure), "EntryNotOrdinary", "{}", row["name"]);
    }
}

#[test]
fn two_names_that_are_one_file_where_case_folds_are_refused() {
    let entries = vec![ordinary("LICENSE", 1), ordinary("license", 1)];
    let failure = require_admissible(&entries, &["LICENSE".to_owned()], 1)
        .expect_err("two names, one file on two of the three rows");
    assert_eq!(refusal_name(&failure), "NamesCollide");
}

#[test]
fn an_archive_holds_exactly_what_its_row_declares_and_nothing_else() {
    for profile in declared_profiles() {
        let profile = profile.as_str();
        let members = members_for(profile);
        let entries: Vec<ArchiveEntry> = members.iter().map(|name| ordinary(name, 1)).collect();
        require_admissible(&entries, &members, 1).unwrap_or_else(|failure| panic!("{failure}"));

        let mut extra = entries.clone();
        extra.push(ordinary("surprise", 1));
        let failure = require_admissible(&extra, &members, 1).expect_err("an undeclared member");
        assert_eq!(refusal_name(&failure), "MemberUndeclared");

        let short = &entries[..entries.len() - 1];
        let failure = require_admissible(short, &members, 1).expect_err("a declared member");
        assert_eq!(refusal_name(&failure), "MemberMissing");

        let mut repeated = entries.clone();
        repeated.push(entries[0].clone());
        let failure = require_admissible(&repeated, &members, 1).expect_err("a duplicate");
        assert_eq!(refusal_name(&failure), "NamesCollide");
    }
}

#[test]
fn an_archive_beyond_any_of_its_bounds_is_refused() {
    let members = members_for(TAR_PROFILE);
    let entries: Vec<ArchiveEntry> = members.iter().map(|name| ordinary(name, 1)).collect();
    let failure = require_admissible(&entries, &members, u64::MAX).expect_err("too large");
    assert_eq!(refusal_name(&failure), "BeyondBounds");

    let enormous: Vec<ArchiveEntry> =
        members.iter().map(|name| ordinary(name, MAXIMUM_ENTRY_BYTES + 1)).collect();
    let failure = require_admissible(&enormous, &members, 1).expect_err("one entry too large");
    assert_eq!(refusal_name(&failure), "BeyondBounds");

    let many: Vec<ArchiveEntry> =
        (0..TOO_MANY_ENTRIES).map(|index| ordinary(&format!("member-{index}"), 1)).collect();
    let failure = require_admissible(&many, &members, 1).expect_err("too many entries");
    assert_eq!(refusal_name(&failure), "BeyondBounds");
}

#[test]
fn the_checksum_manifest_ascends_by_name_and_reads_back_exactly() {
    let members = fixture_members(TAR_PROFILE);
    let manifest =
        String::from_utf8(members[CHECKSUM_MANIFEST].clone()).expect("the manifest is text");
    let parsed = parse_checksum_manifest(&manifest).expect("it reads back");
    let names: Vec<&String> = parsed.keys().collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "the manifest ascends, so no filesystem order decides it");
    assert!(!parsed.contains_key(CHECKSUM_MANIFEST), "a manifest does not checksum itself");
    require_checksums(&parsed, &parsed).expect("a manifest agrees with itself");
}

#[test]
fn a_checksum_manifest_that_is_not_one_is_refused() {
    let digest = "a".repeat(DIGEST_CHARACTERS);
    let other = "b".repeat(DIGEST_CHARACTERS);
    let uppercase = "A".repeat(DIGEST_CHARACTERS);
    for held in [
        String::new(),
        "not-a-digest  slingshot\n".to_owned(),
        "0123456789abcdef  slingshot\n".to_owned(),
        format!("{digest}  ../slingshot\n"),
        format!("{digest}  zebra\n{other}  aardvark\n"),
        format!("{uppercase}  slingshot\n"),
    ] {
        assert!(parse_checksum_manifest(&held).is_err(), "{held:?} was accepted");
    }
}

#[test]
fn a_member_whose_bytes_changed_is_caught_by_the_manifest() {
    let members = fixture_members(TAR_PROFILE);
    let manifest =
        String::from_utf8(members[CHECKSUM_MANIFEST].clone()).expect("the manifest is text");
    let declared = parse_checksum_manifest(&manifest).expect("it reads");
    let mut observed = declared.clone();
    let first = declared.keys().next().expect("a member").clone();
    observed.insert(first.clone(), "0".repeat(DIGEST_CHARACTERS));
    let failure = require_checksums(&declared, &observed).expect_err("one member changed");
    assert_eq!(refusal_name(&failure), "ChecksumDrift");
    assert!(failure.to_string().contains(&first), "the diagnostic names which member");

    observed.remove(&first);
    let failure = require_checksums(&declared, &observed).expect_err("one member is absent");
    assert_eq!(refusal_name(&failure), "MemberMissing");
}

#[test]
fn one_revision_produces_one_archive_whatever_the_machine_is_doing() {
    for profile in declared_profiles() {
        let profile = profile.as_str();
        let members = fixture_members(profile);
        let (first_root, first) = built("first", profile, &members);
        let (second_root, second) = built("second", profile, &members);
        assert_eq!(first, second, "{profile}: the machine decided some of the bytes");
        let written = read_repository_file("crates/slingshot-development/src/release_artifacts.rs");
        for fixed in ["set_mtime", "set_uid", "set_gid", "last_modified_time"] {
            assert!(written.contains(fixed), "the writer leaves {fixed} to the machine");
        }
        std::fs::remove_dir_all(&first_root).ok();
        std::fs::remove_dir_all(&second_root).ok();
    }
}

#[test]
fn an_archive_this_build_writes_is_one_this_build_admits() {
    for profile in declared_profiles() {
        let profile = profile.as_str();
        let members = fixture_members(profile);
        let declared = members_for(profile);
        let (root, _) = built("survey", profile, &members);
        let archive = root.join(format!("slingshot.{profile}"));
        let surveyed = survey_archive(&archive, profile).expect("it surveys");
        let compressed = std::fs::metadata(&archive).expect("it is there").len();
        require_admissible(&surveyed, &declared, compressed)
            .unwrap_or_else(|failure| panic!("{profile}: {failure}"));
        let held: BTreeSet<&str> = surveyed.iter().map(|entry| entry.name.as_str()).collect();
        let expected: BTreeSet<&str> = declared.iter().map(String::as_str).collect();
        assert_eq!(held, expected, "{profile}");
        std::fs::remove_dir_all(&root).ok();
    }
}

/// Returns the digest a fixture cache carries.
fn cache_digest() -> String {
    "2".repeat(DIGEST_CHARACTERS)
}

/// Returns one evidence manifest as it would be committed.
fn evidence_text() -> String {
    let digest = "3".repeat(DIGEST_CHARACTERS);
    format!(
        "archive = \"slingshot-x86_64-unknown-linux-gnu.tar.gz\"\n\
         archive-sha256 = \"{digest}\"\n\
         cache-sha256 = \"{}\"\n\
         format = \"{EVIDENCE_FORMAT}\"\n\
         provider-run = \".github/workflows/release.yml@refs/heads/main\"\n\
         rustsec-review-record-sha256 = \"{digest}\"\n\
         source-commit = \"{SOURCE_COMMIT}\"\n\
         source-tree = \"{SOURCE_COMMIT}\"\n\
         toolchain = \"1.98.0\"\n\
         triple = \"x86_64-unknown-linux-gnu\"\n",
        cache_digest()
    )
}

#[test]
fn evidence_binds_the_row_the_source_and_the_cache_it_was_built_from() {
    let held = parse_evidence(&evidence_text()).expect("it parses");
    require_evidence_binds(&held, &held.triple, SOURCE_COMMIT, &cache_digest())
        .expect("this evidence is about this release");
    let drifted = [
        require_evidence_binds(&held, "aarch64-apple-darwin", SOURCE_COMMIT, &cache_digest()),
        require_evidence_binds(&held, &held.triple, &"0".repeat(40), &cache_digest()),
        require_evidence_binds(&held, &held.triple, SOURCE_COMMIT, &"0".repeat(DIGEST_CHARACTERS)),
    ];
    for outcome in drifted {
        let failure = outcome.expect_err("this evidence is about something else");
        assert_eq!(refusal_name(&failure), "EvidenceDrift");
    }
}

#[test]
fn evidence_that_leaves_a_required_binding_empty_is_refused() {
    for absent in [
        "archive-sha256",
        "cache-sha256",
        "provider-run",
        "rustsec-review-record-sha256",
        "source-commit",
        "source-tree",
    ] {
        let text = evidence_text()
            .lines()
            .map(|line| {
                if line.starts_with(&format!("{absent} = ")) {
                    format!("{absent} = \"\"")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<String>>()
            .join("\n");
        let failure = parse_evidence(&text).expect_err(&format!("{absent} was accepted empty"));
        assert_eq!(refusal_name(&failure), "EvidenceUnbound", "{absent}");
    }
}

#[test]
fn the_verifier_authenticates_provenance_before_it_reads_a_manifest() {
    let verify = read_repository_file("scripts/verify_release_artifacts");
    let attestation = verify.find("gh attestation verify").expect("it verifies provenance");
    let manifest = verify.find("verify-release-artifacts").expect("it reads the manifest");
    assert!(attestation < manifest, "a manifest checked first has already said what checks it");
    // The pinned client has no offline switch, and one it does not have is one
    // it cannot obey. Verification is offline because everything it reads comes
    // from disk: the attestation beside the archive, and the trust root
    // committed here. Verifying with every route to the network poisoned
    // changes nothing about the outcome.
    assert!(verify.contains("--bundle"), "it reads the attestation beside the archive");
    assert!(verify.contains("--custom-trusted-root"), "and never the verifier's own root");
    for reaching in ["curl", "git fetch", "gh api", "--update"] {
        assert!(!verify.contains(reaching), "a verifier that ran {reaching} would report on more");
    }
}

#[test]
fn the_builder_builds_twice_and_compares_before_it_packages() {
    let build = read_repository_file("scripts/build_release_artifacts");
    assert!(build.contains("the first export"), "it exports once");
    assert!(build.contains("the second export"), "and again, independently");
    let compare = build.find("cmp -s").expect("it compares the two");
    let package = build.find("package-release-artifacts").expect("it packages");
    assert!(compare < package, "an archive built before the comparison would prove nothing");
    for ambient in ["CARGO_INCREMENTAL=0", "--frozen --offline", "--remap-path-prefix"] {
        assert!(build.contains(ambient), "the build does not fix {ambient}");
    }
    assert!(build.contains("CARGO_HOME=\"$CACHE_SET\""), "the verified cache is the Cargo home");
    assert!(build.contains("git diff --quiet HEAD"), "and a dirty tree is refused before a build");
}
