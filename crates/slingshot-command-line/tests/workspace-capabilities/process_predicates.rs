//! Probe for the process-predicates capability.
//!
//! Requires composable assertions over captured output, including a negated
//! predicate, so a rendered result can be checked for what it must not contain.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn composed_predicates_accept_and_refuse_captured_output() {
    let expected = predicate::str::starts_with("slingshot ")
        .and(predicate::str::contains("\n"))
        .and(predicate::str::contains("secret").not());
    Command::new(env!("CARGO_BIN_EXE_slingshot"))
        .arg("--version")
        .assert()
        .success()
        .stdout(expected);

    let refusing = predicate::str::contains("slingshot ").not();
    assert!(!refusing.eval("slingshot 0.1.0"), "the negated predicate refuses a match");
    assert!(refusing.eval("something else"), "the negated predicate accepts a mismatch");
}
