//! Probe for the Structured Query Language syntax capability.
//!
//! Requires parsing a migration with the SQLite dialect, walking the parsed
//! statements to reach declared names, and refusing malformed input, so a
//! migration is checked as code rather than as text.

use sqlparser::ast::Statement;
use sqlparser::dialect::SQLiteDialect;
use sqlparser::parser::Parser;

#[test]
fn a_migration_parses_into_statements_that_expose_their_declared_names() {
    let migration = "CREATE TABLE operation (identifier TEXT PRIMARY KEY) STRICT; \
                     CREATE INDEX operation_state_index ON operation (identifier);";
    let statements = Parser::parse_sql(&SQLiteDialect {}, migration).expect("the migration parses");
    assert_eq!(statements.len(), 2);
    let declared: Vec<String> = statements
        .iter()
        .map(|statement| match statement {
            Statement::CreateTable(created) => created.name.to_string(),
            Statement::CreateIndex(created) => {
                created.name.as_ref().map(ToString::to_string).unwrap_or_default()
            }
            other => panic!("unexpected statement {other}"),
        })
        .collect();
    assert_eq!(declared, vec!["operation".to_owned(), "operation_state_index".to_owned()]);

    let malformed = Parser::parse_sql(&SQLiteDialect {}, "CREATE TABLE (");
    assert!(malformed.is_err(), "malformed input must be refused");
}
