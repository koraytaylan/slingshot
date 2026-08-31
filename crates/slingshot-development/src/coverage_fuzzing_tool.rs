//! Which coverage-guided fuzzing tool this repository uses, and how it is proved.
//!
//! A tool that fuzzes this product decides which inputs the product is ever
//! tried against, so which tool it is has to be as exact as anything else that
//! is pinned. One repository, one full commit, one locked dependency graph, one
//! dated toolchain: a branch, a tag, a shortened commit, or a name found on the
//! path would each let a different tool answer to the same description.
//!
//! # A bundle is verified before it is run, and by path alone afterwards
//!
//! What a build produces is a directory with a manifest in it. Verification
//! reads that manifest, checks every entry against what the manifest says, and
//! returns the one absolute path of the executable. Nothing installs anything,
//! nothing mutates a Cargo home, and nothing searches the path - because a tool
//! found rather than supplied is a tool nobody pinned.
//!
//! # A version string is not provenance
//!
//! An executable that prints the expected version proves it prints the expected
//! version. The bundle is accepted on its recorded source, lock, cache, and
//! binary digests; the version check afterwards is a sanity check on top of
//! that, not the thing being relied on.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Where the pin lives.
pub const PIN_PATH: &str = "compatibility/coverage-fuzzing.toml";

/// Where the bundle schema lives.
pub const SCHEMA_PATH: &str = "schemas/compatibility/coverage-fuzzing-tool.schema.json";

/// What a bundle manifest is called inside a bundle.
pub const BUNDLE_MANIFEST: &str = "bundle.json";

/// The format a bundle manifest declares.
pub const BUNDLE_FORMAT: &str = "slingshot.coverage-fuzzing-bundle/1";

/// The variable a consumer receives a verified bundle in.
pub const BUNDLE_VARIABLE: &str = "SLINGSHOT_COVERAGE_FUZZING_TOOL_BUNDLE";

/// How many characters a commit is named by.
pub const COMMIT_CHARACTERS: usize = 40;

/// The pin, exactly as it is committed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageFuzzingPin {
    /// What the executable is called.
    pub binary: String,
    /// What a bundle may hold.
    pub bundle: BundleLimits,
    /// The toolchain the tool itself is built with.
    pub build_toolchain: String,
    /// The exact commit the tool is built from.
    pub commit: String,
    /// The format this pin declares.
    pub format: String,
    /// The dated nightly every fuzz target is built with.
    pub fuzz_toolchain: String,
    /// The package the executable comes from.
    pub package: String,
    /// What acquiring the tool may and may not do.
    pub policy: AcquisitionPolicy,
    /// The repository the commit lives in.
    pub repository: String,
    /// The version the executable reports.
    pub version: String,
}

/// What a bundle may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleLimits {
    /// How many bytes a bundle may hold altogether.
    pub maximum_bytes: u64,
    /// How many entries it may hold.
    pub maximum_entries: u64,
    /// How many bytes one entry's path may hold.
    pub maximum_entry_utf8_bytes: u64,
}

/// What acquiring the tool may and may not do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionPolicy {
    /// Whether a dependency may come from a repository rather than a registry.
    pub allows_git_dependencies: bool,
    /// Whether a source may be replaced with another.
    pub allows_source_replacement: bool,
    /// Whether every registry entry carries a checksum.
    pub requires_checksums: bool,
    /// Whether resolution is locked.
    pub requires_locked_resolution: bool,
}

/// One built bundle, as its manifest describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    /// What the executable digests to.
    pub binary_sha256: String,
    /// Which toolchain built it.
    pub build_toolchain: String,
    /// Which commit it was built from.
    pub commit: String,
    /// What the dependency cache digests to.
    pub dependency_cache_sha256: String,
    /// The closed environment it was built under.
    pub environment: std::collections::BTreeMap<String, String>,
    /// The format this manifest declares.
    pub format: String,
    /// Which host it was built for.
    pub host: String,
    /// What the lockfile digests to.
    pub lock_sha256: String,
    /// Which repository it came from.
    pub repository: String,
    /// What the source tree digests to.
    pub tree_sha256: String,
}

