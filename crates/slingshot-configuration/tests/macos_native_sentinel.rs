//! A macOS-only compilation and execution sentinel for the native PR gate.
//!
//! This stays outside the platform-runtime contract: a job that executes only
//! that contract must not be able to claim it compiled the macOS code path.

#[cfg(target_os = "macos")]
#[test]
fn the_macos_native_pull_request_gate_compiles_and_executes_this_sentinel() {
    assert_eq!(std::env::consts::OS, "macos");
}

#[cfg(not(target_os = "macos"))]
#[test]
fn non_macos_rows_keep_the_sentinel_test_target_buildable() {
    assert_ne!(std::env::consts::OS, "macos");
}
