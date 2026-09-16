//! HTTP/1 submission acknowledgements preserve bound bytes and never repeat a POST.

use super::*;
use slingshot_agent_connection::command_submission::{Submission, SubmissionOutcome};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, timeout};

const EXCHANGE_TIMEOUT_SECONDS: u64 = 5;
const NO_REPEAT_OBSERVATION_MILLISECONDS: u64 = 10;

/// The original submission and independently selected acknowledgement expectations.
pub(super) struct Case<'case> {
    pub(super) submission: &'case Submission,
    pub(super) endpoint: &'case str,
    pub(super) arguments: &'case str,
    pub(super) media: &'case str,
    pub(super) wrong_echo: bool,
    pub(super) accepted: bool,
    pub(super) answer: &'case str,
}

impl preflight::Preflight<'_> {
    /// Exercises every acknowledgement job set in synchronous and asynchronous modes.
    pub(super) async fn verify_http_one(&self, case: Case<'_>) {
        let Self { listener, transport, identity, authentication, async_provider, .. } = *self;
        let Case { submission, endpoint, arguments, media, wrong_echo, accepted, answer } = case;
        let job_sets = job_sets(media, wrong_echo);
        for asynchronous in [false, true] {
            for job_set in &job_sets {
                let mut wire_answer: serde_json::Value = serde_json::from_str(answer).unwrap();
                wire_answer["physical_sling_job_identifiers"] = job_set["identifiers"].clone();
                let answer = wire_answer.to_string();
                let expected_recorded = accepted && job_set["acceptable"].as_bool().unwrap();
                let peer = async {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut head = Vec::new();
                    while !head.ends_with(b"\r\n\r\n") {
                        head.push(socket.read_u8().await.unwrap());
                    }
                    let head = String::from_utf8(head).unwrap();
                    authentication.lend_value_bytes(|value| {
                        assert!(request::matches(
                            head.as_bytes(),
                            "GET /aem/libs/granite/csrf/token.json HTTP/1.1",
                            value
                        ));
                    });
                    assert!(!head.to_ascii_lowercase().contains("csrf-token:"));
                    let body = r#"{"token":"csrf-test-value"}"#;
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                    drop(socket);
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut head = Vec::new();
                    while !head.ends_with(b"\r\n\r\n") {
                        head.push(socket.read_u8().await.unwrap());
                    }
                    let head = String::from_utf8(head).unwrap();
                    authentication.lend_value_bytes(|value| {
                        assert!(request::matches(
                            head.as_bytes(),
                            "POST /aem/bin/slingshot/agent/submit HTTP/1.1",
                            value
                        ));
                    });
                    assert!(head.contains(&format!("referer: {endpoint}/\r\n")));
                    assert!(head.contains("csrf-token: csrf-test-value\r\n"));
                    assert!(head.contains(&format!(
                        "idempotency-key: {}\r\n",
                        submission.submitted_command_digest
                    )));
                    let length: usize = head
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    let mut body = vec![0; length];
                    socket.read_exact(&mut body).await.unwrap();
                    assert_eq!(body, submission.wire_body().unwrap());
                    let decoded: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(decoded["canonical_arguments"].as_str(), Some(arguments));
                    assert_eq!(decoded["artifact_manifest"]["kind"], "empty");
                    let response = format!(
                        "HTTP/1.1 202 Accepted\r\nContent-Type: {media}\r\nContent-Length: {}\r\n\r\n{answer}",
                        answer.len()
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                };
                let (outcome, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
                    tokio::join!(
                        async {
                            if asynchronous {
                                transport
                                    .send_submission_authenticated_async_guarded(
                                        identity,
                                        submission,
                                        async_provider,
                                        &async_cases::NoClocks,
                                        &async_cases::NoClocks,
                                        1,
                                        || Ok(()),
                                    )
                                    .await
                            } else {
                                transport
                                    .send_submission_with_fresh_token(
                                        identity,
                                        submission,
                                        authentication,
                                        1,
                                    )
                                    .await
                            }
                        },
                        peer
                    )
                })
                .await
                .unwrap();
                let outcome = outcome.unwrap();
                assert_eq!(
                    outcome.provably_recorded(),
                    expected_recorded,
                    "job set {:?}, async={asynchronous}",
                    job_set["name"]
                );
                if !expected_recorded {
                    assert!(matches!(outcome, SubmissionOutcome::SubmissionUnknown { .. }));
                }
                assert!(
                    timeout(
                        Duration::from_millis(NO_REPEAT_OBSERVATION_MILLISECONDS),
                        listener.accept()
                    )
                    .await
                    .is_err(),
                    "acknowledgement triggered another request"
                );
            }
        }
    }
}

fn job_sets(media: &str, wrong_echo: bool) -> Vec<serde_json::Value> {
    if media == "application/json" && !wrong_echo {
        serde_json::from_str::<serde_json::Value>(include_str!(
            "../../fixtures/command-submission/job-sets.json"
        ))
        .unwrap()["sets"]
            .as_array()
            .unwrap()
            .clone()
    } else {
        vec![serde_json::json!({"name":"baseline", "identifiers":["job-one"], "acceptable":true})]
    }
}