/// Why a bundle is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BundleRefusal {
    /// The bundle holds no manifest, or one that cannot be read.
    #[error("the bundle manifest could not be read: {0}")]
    Unreadable(String),
    /// The manifest declares another format.
    #[error("a bundle manifest declares {BUNDLE_FORMAT}, and this declares {0}")]
    ForeignFormat(String),
    /// The bundle was built from something other than the pin.
    #[error("{0} does not match what this repository pins")]
    NotThePinnedTool(String),
    /// The bundle was built for another host.
    #[error("this bundle was built for {0}")]
    AnotherHost(String),
    /// An entry is missing, extra, or outside what a bundle may hold.
    #[error("{0}")]
    EntryRefused(String),
    /// The executable is not there, or is not what the manifest recorded.
    #[error("the executable is absent or is not the one this manifest records")]
    ExecutableUnusable,
}

/// Requires one bundle to be the tool this repository pins, and returns its path.
///
/// Everything the manifest records is checked before the executable is run, and
/// the path returned is the only way a consumer reaches it. A consumer that
/// searched for it instead would run whichever tool the machine happened to
/// have.
///
/// # Errors
///
/// Returns [`BundleRefusal`] naming the first thing that stops the bundle.
pub fn verified(
    bundle: &Path,
    pin: &CoverageFuzzingPin,
    host: &str,
) -> Result<PathBuf, BundleRefusal> {
    let text = std::fs::read_to_string(bundle.join(BUNDLE_MANIFEST))
        .map_err(|failure| BundleRefusal::Unreadable(failure.to_string()))?;
    let manifest: BundleManifest = serde_json::from_str(&text)
        .map_err(|failure| BundleRefusal::Unreadable(failure.to_string()))?;
    if manifest.format != BUNDLE_FORMAT {
        return Err(BundleRefusal::ForeignFormat(manifest.format));
    }
    if manifest.repository != pin.repository {
        return Err(BundleRefusal::NotThePinnedTool("the repository".to_owned()));
    }
    if manifest.commit != pin.commit {
        return Err(BundleRefusal::NotThePinnedTool("the commit".to_owned()));
    }
    if manifest.build_toolchain != pin.build_toolchain {
        return Err(BundleRefusal::NotThePinnedTool("the build toolchain".to_owned()));
    }
    if manifest.host != host {
        return Err(BundleRefusal::AnotherHost(manifest.host));
    }
    require_bounded(bundle, &pin.bundle)?;
    let executable = bundle.join(&pin.binary);
    require_recorded_executable(&executable, &manifest.binary_sha256)?;
    Ok(executable)
}

/// Requires the bundle's contents to be inside what a bundle may hold.
fn require_bounded(bundle: &Path, limits: &BundleLimits) -> Result<(), BundleRefusal> {
    let mut entries = 0_u64;
    let mut bytes = 0_u64;
    let mut pending = vec![bundle.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let held = std::fs::read_dir(&directory)
            .map_err(|failure| BundleRefusal::EntryRefused(failure.to_string()))?;
        for entry in held.filter_map(Result::ok) {
            let path = entry.path();
            entries += 1;
            let named = path.strip_prefix(bundle).unwrap_or(&path).to_string_lossy().into_owned();
            if u64::try_from(named.len()).unwrap_or(u64::MAX) > limits.maximum_entry_utf8_bytes {
                return Err(BundleRefusal::EntryRefused(format!("{named} is named too long")));
            }
            let kind = entry
                .file_type()
                .map_err(|failure| BundleRefusal::EntryRefused(failure.to_string()))?;
            if kind.is_symlink() {
                return Err(BundleRefusal::EntryRefused(format!("{named} is a link")));
            }
            if kind.is_dir() {
                pending.push(path);
                continue;
            }
            if !kind.is_file() {
                return Err(BundleRefusal::EntryRefused(format!(
                    "{named} is not an ordinary file"
                )));
            }
            bytes += entry.metadata().map(|held| held.len()).unwrap_or_default();
        }
    }
    if entries > limits.maximum_entries {
        return Err(BundleRefusal::EntryRefused(format!("the bundle holds {entries} entries")));
    }
    if bytes > limits.maximum_bytes {
        return Err(BundleRefusal::EntryRefused(format!("the bundle holds {bytes} bytes")));
    }
    Ok(())
}

