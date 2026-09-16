//! Malformed token refusals must not send a submission POST.

use super::*;
use slingshot_agent_connection::command_submission::Submission;
use slingshot_agent_connection::selected_author_submission::SubmissionSendRefusal;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, timeout};

const EXCHANGE_TIMEOUT_SECONDS: u64 = 5;
const NO_POST_OBSERVATION_MILLISECONDS: u64 = 10;

impl preflight::Preflight<'_> {
    /// Rejects malformed token responses before submission bytes reach the listener.
    pub(super) async fn reject_invalid_tokens(&self, submission: &Submission) {
        let Self { listener, transport, identity, authentication, .. } = *self;
        let oversized_token = serde_json::json!({"token": "a".repeat(
            AuthorAgentTransportContract::embedded().limit("maximum_author_response_header_bytes") as usize + 1
        )}).to_string();
        for body in [
            r#"{}"#,
            r#"{"token":""}"#,
            r#"{"token":"one","token":"two"}"#,
            r#"{"token":"one","extra":true}"#,
            r#"{"token":"a\r\nb"}"#,
            oversized_token.as_str(),
        ] {
            let peer = async {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    head.push(socket.read_u8().await.unwrap());
                }
                authentication.lend_value_bytes(|value| {
                    assert!(request::matches(
                        &head,
                        "GET /aem/libs/granite/csrf/token.json HTTP/1.1",
                        value,
                    ));
                });
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            };
            let (outcome, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
                tokio::join!(
                    transport.send_submission_with_fresh_token(
                        identity,
                        submission,
                        authentication,
                        1
                    ),
                    peer
                )
            })
            .await
            .unwrap();
            assert_eq!(outcome, Err(SubmissionSendRefusal::Request));
            assert!(
                timeout(Duration::from_millis(NO_POST_OBSERVATION_MILLISECONDS), listener.accept())
                    .await
                    .is_err(),
                "invalid token caused a POST"
            );
        }
    }
}
