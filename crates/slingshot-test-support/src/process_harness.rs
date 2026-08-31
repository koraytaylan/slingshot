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
use std::io::Read;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use rustix::fs::{Mode, OFlags, open};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};

/// How often a timed wait asks the operating system again.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// How much terminal output one read asks for.
const TERMINAL_READ_CHUNK: usize = 4096;

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
    /// The child outlived the deadline the scenario allowed it.
    #[error("the child did not finish within {0:?}")]
    DeadlineElapsed(Duration),
    /// The child has already been waited for, so its handle names nothing.
    #[error("this child has already been reaped")]
    AlreadyReaped,
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
    /// Whether the child starts from an empty environment.
    pub sealed: bool,
    /// What the child's three standard streams are connected to.
    pub attachment: StreamAttachment,
}

/// What a child's standard streams are connected to.
///
/// The distinction is visible to the child. A program that asks whether it is
/// writing to a terminal answers differently under each, so a suite that only
/// ever ran one of them would pin half the behaviour and call it all of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StreamAttachment {
    /// Pipes the harness drains itself.
    #[default]
    Redirected,
    /// A pseudo-terminal the harness holds the controlling end of.
    Terminal,
}

impl ProcessRequest {
    /// Names one invocation with its arguments.
    #[must_use]
    pub fn new(arguments: &[&str]) -> Self {
        Self {
            arguments: arguments.iter().map(|argument| (*argument).to_owned()).collect(),
            environment: BTreeMap::new(),
            sealed: false,
            attachment: StreamAttachment::Redirected,
        }
    }

    /// Starts this child from an empty environment.
    ///
    /// Whatever the machine running the suite happens to export stops here. A
    /// scenario that inherited it would read one answer on a developer's
    /// workstation and another on a build agent, and neither run would record
    /// which variable made the difference.
    #[must_use]
    pub fn sealed(mut self) -> Self {
        self.sealed = true;
        self
    }

    /// Runs this child against a pseudo-terminal instead of pipes.
    #[must_use]
    pub fn on_terminal(mut self) -> Self {
        self.attachment = StreamAttachment::Terminal;
        self
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
    if request.sealed {
        command.env_clear();
    }
    for (name, value) in &request.environment {
        command.env(name, value);
    }
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    command
}

/// Opens one pseudo-terminal and returns its controlling and follower ends.
fn open_terminal_pair() -> Result<(OwnedFd, OwnedFd), HarnessFailure> {
    let flags = OpenptFlags::RDWR | OpenptFlags::NOCTTY;
    let controller = openpt(flags).map_err(unusable)?;
    grantpt(&controller).map_err(unusable)?;
    unlockpt(&controller).map_err(unusable)?;
    let name = ptsname(&controller, Vec::new()).map_err(unusable)?;
    let follower =
        open(name.as_c_str(), OFlags::RDWR | OFlags::NOCTTY, Mode::empty()).map_err(unusable)?;
    Ok((controller, follower))
}

/// Reports one operating-system refusal as a harness failure.
fn unusable(failure: impl ToString) -> HarnessFailure {
    HarnessFailure::Unusable(failure.to_string())
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

/// How one owned child is asked to stop.
///
/// Cooperatively first, and by force only for a child this harness actually
/// owns and that would not go. The order is the point: a daemon that answers
/// its own stop request releases its endpoint and leaves its durable state
/// intact, and killing one that would have answered destroys that for nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanupRoute {
    /// It answered a stop quoting its current nonce.
    Cooperative {
        /// The nonce that was quoted.
        nonce: String,
    },
    /// It did not answer, and the retained handle ended it.
    RetainedHandle,
    /// It had already gone.
    AlreadyGone,
}

/// Why a cooperative stop was not attempted or did not work.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CleanupRefusal {
    /// The nonce named an owner that is no longer there.
    #[error("that nonce named an owner which has since been replaced")]
    NonceStale,
    /// The child is not one this harness owns.
    #[error("only a child this harness owns may be ended through a retained handle")]
    NotOwned,
}

