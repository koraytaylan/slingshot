//! Probe for the shell-syntax capability.
//!
//! Requires parsing a repository script into a syntax tree whose commands are
//! reachable as structure, and refusing an unterminated construct, so script
//! policy is applied to syntax rather than to matched text.

use brush_parser::ast::{Command, CompoundCommand};
use brush_parser::{Parser, ParserOptions};

/// Complete commands the probe script holds.
const COMPLETE_COMMAND_COUNT: usize = 3;

/// Parses one script into its syntax tree.
fn parse(script: &str) -> Result<brush_parser::ast::Program, brush_parser::ParseError> {
    let options = ParserOptions::default();
    Parser::new(std::io::Cursor::new(script.as_bytes()), &options).parse_program()
}

#[test]
fn a_script_parses_into_reachable_commands_and_refuses_malformed_input() {
    let script = "set -eu\nif [ -d target ]; then rm -rf target; fi\necho done\n";
    let parsed = parse(script).expect("the script parses");
    let commands: Vec<&Command> = parsed
        .complete_commands
        .iter()
        .flat_map(|complete| complete.0.iter())
        .flat_map(|item| item.0.first.seq.iter())
        .collect();
    assert_eq!(commands.len(), COMPLETE_COMMAND_COUNT, "every command is reachable as structure");
    let conditionals = commands
        .iter()
        .filter(|command| matches!(command, Command::Compound(CompoundCommand::IfClause(_), _)))
        .count();
    assert_eq!(conditionals, 1, "the conditional is reachable as a conditional");

    let malformed = parse("if [ -d target ]; then\n");
    assert!(malformed.is_err(), "an unterminated construct must be refused");
}
