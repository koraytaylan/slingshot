//! Bounded operator evidence that never carries a secret out with it.
//!
//! A detached daemon has nobody watching its terminal, so what it records is
//! the only account of what it did. Two things follow. It has to be bounded,
//! because a sink that grows without limit eventually consumes the space the
//! operations themselves need. And it has to be redacted before anything is
//! written, because a file on disk outlives the moment that produced it and
//! nobody rereads an old log looking for a secret they left in it.
//!
//! Redaction happens on the way in rather than on the way out. A record that
//! reached the sink carrying a token is a record that has already leaked; the
//! only useful place to remove one is before the bytes exist.
//!
//! The bounds are the runtime contract's, and diagnostics are accounted
//! separately from operations and artifacts. Diagnostics filling up must never
//! be the reason an operation cannot store its result, and an operation store
//! at its limit must never be the reason an operator loses the record of why.

use std::io::Write as _;

use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;

/// What one redacted value is replaced with.
pub const REDACTION: &str = "[redacted]";

/// What a redacted filesystem path is replaced with.
pub const REDACTED_PATH: &str = "[path]";

/// Suffix a rotated diagnostic file carries, before its ordinal.
pub const ROTATED_SUFFIX: &str = ".rotated";

/// Name of the file records are appended to.
pub const ACTIVE_FILE_NAME: &str = "diagnostics.log";

/// Names whose value is a secret wherever one of them appears.
///
/// Matched as a name followed by a separator, so the value after it goes and
/// the sentence around it stays. A record saying an exchange failed is worth
/// keeping; the credential it failed with is not.
pub const SECRET_NAMES: &[&str] = &[
    "access_token",
    "assertion",
    "authorization",
    "client_secret",
    "id_token",
    "password",
    "private_key",
    "refresh_token",
    "secret",
    "token",
];

/// Openings of a block whose whole content is secret.
pub const SECRET_BLOCK_OPENINGS: &[&str] =
    &["-----BEGIN PRIVATE KEY-----", "-----BEGIN RSA PRIVATE KEY-----", "Bearer "];

/// Characters that can separate a secret's name from its value.
const NAME_SEPARATORS: &[char] = &['=', ':'];

/// Characters of a drive-lettered path before its separator.
const DRIVE_PREFIX_CHARACTERS: usize = 2;

/// Reason a diagnostic sink could not be used.
#[derive(Debug, thiserror::Error)]
pub enum DiagnosticFailure {
    /// The filesystem refused.
    #[error("the diagnostic sink could not be written: {0}")]
    FilesystemRefused(String),
    /// The sink directory is not one this user alone owns.
    #[error("the diagnostic directory is not a plain directory this user alone owns")]
    NotPrivate,
}

/// Returns a filesystem refusal as this module's failure.
fn refused(failure: std::io::Error) -> DiagnosticFailure {
    DiagnosticFailure::FilesystemRefused(failure.to_string())
}

/// The bounds a diagnostic sink is held to.
///
/// Read from the runtime contract rather than declared here, and kept apart
/// from the operation and artifact bounds on purpose: the two must never be
/// able to exhaust each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticBounds {
    /// Bytes one field may occupy before it is truncated.
    pub field_bytes: usize,
    /// Bytes one file may reach before it rotates.
    pub file_bytes: u64,
    /// Bytes one record may occupy before it is truncated.
    pub record_bytes: usize,
    /// Files kept beside the active one.
    pub retained_files: usize,
    /// Bytes every diagnostic file may occupy together.
    pub total_bytes: u64,
}

impl DiagnosticBounds {
    /// Returns the bounds the embedded runtime contract names.
    #[must_use]
    pub fn embedded() -> Self {
        let contract = DaemonRuntimeContract::embedded();
        Self {
            field_bytes: usize::try_from(contract.limit("maximum_diagnostic_field_bytes"))
                .unwrap_or(usize::MAX),
            file_bytes: contract.limit("maximum_diagnostic_file_bytes"),
            record_bytes: usize::try_from(contract.limit("maximum_diagnostic_record_bytes"))
                .unwrap_or(usize::MAX),
            retained_files: usize::try_from(contract.limit("maximum_retained_diagnostic_files"))
                .unwrap_or_default(),
            total_bytes: contract.formula("maximum_total_diagnostic_bytes"),
        }
    }
}