/// What a cooperative stop knows about the daemon it is stopping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CooperativeStop {
    /// The nonce the live owner answers to.
    pub current_nonce: String,
    /// Whether it answers at all.
    pub responsive: bool,
}

impl CooperativeStop {
    /// Returns how a child answering this should be cleaned up.
    ///
    /// A stale nonce is refused rather than escalated. The owner it named has
    /// been replaced, and ending the replacement because its predecessor is
    /// gone would stop work nobody asked to stop.
    ///
    /// # Errors
    ///
    /// Returns [`CleanupRefusal::NonceStale`].
    pub fn route(&self, quoted_nonce: &str, owned: bool) -> Result<CleanupRoute, CleanupRefusal> {
        if quoted_nonce != self.current_nonce {
            return Err(CleanupRefusal::NonceStale);
        }
        if self.responsive {
            return Ok(CleanupRoute::Cooperative { nonce: quoted_nonce.to_owned() });
        }
        if owned { Ok(CleanupRoute::RetainedHandle) } else { Err(CleanupRefusal::NotOwned) }
    }
}

/// What a harness found when it checked itself at the end of a scenario.
///
/// Reported rather than cleaned up quietly. A suite that tidied away an orphan
/// would pass while leaving a process behind, and the next scenario would meet
/// it without knowing where it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeakReport {
    /// How many children were still owned when the scenario ended.
    pub orphaned_children: usize,
    /// Whether either captured stream was left unread.
    pub undrained_streams: bool,
}

impl LeakReport {
    /// Returns whether the scenario left nothing behind.
    #[must_use]
    pub fn is_clean(self) -> bool {
        self.orphaned_children == 0 && !self.undrained_streams
    }
}

impl ProcessHarness {
    /// Returns what this harness is still holding.
    #[must_use]
    pub fn leak_report(&self, undrained_streams: bool) -> LeakReport {
        LeakReport { orphaned_children: self.owned_count(), undrained_streams }
    }

    /// Returns the environment a scenario runs under, built rather than inherited.
    ///
    /// Explicit and small. A child that inherited this process's environment
    /// would read whatever the machine happened to have, and the scenario that
    /// passed here would fail on somebody else's laptop for a reason nothing
    /// recorded.
    #[must_use]
    pub fn isolated_environment(root: &Path) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("HOME".to_owned(), root.to_string_lossy().into_owned()),
            ("PATH".to_owned(), String::new()),
            ("SLINGSHOT_CONFIGURATION_ROOT".to_owned(), root.to_string_lossy().into_owned()),
        ])
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

/// A signal a harness delivers to a child it owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliverableSignal {
    /// What a person typing at a terminal sends.
    Interrupt,
    /// The termination a program may answer.
    Terminate,
    /// The termination a program cannot answer.
    Kill,
}

/// What one retained child is held by.
///
/// A process file descriptor, on the one supported platform that has them. It
/// is taken while the child is known to exist and closed only after the child
/// is waited for, so it names that child and no later occupant of its number.
#[cfg(target_os = "linux")]
type InstanceHandle = OwnedFd;

/// What one retained child is held by where no such handle exists.
///
/// The other supported platform offers no descriptor bound to one process
/// instance. Rather than fall back to signalling a number - the mistake the
/// retained handle exists to prevent - this harness starts no retained child
/// there, and the type says so by having no values.
#[cfg(not(target_os = "linux"))]
type InstanceHandle = core::convert::Infallible;

/// Takes the handle that names one running child.
#[cfg(target_os = "linux")]
fn retain_instance(child: &Child) -> Result<InstanceHandle, HarnessFailure> {
    use rustix::process::{Pid, PidfdFlags, pidfd_open};
    pidfd_open(Pid::from_child(child), PidfdFlags::empty()).map_err(unusable)
}

