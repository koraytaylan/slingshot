//! Missing and retired operation lookup checks on the submission listener.

use super::*;
use slingshot_agent_connection::command_submission::Submission;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, timeout};

const MISSING_STATUS: u16 = 404;
const RETIRED_STATUS: u16 = 410;
const EXCHANGE_TIMEOUT_SECONDS: u64 = 5;

#[derive(PartialEq, Eq)]
enum LookupMode {
    Direct,
    Negotiated,
    Authenticated,
    AuthenticatedAsync,
}

impl preflight::Preflight<'_> {
    /// Checks authenticated absence proofs without replacing the submission listener.
    pub(super) async fn lookup_absent(&self, submission: &Submission) {
        let Self {
            listener,
            transport,
            identity,
            authentication,
            async_provider,
            provider,
            source,
            ..
        } = *self;
        for mode in [
            LookupMode::Direct,
            LookupMode::Negotiated,
            LookupMode::Authenticated,
            LookupMode::AuthenticatedAsync,
        ] {
            for (status, kind) in [(MISSING_STATUS, "missing"), (RETIRED_STATUS, "retired")] {
                let mut document = serde_json::json!({
                    "kind": kind,
                    "agent_event_store_generation": submission.operation.agent_event_store_generation,
                    "agent_operation_identifier": submission.operation.agent_operation_identifier,
                    "author_target_identity_digest": submission.operation.author_target_identity_digest,
                });
                if status == MISSING_STATUS {
                    document["format"] = serde_json::json!("slingshot.agent/1");
                    document["transport_contract_digest"] =
                        serde_json::json!(submission.provenance.transport_contract_digest);
                } else {
                    document["provenance"] = serde_json::json!(submission.provenance);
                    document["selected_environment_revision"] =
                        serde_json::json!(submission.operation.selected_environment_revision);
                    document["daemon_subscription_identifier"] =
                        serde_json::json!(submission.daemon_subscription_identifier);
                    document["submitted_command_digest"] =
                        serde_json::json!(submission.submitted_command_digest);
                }
                let body = document.to_string();
                let peer = async {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut bytes = Vec::new();
                    while !bytes.ends_with(b"\r\n\r\n") {
                        bytes.push(socket.read_u8().await.unwrap());
                    }
                    authentication.lend_value_bytes(|value| {
                        assert!(request_matches(
                            &bytes,
                            &submission.operation.agent_operation_identifier,
                            value,
                        ));
                    });
                    let response = format!(
                        "HTTP/1.1 {status} Absent\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                        body.len()
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                };
                let (receipt, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
                    tokio::join!(
                        async {
                            if mode == LookupMode::AuthenticatedAsync {
                                transport
                                    .lookup_operation_authenticated_async(
                                        identity,
                                        submission,
                                        async_provider,
                                        &async_cases::NoClocks,
                                        &async_cases::NoClocks,
                                    )
                                    .await
                            } else if mode == LookupMode::Authenticated {
                                transport
                                    .lookup_operation_authenticated(
                                        identity, submission, provider, source, READING,
                                    )
                                    .await
                            } else if mode == LookupMode::Negotiated {
                                transport
                                    .lookup_operation_negotiated(
                                        identity,
                                        submission,
                                        authentication,
                                    )
                                    .await
                            } else {
                                transport
                                    .lookup_operation(identity, submission, authentication)
                                    .await
                            }
                        },
                        peer
                    )
                })
                .await
                .unwrap();
                use slingshot_agent_connection::{
                    job_snapshot_reconciliation::LookupAnswer,
                    selected_author_lookup::OperationLookupReceipt,
                };
                assert!(matches!(
                    (status, receipt.unwrap()),
                    (MISSING_STATUS, OperationLookupReceipt::Absent(LookupAnswer::Missing))
                        | (
                            RETIRED_STATUS,
                            OperationLookupReceipt::Absent(LookupAnswer::Retired(_))
                        )
                ));
            }
        }
    }
}

fn request_matches(bytes: &[u8], identifier: &str, authentication: &[u8]) -> bool {
    let expected = format!(
        "GET /aem/bin/slingshot/agent/snapshot?agent_operation_identifier={identifier} HTTP/1.1"
    );
    request::matches(bytes, &expected, authentication)
}

#[test]
fn absence_request_requires_the_exact_operation() {
    let request = b"GET /aem/bin/slingshot/agent/snapshot?agent_operation_identifier=operation HTTP/1.1\r\nAuthorization: Basic fixture\r\n\r\n";
    assert!(request_matches(request, "operation", b"Basic fixture"));
    assert!(!request_matches(request, "another-operation", b"Basic fixture"));
    let surplus = String::from_utf8(request.to_vec())
        .unwrap()
        .replace("=operation HTTP/1.1", "=operation&unexpected=true HTTP/1.1");
    assert!(!request_matches(surplus.as_bytes(), "operation", b"Basic fixture"));
}

#[test]
fn absence_request_requires_one_matching_authorization() {
    let route =
        "GET /aem/bin/slingshot/agent/snapshot?agent_operation_identifier=operation HTTP/1.1\r\n";
    for headers in [
        "\r\n",
        "Authorization: Basic other\r\n\r\n",
        "Authorization: Basic fixture\r\nAuthorization: Basic fixture\r\n\r\n",
        "Authorization: Basic fixture\r\nauthorization: Basic other\r\n\r\n",
        "X-Authorization: Basic fixture\r\n\r\n",
    ] {
        assert!(!request_matches(
            format!("{route}{headers}").as_bytes(),
            "operation",
            b"Basic fixture"
        ));
    }
    assert!(request_matches(
        format!("{route}authorization: Basic fixture\r\n\r\n").as_bytes(),
        "operation",
        b"Basic fixture"
    ));
}
