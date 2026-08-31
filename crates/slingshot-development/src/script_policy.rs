//! What an executable script may hold.
//!
//! A script is the part of this repository that runs with the fewest
//! guardrails: no types, no compiler, and a word that is one thing in one
//! position and something else in another. So the two questions asked of it
//! here are the two that a reader of a shell script most often cannot answer -
//! how many ways a function can go, and what a bare number means.
//!
//! A shell has no constants, so a name is an assignment. Positions are not
//! quantities: a positional parameter, a file descriptor, and arithmetic on the
//! argument count are structure a reader can see, while a timeout, a retry
//! count, a limit, or a status is a number somebody would have to guess.

use crate::source_policy::{FIRST_LINE, LoadedPolicy, Violation, check_line_count};

/// Text that makes a number on the same line argument arithmetic rather than a quantity.
const ARGUMENT_COUNT: &str = "$#";

/// The word that discards leading arguments.
const SHIFT_WORD: &str = "shift";

/// Refuses a script quantity nobody named.
///
/// A shell has no constants, so a name is an assignment and a quantity is a
/// bare word. Positions are not quantities: a positional parameter, a file
/// descriptor, and arithmetic on the argument count are all structure a reader
/// can see, while a timeout, a retry count, a limit, or a status is a number
/// somebody has to guess the meaning of.
fn check_script_numbers(policy: &LoadedPolicy, path: &str, text: &str) -> Vec<Violation> {
    let mut violations = Vec::new();
    let rule = "numeric-value-carries-no-name";
    for (offset, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.contains(ARGUMENT_COUNT) {
            continue;
        }
        if trimmed.starts_with(SHIFT_WORD) || names_a_value(trimmed) {
            continue;
        }
        let quantities = trimmed
            .split_whitespace()
            .filter(|word| word.chars().all(|held| held.is_ascii_digit()) && !word.is_empty())
            .filter(|word| {
                word.parse::<u128>()
                    .is_ok_and(|held| held >= policy.source.smallest_meaningful_literal)
            });
        for quantity in quantities {
            violations.push(Violation::at(path, offset + FIRST_LINE, rule, quantity));
        }
    }
    violations
}

/// Returns whether one script line gives a value a name.
fn names_a_value(line: &str) -> bool {
    let assigned = line.strip_prefix("readonly ").unwrap_or(line);
    let named: String =
        assigned.chars().take_while(|held| held.is_ascii_alphanumeric() || *held == '_').collect();
    !named.is_empty() && assigned[named.len()..].starts_with('=')
}

/// Counts the decisions one shell list reaches.
fn shell_complexity(list: &brush_parser::ast::CompoundList) -> u32 {
    let mut decisions = 0_u32;
    for item in &list.0 {
        decisions += u32::try_from(item.0.additional.len()).unwrap_or_default();
        let following = item.0.additional.iter().map(following_pipeline);
        let pipelines = std::iter::once(&item.0.first).chain(following);
        for pipeline in pipelines {
            for command in &pipeline.seq {
                decisions += shell_command_complexity(command);
            }
        }
    }
    decisions
}

/// Returns the pipeline one and-or operator guards.
fn following_pipeline(operator: &brush_parser::ast::AndOr) -> &brush_parser::ast::Pipeline {
    match operator {
        brush_parser::ast::AndOr::And(pipeline) | brush_parser::ast::AndOr::Or(pipeline) => {
            pipeline
        }
    }
}

/// Counts the decisions one shell command reaches.
fn shell_command_complexity(command: &brush_parser::ast::Command) -> u32 {
    use brush_parser::ast::Command;

    match command {
        Command::Simple(_) | Command::ExtendedTest(_, _) => 0,
        Command::Function(defined) => compound_complexity(&defined.body.0),
        Command::Compound(compound, _) => compound_complexity(compound),
    }
}

/// Counts the decisions one compound shell command reaches.
fn compound_complexity(compound: &brush_parser::ast::CompoundCommand) -> u32 {
    use brush_parser::ast::CompoundCommand;

    match compound {
        CompoundCommand::Arithmetic(_) => 0,
        CompoundCommand::BraceGroup(group) => shell_complexity(&group.list),
        CompoundCommand::Subshell(subshell) => shell_complexity(&subshell.list),
        CompoundCommand::ArithmeticForClause(clause) => 1 + shell_complexity(&clause.body.list),
        CompoundCommand::ForClause(clause) => 1 + shell_complexity(&clause.body.list),
        CompoundCommand::WhileClause(clause) | CompoundCommand::UntilClause(clause) => {
            1 + shell_complexity(&clause.0) + shell_complexity(&clause.1.list)
        }
        CompoundCommand::IfClause(clause) => {
            let branches = clause.elses.as_ref().map_or(0, Vec::len);
            let otherwise = u32::try_from(branches).unwrap_or_default();
            1 + otherwise + shell_complexity(&clause.condition) + shell_complexity(&clause.then)
        }
        CompoundCommand::CaseClause(clause) => {
            u32::try_from(clause.cases.len()).unwrap_or_default().saturating_sub(1)
        }
        CompoundCommand::Coprocess(coprocess) => shell_command_complexity(&coprocess.body),
    }
}

/// Refuses every rule one executable script breaks.
#[must_use]
pub fn check(policy: &LoadedPolicy, path: &str, text: &str) -> Vec<Violation> {
    let mut violations = check_line_count(policy, path, text);
    let options = brush_parser::ParserOptions::default();
    let mut parser = brush_parser::Parser::new(std::io::Cursor::new(text.as_bytes()), &options);
    let program = match parser.parse_program() {
        Ok(program) => program,
        Err(failure) => {
            let rule = "source-is-not-parseable";
            violations.push(Violation::at(path, FIRST_LINE, rule, failure.to_string()));
            return violations;
        }
    };
    for complete in &program.complete_commands {
        for item in &complete.0 {
            for command in &item.0.first.seq {
                let brush_parser::ast::Command::Function(defined) = command else { continue };
                let name = defined.fname.value.clone();
                let line = defined.fname.loc.as_ref().map_or(FIRST_LINE, |span| span.start.line);
                if !policy.name_is_spelled_in_full(&name) {
                    let rule = "declared-name-is-not-spelled-in-full";
                    violations.push(Violation::at(path, line, rule, name.clone()));
                }
                let reached = 1 + compound_complexity(&defined.body.0);
                if reached > policy.source.maximum_cyclomatic_complexity {
                    let detail = format!("{name} reaches {reached}");
                    violations.push(Violation::at(
                        path,
                        line,
                        "function-branches-too-many-ways",
                        detail,
                    ));
                }
            }
        }
    }
    violations.extend(crate::source_policy::check_suppressions(policy, path, text));
    violations.extend(crate::source_policy::check_contract_redeclaration(policy, path, text));
    violations.extend(check_script_numbers(policy, path, text));
    violations.sort();
    violations
}
