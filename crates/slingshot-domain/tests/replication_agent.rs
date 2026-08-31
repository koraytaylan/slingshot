//! Assertions for listing and inspecting replication agents.
//!
//! The structural assertion is the important one: neither result has a member
//! that could hold a transport address, because a publish agent's transport
//! carries the credentials it authenticates to a publisher with. What both
//! report instead is a closed transport kind, which answers "which agent is
//! this" without answering "what are its credentials".

use serde_json::Value;
use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::find_pages_containing_phrase::PageTitle;
use slingshot_domain::command::operational_listing::ListingResultFailure;
use slingshot_domain::command::platform_service_identity::{
    ReplicationAgentIdentifier, ReplicationTransportKind,
};
use slingshot_domain::command::replication_agent::{
    InspectReplicationAgentCommand, InspectReplicationAgentResult, ListReplicationAgentsCommand,
    ListReplicationAgentsResult, ReplicationAgentMatch,
};
use slingshot_domain::command::repository_path::RepositoryPath;

/// Listing commands this test reads.
const COMMANDS: &str = include_str!("fixtures/commands/replication_agent/commands.jsonl");

/// Inspection commands this test reads.
const INSPECTION: &str = include_str!("fixtures/commands/replication_agent/inspection.jsonl");

/// Agent every vector addresses.
const AGENT: &str = "publish";

/// Entries one agent reported holding.
const QUEUED: u64 = 12;

/// Milliseconds one agent waits before retrying.
const RETRY_DELAY: u64 = 60_000;

/// Reads one row's string member.
fn text<'row>(row: &'row Value, member: &str) -> &'row str {
    row[member].as_str().unwrap_or_else(|| panic!("{member} is a string in {row}"))
}

/// Returns every row of one fixture.
fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("every fixture line is one object"))
        .collect()
}

/// Returns one agent identifier.
fn agent(value: &str) -> ReplicationAgentIdentifier {
    ReplicationAgentIdentifier::parse(value).expect("a legal identifier")
}

/// Returns one agent row.
fn matched(value: &str) -> ReplicationAgentMatch {
    ReplicationAgentMatch::new(
        agent(value),
        true,
        false,
        QUEUED,
        RepositoryPath::parse("/etc/replication/agents.author/publish").expect("a legal path"),
        PageTitle::new("Default Agent").expect("a legal title"),
        ReplicationTransportKind::Publish,
    )
    .expect("a legal row")
}

/// Returns one inspection of `AGENT`.
fn inspected() -> InspectReplicationAgentResult {
    InspectReplicationAgentResult {
        agent_identifier: agent(AGENT),
        enabled: true,
        queue_blocked: true,
        queued_entry_count: QUEUED,
        repository_path: RepositoryPath::parse("/etc/replication/agents.author/publish")
            .expect("a legal path"),
        retry_delay_milliseconds: RETRY_DELAY,
        title: PageTitle::new("Default Agent").expect("a legal title"),
        transport_kind: ReplicationTransportKind::Publish,
    }
}

#[test]
fn every_command_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(COMMANDS);
    assert!(vectors.len() >= 3, "every document shape and every refusal");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<ListReplicationAgentsCommand>(document),
        ) {
            (Some(true), Ok(parsed)) => {
                assert_eq!(
                    serde_json::to_string(&parsed).expect("a command serializes"),
                    document,
                    "{note}: rewritten differently"
                );
            }
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the command answered {parsed:?}"),
        }
    }
}

#[test]
fn every_inspection_vector_parses_exactly_as_the_fixture_says() {
    let vectors = rows(INSPECTION);
    assert!(vectors.len() >= 3, "the inspection and its refusals");
    for row in &vectors {
        let document = text(row, "document");
        let note = text(row, "note");
        match (
            row["accepted"].as_bool(),
            serde_json::from_str::<InspectReplicationAgentCommand>(document),
        ) {
            (Some(true), Ok(parsed)) => assert_eq!(
                serde_json::to_string(&parsed).expect("a command serializes"),
                document,
                "{note}: rewritten differently"
            ),
            (Some(false), Err(_)) => (),
            (_, parsed) => panic!("{note}: the command answered {parsed:?}"),
        }
    }
}

#[test]
fn no_member_of_either_answer_could_carry_a_transport_address() {
    let listed = serde_json::to_value(matched(AGENT)).expect("a row serializes");
    let members: Vec<&str> =
        listed.as_object().expect("an object").keys().map(String::as_str).collect();
    assert_eq!(
        members,
        vec![
            "agent_identifier",
            "enabled",
            "queue_blocked",
            "queued_entry_count",
            "repository_path",
            "title",
            "transport_kind",
        ]
    );
    let written = serde_json::to_string(&inspected()).expect("an inspection serializes");
    assert!(!written.contains("transport_uri"), "an inspection carried a transport address");
    assert!(!written.contains("password"), "an inspection carried a credential");
    assert!(written.contains("transport_kind"), "an inspection lost its transport kind");
}

#[test]
fn a_queued_count_is_accepted_at_its_bound_and_refused_one_past_it() {
    let bound = CommandContract::embedded().limit("maximum_replication_queue_entries");
    let build = |count: u64| {
        ReplicationAgentMatch::new(
            agent(AGENT),
            true,
            false,
            count,
            RepositoryPath::parse("/etc/replication/agents.author/publish").expect("a legal path"),
            PageTitle::new("Default Agent").expect("a legal title"),
            ReplicationTransportKind::Publish,
        )
    };
    assert!(build(bound).is_ok(), "the bound itself was refused");
    assert_eq!(build(bound + 1), Err(ListingResultFailure::TooManyRequested));
}

#[test]
fn rows_are_strictly_ascending_and_an_inspection_answers_its_own_agent() {
    assert!(ListReplicationAgentsResult::new(vec![matched("a"), matched("b")], None).is_ok());
    assert_eq!(
        ListReplicationAgentsResult::new(vec![matched("b"), matched("a")], None),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
    let asked = InspectReplicationAgentCommand { agent_identifier: agent(AGENT) };
    assert_eq!(inspected().require_answers(&asked), Ok(()));
    let elsewhere = InspectReplicationAgentCommand { agent_identifier: agent("flush") };
    assert_eq!(inspected().require_answers(&elsewhere), Err(ListingResultFailure::NotThisRequest));
}
