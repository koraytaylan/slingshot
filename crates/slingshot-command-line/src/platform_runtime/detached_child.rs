//! Creation of the detached daemon child.
//!
//! An elected client starts the daemon as a child that outlives it. The child
//! never inherits the client's protocol streams, and it is placed outside the
//! client's process group, so the daemon survives the client exiting and no
//! signal aimed at the client's group reaches it. The client never lends the
//! child a lock: the daemon acquires its own ownership after it starts.

use std::path::Path;
use std::process::{Child, Command, Stdio};

/// Detaches one prepared command from its caller.
///
/// The child's standard input, output, and error are all discarded, so the
/// client's own result stream stays free of daemon diagnostics, and the child
/// is placed outside the caller's process group or console.
pub fn detach(command: &mut Command) {
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    apply_detachment(command);
}

/// Places the child outside the caller's process group and session.
#[cfg(unix)]
fn apply_detachment(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    /// Process group identifier that asks for a new group led by the child.
    const NEW_PROCESS_GROUP: i32 = 0;

    command.process_group(NEW_PROCESS_GROUP);
}

/// Places the child outside the caller's console and process group.
#[cfg(windows)]
fn apply_detachment(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};

    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

/// Starts one detached daemon child and returns its handle.
///
/// The returned handle is the only way to observe or end the child. Nothing in
/// this module reads the child's numeric process identifier as authority.
///
/// # Errors
///
/// Returns the operating-system failure that prevented the child from starting.
pub fn spawn_detached(executable: &Path, arguments: &[String]) -> std::io::Result<Child> {
    let mut command = Command::new(executable);
    command.args(arguments);
    detach(&mut command);
    command.spawn()
}