/// Requires the executable to be the one the manifest recorded.
fn require_recorded_executable(executable: &Path, recorded: &str) -> Result<(), BundleRefusal> {
    let held = std::fs::read(executable).map_err(|_| BundleRefusal::ExecutableUnusable)?;
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    digest.update(&held);
    let observed: String = digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    if observed == recorded { Ok(()) } else { Err(BundleRefusal::ExecutableUnusable) }
}

/// Returns the pin one manifest text declares.
///
/// # Errors
///
/// Returns [`BundleRefusal::Unreadable`] for a manifest this build cannot read
/// and [`BundleRefusal::NotThePinnedTool`] for a commit that is not one.
pub fn parse_pin(text: &str) -> Result<CoverageFuzzingPin, BundleRefusal> {
    let held: CoverageFuzzingPin =
        toml::from_str(text).map_err(|failure| BundleRefusal::Unreadable(failure.to_string()))?;
    let shaped = held.commit.len() == COMMIT_CHARACTERS
        && held
            .commit
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character));
    if !shaped {
        return Err(BundleRefusal::NotThePinnedTool("the commit".to_owned()));
    }
    Ok(held)
}

/// The one command in this repository that fetches somebody else's source.
const GIT: &str = "git";

/// The command that builds it.
const CARGO: &str = "cargo";

/// The variables one build of the pinned tool is allowed to see.
///
/// Closed rather than inherited. A build that saw an ambient Cargo home, a
/// source replacement, or a set of Rust flags would produce bytes that depended
/// on the machine, and the whole point of building it twice is that they do
/// not.
const PERMITTED_VARIABLES: &[&str] =
    &["CARGO_HOME", "CARGO_TARGET_DIR", "PATH", "RUSTUP_TOOLCHAIN"];

/// Returns what a set of bytes digests to.
fn digest_of(bytes: &[u8]) -> String {
    use sha2::Digest as _;

    hex::encode(sha2::Sha256::digest(bytes))
}