/// Refuses to retain a child where no instance-bound handle exists.
#[cfg(not(target_os = "linux"))]
fn retain_instance(_child: &Child) -> Result<InstanceHandle, HarnessFailure> {
    Err(HarnessFailure::Unusable(
        "this platform offers no handle bound to one process instance".to_owned(),
    ))
}

/// Delivers one signal through the handle that names the child.
#[cfg(target_os = "linux")]
fn deliver_through(
    handle: &InstanceHandle,
    signal: DeliverableSignal,
) -> Result<(), HarnessFailure> {
    use rustix::process::{Signal, pidfd_send_signal};
    let delivered = match signal {
        DeliverableSignal::Interrupt => Signal::INT,
        DeliverableSignal::Terminate => Signal::TERM,
        DeliverableSignal::Kill => Signal::KILL,
    };
    pidfd_send_signal(handle, delivered).map_err(unusable)
}

/// Delivers nothing, because no child is ever retained here.
#[cfg(not(target_os = "linux"))]
fn deliver_through(
    handle: &InstanceHandle,
    _signal: DeliverableSignal,
) -> Result<(), HarnessFailure> {
    match *handle {}
}

/// One child held by an instance-bound handle from spawn until reap.
///
/// The handle is a process file descriptor taken the moment the child exists
/// and kept until it is waited for. Everything this type does to the child goes
/// through it. That is what makes the operations safe to perform late: the
/// descriptor names *this* child, so a signal sent after the child has gone
/// fails rather than reaching whatever the operating system has since given the
/// same number to.
///
/// The numeric process identifier is recorded, and never used to find, check,
/// or signal anything.
#[derive(Debug)]
pub struct RetainedChild {
    child: Child,
    instance: InstanceHandle,
    controller: Option<OwnedFd>,
    identifier: u32,
    reaped: bool,
}

impl RetainedChild {
    /// Returns the child's numeric identifier, for correlating output only.
    #[must_use]
    pub fn identifier(&self) -> u32 {
        self.identifier
    }

    /// Reports whether this child has been waited for.
    #[must_use]
    pub fn is_reaped(&self) -> bool {
        self.reaped
    }

    /// Delivers one signal through the retained handle.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessFailure::AlreadyReaped`] once the child has been waited
    /// for, and [`HarnessFailure::Unusable`] when the operating system refuses.
    pub fn deliver(&self, signal: DeliverableSignal) -> Result<(), HarnessFailure> {
        if self.reaped {
            return Err(HarnessFailure::AlreadyReaped);
        }
        deliver_through(&self.instance, signal)
    }

    /// Waits for the child to finish inside `deadline`.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessFailure::DeadlineElapsed`] when it is still running at
    /// the end, leaving the handle valid so the caller can still end it.
    pub fn wait_within(&mut self, deadline: Duration) -> Result<ExitStatus, HarnessFailure> {
        let started = Instant::now();
        loop {
            match self.child.try_wait().map_err(unusable)? {
                Some(status) => {
                    self.reaped = true;
                    return Ok(status);
                }
                None if started.elapsed() >= deadline => {
                    return Err(HarnessFailure::DeadlineElapsed(deadline));
                }
                None => std::thread::sleep(POLL_INTERVAL),
            }
        }
    }

    /// Ends this child through the retained handle and waits for it.
    ///
    /// Used for a child that did not answer a cooperative stop. A child that
    /// has already gone is waited for and nothing is delivered.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessFailure::DeadlineElapsed`] when the child does not
    /// finish after the signal.
    pub fn end_within(&mut self, deadline: Duration) -> Result<ExitStatus, HarnessFailure> {
        if let Some(status) = self.child.try_wait().map_err(unusable)? {
            self.reaped = true;
            return Ok(status);
        }
        self.deliver(DeliverableSignal::Kill)?;
        self.wait_within(deadline)
    }