/// Returns `text` with every secret and filesystem path removed.
///
/// Applied to the whole record rather than to fields a caller remembered to
/// mark, because the records that leak are the ones nobody thought to mark.
#[must_use]
pub fn redact(text: &str) -> String {
    let without_blocks = redact_blocks(text);
    let without_named = redact_named_values(&without_blocks);
    redact_paths(&without_named)
}

/// Replaces everything from a secret block's opening to the end of its word.
fn redact_blocks(text: &str) -> String {
    let mut held = text.to_owned();
    for opening in SECRET_BLOCK_OPENINGS {
        while let Some(start) = held.find(opening) {
            let after = start + opening.len();
            let end =
                held[after..].find(char::is_whitespace).map_or(held.len(), |offset| after + offset);
            held.replace_range(start..end, REDACTION);
        }
    }
    held
}

/// Replaces the value following any name this module treats as a secret.
fn redact_named_values(text: &str) -> String {
    text.split_whitespace().map(redact_word).collect::<Vec<String>>().join(" ")
}

/// Returns one word with its value removed when its name names a secret.
fn redact_word(word: &str) -> String {
    let Some(separator) = word.find(NAME_SEPARATORS) else {
        return word.to_owned();
    };
    let name = word[..separator].trim_start_matches(|character: char| !character.is_alphanumeric());
    let names_a_secret = SECRET_NAMES.iter().any(|secret| name.eq_ignore_ascii_case(secret));
    if names_a_secret {
        format!("{}{}{REDACTION}", &word[..separator], &word[separator..=separator])
    } else {
        word.to_owned()
    }
}

/// Replaces anything that reads as a filesystem path.
///
/// A path is not a secret in itself, but it names where one is kept, and a
/// record that says which file failed to parse has told a reader where to go
/// looking.
fn redact_paths(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            let absolute = word.starts_with('/') || word.starts_with("~/");
            let drive_lettered = word.len() > DRIVE_PREFIX_CHARACTERS
                && word.as_bytes()[1] == b':'
                && word.as_bytes()[0].is_ascii_alphabetic()
                && (word.as_bytes()[DRIVE_PREFIX_CHARACTERS] == b'\\'
                    || word.as_bytes()[DRIVE_PREFIX_CHARACTERS] == b'/');
            if absolute || drive_lettered { REDACTED_PATH.to_owned() } else { word.to_owned() }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// Bounded health a status response may carry.
///
/// Counts and byte totals, and never a path. An operator learns whether the
/// sink is working and how much of its budget is gone; nobody learns where to
/// go looking for the files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SinkHealth {
    /// Bytes the active file holds.
    pub active_bytes: u64,
    /// Files kept beside the active one.
    pub rotated_files: usize,
    /// Bytes every diagnostic file holds together.
    pub total_bytes: u64,
    /// Bytes they may hold together.
    pub total_limit: u64,
}

/// One daemon's diagnostic sink, rooted in its own directory.
#[derive(Debug)]
pub struct DiagnosticSink {
    /// The bounds this sink is held to.
    bounds: DiagnosticBounds,
    /// The directory the files live in.
    directory: std::path::PathBuf,
}

impl DiagnosticSink {
    /// Returns a sink writing into `directory`, creating it if it is absent.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticFailure::NotPrivate`] when what is there is not a
    /// directory this user alone owns, or
    /// [`DiagnosticFailure::FilesystemRefused`].
    pub fn open(
        directory: &std::path::Path,
        bounds: DiagnosticBounds,
    ) -> Result<Self, DiagnosticFailure> {
        crate::platform_runtime::current_user::create_owner_only_directory(directory)
            .map_err(|failure| DiagnosticFailure::FilesystemRefused(failure.to_string()))?;
        let metadata = std::fs::symlink_metadata(directory).map_err(refused)?;
        let private = crate::platform_runtime::current_user::is_owner_only(directory)
            .map_err(|failure| DiagnosticFailure::FilesystemRefused(failure.to_string()))?;
        if !metadata.is_dir() || !private {
            return Err(DiagnosticFailure::NotPrivate);
        }
        Ok(Self { bounds, directory: directory.to_path_buf() })
    }

