//! Reusable operating-system process harnesses.
//!
//! The harness accepts paths and plain process inputs. It never names a type
//! from the command line, the daemon, configuration, the author transport, or
//! the repository tooling, so any of those can be driven as a real process
//! without the harness knowing anything about them.
//!
//! Every child a harness starts is owned: it is accounted for, waited for, and
//! reaped, whether the test succeeds or fails.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Barrier};

/// One executable named by path alone.
///
/// The value carries no build, provenance, or ownership meaning. Whoever
/// produced the executable proves that separately; a harness only runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutablePath {
    path: PathBuf,
}

/// Reason a harness refused to run something.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HarnessFailure {
    /// The named path is not an existing file.
    #[error("{0} is not an executable file")]
    NotAnExecutable(PathBuf),
    /// The child could not be started, waited for, or read.
    #[error("the child process could not be used: {0}")]
    Unusable(String),
}

impl ExecutablePath {
    /// Names one executable by path.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessFailure::NotAnExecutable`] when the path is not an
    /// existing file.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, HarnessFailure> {
        let path = path.into();
        if path.is_file() { Ok(Self { path }) } else { Err(HarnessFailure::NotAnExecutable(path)) }
    }

    /// Returns the path of this executable.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Everything one child process produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedProcess {
    /// Status the child exited with.
    pub status: ExitStatus,
    /// Everything the child wrote to its result stream.
    pub standard_output: String,
    /// Everything the child wrote to its diagnostic stream.
    pub standard_error: String,
}

impl CapturedProcess {
    /// Returns the single line the child wrote to its result stream.
    ///
    /// # Panics
    ///
    /// Panics when the result stream does not hold exactly one line, because a
    /// command that writes anything else has already broken its contract.
    #[must_use]
    pub fn single_result_line(&self) -> &str {
        let mut lines = self.standard_output.lines();
        let first = lines.next().unwrap_or_else(|| {
            panic!(
                "the result stream is empty; the diagnostic stream held {:?}",
                self.standard_error
            )
        });
        assert!(
            lines.next().is_none(),
            "the result stream held more than one line: {:?}",
            self.standard_output
        );
        first
    }
}

/// What one process is asked to run.
#[derive(Debug, Clone, Default)]
pub struct ProcessRequest {
    /// Arguments after the executable path.
    pub arguments: Vec<String>,
    /// Environment values set for this child alone.
    pub environment: BTreeMap<String, String>,
}

impl ProcessRequest {
    /// Names one invocation with its arguments.
    #[must_use]
    pub fn new(arguments: &[&str]) -> Self {
        Self {
            arguments: arguments.iter().map(|argument| (*argument).to_owned()).collect(),
            environment: BTreeMap::new(),
        }
    }

    /// Adds one environment value this child alone sees.
    #[must_use]
    pub fn with_environment(mut self, name: &str, value: impl AsRef<OsStr>) -> Self {
        self.environment.insert(name.to_owned(), value.as_ref().to_string_lossy().into_owned());
        self
    }
}

/// Builds the command one request describes.
fn command_for(executable: &ExecutablePath, request: &ProcessRequest) -> Command {
    let mut command = Command::new(executable.path());
    command.args(&request.arguments);
    for (name, value) in &request.environment {
        command.env(name, value);
    }
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    command
}

/// A harness that owns every child it starts.
#[derive(Debug, Default)]
pub struct ProcessHarness {
    owned: Vec<Child>,
}

impl ProcessHarness {
    /// Creates a harness that owns nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs one process to completion and captures everything it produced.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessFailure::Unusable`] when the child cannot be started or
    /// waited for.
    pub fn run(
        &self,
        executable: &ExecutablePath,
        request: &ProcessRequest,
    ) -> Result<CapturedProcess, HarnessFailure> {
        let produced = command_for(executable, request)
            .output()
            .map_err(|failure| HarnessFailure::Unusable(failure.to_string()))?;
        Ok(CapturedProcess {
            status: produced.status,
            standard_output: String::from_utf8_lossy(&produced.stdout).into_owned(),
            standard_error: String::from_utf8_lossy(&produced.stderr).into_owned(),
        })
    }

    /// Starts one process this harness keeps until it is reaped.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessFailure::Unusable`] when the child cannot be started.
    pub fn start(
        &mut self,
        executable: &ExecutablePath,
        request: &ProcessRequest,
    ) -> Result<u32, HarnessFailure> {
        let child = command_for(executable, request)
            .spawn()
            .map_err(|failure| HarnessFailure::Unusable(failure.to_string()))?;
        let identifier = child.id();
        self.owned.push(child);
        Ok(identifier)
    }

    /// Returns how many children this harness still owns.
    #[must_use]
    pub fn owned_count(&self) -> usize {
        self.owned.len()
    }

    /// Ends and reaps every child this harness still owns.
    ///
    /// Each child is ended through the handle this harness kept, never by
    /// looking a numeric process identifier up and signalling it.
    pub fn reap_all(&mut self) -> Vec<ExitStatus> {
        let mut statuses = Vec::new();
        for mut child in std::mem::take(&mut self.owned) {
            match child.try_wait() {
                Ok(Some(status)) => statuses.push(status),
                Ok(None) => {
                    child.kill().ok();
                    if let Ok(status) = child.wait() {
                        statuses.push(status);
                    }
                }
                Err(_) => {}
            }
        }
        statuses
    }
}

impl Drop for ProcessHarness {
    fn drop(&mut self) {
        self.reap_all();
    }
}

/// A barrier that releases a whole cohort of callers at once.
#[derive(Debug, Clone)]
pub struct ReleaseBarrier {
    inner: Arc<Barrier>,
}

impl ReleaseBarrier {
    /// Creates a barrier that releases exactly `members` callers together.
    #[must_use]
    pub fn new(members: usize) -> Self {
        Self { inner: Arc::new(Barrier::new(members)) }
    }

    /// Waits until every member of the cohort has arrived.
    pub fn release(&self) {
        self.inner.wait();
    }
}