    /// Reads everything the child wrote to its terminal.
    ///
    /// Answers only for a child on a pseudo-terminal, and only once it has
    /// finished: the follower end closes with the child, and the read that
    /// follows returns the buffered bytes and then the end of the stream.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessFailure::Unusable`] when the child was not given a
    /// terminal.
    pub fn terminal_output(&mut self) -> Result<String, HarnessFailure> {
        let controller = self
            .controller
            .take()
            .ok_or_else(|| HarnessFailure::Unusable("this child has no terminal".to_owned()))?;
        let mut reader = std::fs::File::from(controller);
        let mut collected = Vec::new();
        let mut chunk = vec![0_u8; TERMINAL_READ_CHUNK];
        while let Ok(read) = reader.read(&mut chunk) {
            if read == 0 {
                break;
            }
            collected.extend_from_slice(&chunk[..read]);
        }
        Ok(String::from_utf8_lossy(&collected).into_owned())
    }
}

impl Drop for RetainedChild {
    fn drop(&mut self) {
        if !self.reaped {
            self.child.kill().ok();
            self.child.wait().ok();
        }
    }
}

impl ProcessHarness {
    /// Starts one child and keeps an instance-bound handle on it.
    ///
    /// The handle is taken before anything else happens to the child, so there
    /// is no moment at which the harness knows only a number.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessFailure::Unusable`] when the child cannot be started or
    /// the handle cannot be taken.
    pub fn start_retained(
        &self,
        executable: &ExecutablePath,
        request: &ProcessRequest,
    ) -> Result<RetainedChild, HarnessFailure> {
        let mut command = command_for(executable, request);
        let controller = self.attach_terminal(&mut command, request)?;
        let child = command.spawn().map_err(unusable)?;
        let identifier = child.id();
        let instance = retain_instance(&child)?;
        Ok(RetainedChild { child, instance, controller, identifier, reaped: false })
    }

    /// Points one command's three streams at a fresh pseudo-terminal.
    fn attach_terminal(
        &self,
        command: &mut Command,
        request: &ProcessRequest,
    ) -> Result<Option<OwnedFd>, HarnessFailure> {
        if request.attachment == StreamAttachment::Redirected {
            return Ok(None);
        }
        let (controller, follower) = open_terminal_pair()?;
        let input = follower.try_clone().map_err(unusable)?;
        let error = follower.try_clone().map_err(unusable)?;
        command.stdin(Stdio::from(input)).stdout(Stdio::from(follower)).stderr(Stdio::from(error));
        Ok(Some(controller))
    }

    /// Runs one process to completion inside `deadline`, draining as it goes.
    ///
    /// Both streams are read on their own threads for as long as the child
    /// runs, so a child that writes more than a pipe holds keeps going instead
    /// of blocking on a reader that is itself waiting for the child to exit.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessFailure::DeadlineElapsed`] when the child outlives the
    /// deadline; the child is ended through its retained handle first.
    pub fn run_within(
        &self,
        executable: &ExecutablePath,
        request: &ProcessRequest,
        deadline: Duration,
    ) -> Result<CapturedProcess, HarnessFailure> {
        let mut retained = self.start_retained(executable, request)?;
        let output = retained.child.stdout.take().map(drain_on_thread);
        let errors = retained.child.stderr.take().map(drain_on_thread);
        let waited = retained.wait_within(deadline);
        if waited.is_err() {
            retained.end_within(deadline)?;
        }
        let status = waited?;
        Ok(CapturedProcess {
            status,
            standard_output: joined(output),
            standard_error: joined(errors),
        })
    }
}

/// Reads one stream to its end on a thread of its own.
fn drain_on_thread(mut stream: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut collected = Vec::new();
        stream.read_to_end(&mut collected).ok();
        String::from_utf8_lossy(&collected).into_owned()
    })
}

/// Returns what one draining thread collected.
fn joined(handle: Option<std::thread::JoinHandle<String>>) -> String {
    handle.and_then(|handle| handle.join().ok()).unwrap_or_default()
}
