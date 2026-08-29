//! Probe for the Windows process-creation-flags capability.
//!
//! Requires the named detachment flags to be reachable as safe constants, so a
//! daemon can be started without a console and in its own process group without
//! repository-owned unchecked code.

use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};

#[test]
fn the_detached_creation_flags_are_named_constants() {
    let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    assert_ne!(DETACHED_PROCESS, 0);
    assert_ne!(CREATE_NEW_PROCESS_GROUP, 0);
    assert_eq!(flags & DETACHED_PROCESS, DETACHED_PROCESS);
    assert_eq!(flags & CREATE_NEW_PROCESS_GROUP, CREATE_NEW_PROCESS_GROUP);
}
