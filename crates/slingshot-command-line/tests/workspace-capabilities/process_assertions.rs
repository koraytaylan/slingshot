//! Probe for the process-assertions capability.
//!
//! Requires running the repository executable as a real process and asserting
//! its exit status, its standard output, and that its standard error stays
//! empty on success.

use assert_cmd::Command;

#[test]
fn the_product_executable_is_asserted_as_a_real_process() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_slingshot"));
    let assertion = command.arg("--version").assert();
    let output = assertion.success().get_output().clone();
    let rendered = String::from_utf8(output.stdout).expect("the version line is text");
    assert_eq!(rendered.lines().count(), 1, "{rendered}");
    assert!(rendered.starts_with("slingshot "), "{rendered}");
    assert!(output.stderr.is_empty(), "a successful run writes no diagnostics");

    let mut refused = Command::new(env!("CARGO_BIN_EXE_slingshot"));
    refused.arg("--surprise").assert().failure();
}