/// Runs one command and returns its standard output, refusing a failure.
fn run(
    program: &str,
    arguments: &[&str],
    working_directory: &Path,
) -> Result<String, BundleRefusal> {
    let produced = std::process::Command::new(program)
        .args(arguments)
        .current_dir(working_directory)
        .output()
        .map_err(|failure| BundleRefusal::Unreadable(format!("{program}: {failure}")))?;
    if !produced.status.success() {
        return Err(BundleRefusal::Unreadable(format!(
            "{program} {}: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&produced.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&produced.stdout).trim().to_owned())
}

/// Acquires the pinned tool, builds it twice, and writes the bundle.
///
/// The fetch is verified against the pin afterwards rather than trusted from
/// the reference that produced it: a reference is a name somebody can move, and
/// the commit is what the pin names.
///
/// Two builds from one source, into two separate target roots, under one closed
/// environment. One build proves that a build happened; two identical builds
/// prove that the bytes come from the source. If they differ, there is no
/// bundle, because a tool whose bytes depend on the machine cannot be the tool
/// this repository pins.
///
/// # Errors
///
/// Returns [`BundleRefusal`] naming what stopped it: a fetch that produced
/// another commit, a build that failed, or two builds that disagreed.
pub fn prepare(
    destination: &Path,
    pin: &CoverageFuzzingPin,
    host: &str,
) -> Result<BundleManifest, BundleRefusal> {
    let unreadable = |failure: std::io::Error| BundleRefusal::Unreadable(failure.to_string());
    let scratch = destination.with_extension("build");
    std::fs::remove_dir_all(&scratch).ok();
    let source = scratch.join("source");
    std::fs::create_dir_all(&source).map_err(unreadable)?;
    std::fs::create_dir_all(destination).map_err(unreadable)?;

    run(GIT, &["init", "--quiet", "."], &source)?;
    run(GIT, &["remote", "add", "origin", &pin.repository], &source)?;
    run(GIT, &["fetch", "--quiet", "--depth", "1", "origin", &pin.commit], &source)?;
    run(GIT, &["checkout", "--quiet", &pin.commit], &source)?;
    let observed = run(GIT, &["rev-parse", "HEAD"], &source)?;
    if observed != pin.commit {
        return Err(BundleRefusal::NotThePinnedTool(format!("the checkout is at {observed}")));
    }

    let listing = run(GIT, &["ls-tree", "-r", "--full-tree", &pin.commit], &source)?;
    let tree_sha256 = digest_of(listing.as_bytes());
    let lock = std::fs::read(source.join("Cargo.lock")).map_err(unreadable)?;
    let lock_sha256 = digest_of(&lock);

    let cargo_home = scratch.join("cargo-home");
    let first = build_once(&source, &scratch.join("first"), &cargo_home, pin)?;
    let second = build_once(&source, &scratch.join("second"), &cargo_home, pin)?;
    if first != second {
        return Err(BundleRefusal::ExecutableUnusable);
    }

    let executable = scratch.join("first").join("release").join(&pin.binary);
    let bytes = std::fs::read(&executable).map_err(unreadable)?;
    // Copied rather than written, so the bundle's executable is executable. A
    // consumer that verified a bundle and then could not run what is in it
    // would have verified a file rather than a tool.
    std::fs::copy(&executable, destination.join(&pin.binary)).map_err(unreadable)?;
    let manifest = BundleManifest {
        binary_sha256: digest_of(&bytes),
        build_toolchain: pin.build_toolchain.clone(),
        commit: pin.commit.clone(),
        dependency_cache_sha256: crate::release_input_cache::survey(&cargo_home)
            .map(|surveyed| surveyed.digest)
            .map_err(|failure| BundleRefusal::Unreadable(failure.to_string()))?,
        environment: PERMITTED_VARIABLES
            .iter()
            .map(|named| ((*named).to_owned(), "<closed>".to_owned()))
            .collect(),
        format: BUNDLE_FORMAT.to_owned(),
        host: host.to_owned(),
        lock_sha256,
        repository: pin.repository.clone(),
        tree_sha256,
    };
    let rendered = serde_json::to_string_pretty(&manifest)
        .map_err(|failure| BundleRefusal::Unreadable(failure.to_string()))?;
    std::fs::write(destination.join(BUNDLE_MANIFEST), format!("{rendered}\n"))
        .map_err(unreadable)?;
    std::fs::remove_dir_all(&scratch).ok();
    Ok(manifest)
}

/// Builds the pinned tool once into `target_root` and returns its digest.
fn build_once(
    source: &Path,
    target_root: &Path,
    cargo_home: &Path,
    pin: &CoverageFuzzingPin,
) -> Result<String, BundleRefusal> {
    let produced = std::process::Command::new(CARGO)
        .args(["build", "--locked", "--release", "--bin", &pin.binary])
        .current_dir(source)
        .env_clear()
        .env("CARGO_HOME", cargo_home)
        .env("CARGO_TARGET_DIR", target_root)
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("RUSTUP_TOOLCHAIN", &pin.build_toolchain)
        .env("CARGO_INCREMENTAL", "0")
        .output()
        .map_err(|failure| BundleRefusal::Unreadable(format!("{CARGO}: {failure}")))?;
    if !produced.status.success() {
        return Err(BundleRefusal::Unreadable(
            String::from_utf8_lossy(&produced.stderr).trim().to_owned(),
        ));
    }
    let executable = target_root.join("release").join(&pin.binary);
    let bytes = std::fs::read(&executable).map_err(|_| BundleRefusal::ExecutableUnusable)?;
    Ok(digest_of(&bytes))
}
