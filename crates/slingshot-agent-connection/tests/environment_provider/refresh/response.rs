//! Independent author responses for the authentication refresh scenario matrix.

use super::*;

#[test]
fn response_status_matrix_preserves_each_attempt_and_authentication_mode() {
    for (scenario, basic, cloud) in [
        ("success", [401, 200, 401], [401, 200, 401]),
        ("twice", [401, 401, 401], [401, 401, 401]),
        ("forbidden", [403, 403, 403], [403, 403, 403]),
        ("truncated", [401, 401, 401], [401, 401, 401]),
        ("refresh-failure", [401, 401, 401], [401, 401, 401]),
        ("logical-lookup", [401, 404, 401], [401, 404, 401]),
        ("post-401", [200, 401, 401], [200, 401, 401]),
        ("post-403", [200, 403, 403], [200, 403, 403]),
        ("post-refresh-failure", [200, 401, 401], [200, 401, 401]),
        ("post-guard", [200, 401, 401], [200, 401, 401]),
        ("post-token401", [401, 200, 403], [401, 200, 403]),
        ("post-token-invalid", [200, 401, 401], [200, 401, 401]),
        ("artifact", [401, 200, 401], [401, 200, 401]),
        ("artifact-short", [200, 200, 200], [200, 200, 200]),
        ("artifact-twice", [401, 401, 401], [401, 401, 401]),
        ("artifact-401-short", [401, 401, 401], [401, 401, 401]),
        ("high-water", [200, 401, 401], [401, 200, 200]),
        ("physical-lookup", [401, 404, 401], [401, 404, 401]),
        ("event", [401, 200, 401], [401, 200, 401]),
        ("event-twice", [401, 401, 401], [401, 401, 401]),
        ("event-short", [200, 200, 200], [200, 200, 200]),
        ("event-401-short", [401, 401, 401], [401, 401, 401]),
    ] {
        for (cloud, expected) in [(false, basic), (true, cloud)] {
            for (attempt, expected) in expected.into_iter().enumerate() {
                let posting = scenario.starts_with("post-")
                    && attempt
                        == if scenario == "post-token401" {
                            POST_AFTER_TOKEN_REFRESH_INDEX
                        } else {
                            1
                        };
                assert_eq!(
                    status(scenario, cloud, attempt, posting, usize::from(cloud) + 1),
                    expected,
                    "scenario={scenario} cloud={cloud} attempt={attempt}"
                );
            }
        }
    }
}

pub(super) fn status(
    scenario: &str,
    cloud: bool,
    attempt: usize,
    posting: bool,
    high_water_token_attempts: usize,
) -> u16 {
    if scenario.starts_with("event") {
        stream_status(scenario, "event", "event-short", attempt)
    } else if scenario == "physical-lookup" {
        if attempt == 1 { MISSING_STATUS } else { UNAUTHORIZED_STATUS }
    } else if scenario == "high-water" {
        capture_status(cloud, attempt, high_water_token_attempts)
    } else if scenario.starts_with("artifact") {
        stream_status(scenario, "artifact", "artifact-short", attempt)
    } else if scenario.starts_with("post-") {
        submission_status(scenario, attempt, posting)
    } else {
        read_status(scenario, attempt)
    }
}

fn stream_status(scenario: &str, successful: &str, short: &str, attempt: usize) -> u16 {
    if scenario == short || (scenario == successful && attempt == 1) {
        SUCCESS_STATUS
    } else {
        UNAUTHORIZED_STATUS
    }
}

fn capture_status(cloud: bool, attempt: usize, token_attempts: usize) -> u16 {
    if attempt < token_attempts {
        if cloud && attempt == 0 { UNAUTHORIZED_STATUS } else { SUCCESS_STATUS }
    } else if cloud {
        SUCCESS_STATUS
    } else {
        UNAUTHORIZED_STATUS
    }
}

fn submission_status(scenario: &str, attempt: usize, posting: bool) -> u16 {
    if scenario == "post-token401" {
        if attempt == 0 {
            UNAUTHORIZED_STATUS
        } else if posting {
            FORBIDDEN_STATUS
        } else {
            SUCCESS_STATUS
        }
    } else if attempt == 0 {
        SUCCESS_STATUS
    } else if scenario == "post-403" {
        FORBIDDEN_STATUS
    } else {
        UNAUTHORIZED_STATUS
    }
}

fn read_status(scenario: &str, attempt: usize) -> u16 {
    if scenario == "forbidden" {
        FORBIDDEN_STATUS
    } else if attempt == 1 && scenario == "logical-lookup" {
        MISSING_STATUS
    } else if attempt == 1 && scenario == "success" {
        SUCCESS_STATUS
    } else {
        UNAUTHORIZED_STATUS
    }
}

pub(super) fn body(
    scenario: &str,
    status: u16,
    cloud: bool,
    high_water_posting: bool,
    submission: &slingshot_agent_connection::command_submission::Submission,
) -> String {
    if scenario == "high-water" {
        return capture_body(cloud, high_water_posting, submission);
    }
    if scenario == "post-token-invalid" {
        return r#"{"token":""}"#.into();
    }
    match status {
        SUCCESS_STATUS => success_body(scenario),
        MISSING_STATUS => missing_body(scenario, submission),
        _ => "{}".into(),
    }
}

fn success_body(scenario: &str) -> String {
    if scenario.starts_with("event") {
        if scenario == "event-short" {
            ": alive\n\nid: unfinished".into()
        } else {
            ": alive\n\n".into()
        }
    } else if scenario.starts_with("artifact") {
        if scenario == "artifact-short" { "ab".into() } else { "abc".into() }
    } else if scenario.starts_with("post-") {
        r#"{"token":"test-csrf"}"#.into()
    } else {
        "{}".into()
    }
}

fn missing_body(
    scenario: &str,
    submission: &slingshot_agent_connection::command_submission::Submission,
) -> String {
    if scenario == "physical-lookup" {
        serde_json::json!({
                    "kind":"missing", "format":"slingshot.agent/1", "transport_contract_digest":submission.provenance.transport_contract_digest,
                    "agent_event_store_generation":8, "sling_job_identifier":"job-one",
                }).to_string()
    } else {
        serde_json::json!({
            "kind":"missing", "format":"slingshot.agent/1",
            "transport_contract_digest":submission.provenance.transport_contract_digest,
            "agent_event_store_generation":submission.operation.agent_event_store_generation,
            "agent_operation_identifier":submission.operation.agent_operation_identifier,
            "author_target_identity_digest":submission.operation.author_target_identity_digest,
        })
        .to_string()
    }
}

fn capture_body(
    cloud: bool,
    posting: bool,
    submission: &slingshot_agent_connection::command_submission::Submission,
) -> String {
    if !posting {
        r#"{"token":"test-csrf"}"#.into()
    } else if cloud {
        serde_json::json!({
                    "format":"slingshot.agent/1", "transport_contract_digest":submission.provenance.transport_contract_digest,
                    "daemon_subscription_identifier":"subscription-one", "agent_event_store_generation":7,
                    "high_water_cursor":"captured-position",
                }).to_string()
    } else {
        "{}".into()
    }
}
