//! Probe for the shell-syntax capability.
//!
//! Requires parsing a repository script into a syntax tree whose commands are
//! reachable as structure, and refusing an unterminated construct, so script
//! policy is applied to syntax rather than to matched text.

use yash_syntax::syntax::List;

#[test]
fn a_script_parses_into_reachable_commands_and_refuses_malformed_input() {
    let script = "set -eu\nif [ -d target ]; then rm -rf target; fi\necho done\n";
    let parsed: List = script.parse().expect("the script parses");
    assert_eq!(parsed.0.len(), 3, "the script holds three complete commands");
    let rendered = parsed.to_string();
    assert!(rendered.contains("set -eu"), "{rendered}");
    assert!(rendered.contains("echo done"), "{rendered}");

    let malformed = "if [ -d target ]; then\n".parse::<List>();
    assert!(malformed.is_err(), "an unterminated construct must be refused");
}