    /// Returns where the active file lives.
    #[must_use]
    pub fn active_path(&self) -> std::path::PathBuf {
        self.directory.join(ACTIVE_FILE_NAME)
    }

    /// Returns where the rotated file at `ordinal` lives.
    #[must_use]
    pub fn rotated_path(&self, ordinal: usize) -> std::path::PathBuf {
        self.directory.join(format!("{ACTIVE_FILE_NAME}{ROTATED_SUFFIX}.{ordinal}"))
    }

    /// Records one line, redacted and bounded, rotating first if it must.
    ///
    /// The record is redacted before it is measured, so truncating can never
    /// leave half a secret behind, and it is measured before it is written, so
    /// a file never grows past its bound and then gets trimmed.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticFailure::FilesystemRefused`].
    pub fn record(&self, text: &str) -> Result<(), DiagnosticFailure> {
        let mut line = redact(text);
        line.truncate(bounded_end(&line, self.bounds.record_bytes));
        line.push('\n');
        let written = u64::try_from(line.len()).unwrap_or(u64::MAX);
        if self.active_length()?.saturating_add(written) > self.bounds.file_bytes {
            self.rotate()?;
        }
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.active_path())
            .map_err(refused)?;
        file.write_all(line.as_bytes()).map_err(refused)?;
        file.sync_all().map_err(refused)
    }

    /// Moves the active file aside and discards the oldest beyond the bound.
    ///
    /// Oldest first, by renaming each rotated file up one ordinal. A rotation
    /// interrupted part way therefore leaves files that are all still whole -
    /// possibly one duplicated, never one truncated.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticFailure::FilesystemRefused`].
    pub fn rotate(&self) -> Result<(), DiagnosticFailure> {
        if !self.active_path().exists() {
            return Ok(());
        }
        let oldest = self.rotated_path(self.bounds.retained_files);
        if oldest.exists() {
            std::fs::remove_file(&oldest).map_err(refused)?;
        }
        for ordinal in (1..self.bounds.retained_files).rev() {
            let held = self.rotated_path(ordinal);
            if held.exists() {
                std::fs::rename(&held, self.rotated_path(ordinal + 1)).map_err(refused)?;
            }
        }
        if self.bounds.retained_files == 0 {
            return std::fs::remove_file(self.active_path()).map_err(refused);
        }
        std::fs::rename(self.active_path(), self.rotated_path(1)).map_err(refused)?;
        Ok(())
    }

    /// Returns how much of its budget this sink has used.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticFailure::FilesystemRefused`].
    pub fn health(&self) -> Result<SinkHealth, DiagnosticFailure> {
        let mut total = self.active_length()?;
        let mut rotated = 0_usize;
        for ordinal in 1..=self.bounds.retained_files {
            let path = self.rotated_path(ordinal);
            if let Ok(metadata) = std::fs::metadata(&path) {
                total = total.saturating_add(metadata.len());
                rotated += 1;
            }
        }
        Ok(SinkHealth {
            active_bytes: self.active_length()?,
            rotated_files: rotated,
            total_bytes: total,
            total_limit: self.bounds.total_bytes,
        })
    }

    /// Returns how long the active file is, or zero when there is none.
    fn active_length(&self) -> Result<u64, DiagnosticFailure> {
        match std::fs::metadata(self.active_path()) {
            Ok(metadata) => Ok(metadata.len()),
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => Ok(0),
            Err(failure) => Err(refused(failure)),
        }
    }
}

/// Returns where `text` may be cut so it fits `allowed` bytes.
///
/// On a character boundary, because a record cut mid-character would not be
/// text any more, and a diagnostic that cannot be read is not a diagnostic.
fn bounded_end(text: &str, allowed: usize) -> usize {
    if text.len() <= allowed {
        return text.len();
    }
    let mut end = allowed;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}
