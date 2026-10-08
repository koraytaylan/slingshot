//! Retryable status decisions require valid transport, but no acceptance acknowledgement.

use super::*;
use slingshot_agent_connection::author_hypertext_transfer_protocol_policy::retry_delay_milliseconds;
use slingshot_agent_connection::command_submission::{
    RETRYABLE_STATUSES, Submission, SubmissionOutcome,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, timeout};

const EXCHANGE_TIMEOUT_SECONDS: u64 = 5;
const WIRE_RETRY_SECONDS: u64 = 5;
const WIRE_RETRY_MILLISECONDS: u64 = 5000;
const REQUEST_INSTANT_MILLISECONDS: u64 = 1;

impl preflight::Preflight<'_> {
    /// Reproduces generic JSON, malformed JSON and empty capacity responses through real sockets.
    pub(super) async fn verify_retryable_error_bodies(&self, submission: &Submission) {
        for status in RETRYABLE_STATUSES.iter().copied().chain([
            http::StatusCode::ACCEPTED.as_u16(),
            http::StatusCode::FORBIDDEN.as_u16(),
            http::StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
            http::StatusCode::CONFLICT.as_u16(),
        ]) {
            for answer in [r#"{"refusal":"capacity","detail":"synthetic-capacity"}"#, "{", ""] {
                for asynchronous in [false, true] {
                    self.retryable_error_exchange(submission, status, answer, asynchronous).await;
                }
            }
        }
    }

    async fn retryable_error_exchange(
        &self,
        submission: &Submission,
        status: u16,
        answer: &str,
        asynchronous: bool,
    ) {
        let Self { listener, transport, identity, authentication, async_provider, .. } = *self;
        let peer = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(socket.read_u8().await.unwrap());
            }
            authentication.lend_value_bytes(|value| {
                assert!(request::matches(
                    head.as_slice(),
                    "GET /aem/libs/granite/csrf/token.json HTTP/1.1",
                    value
                ))
            });
            let body = r#"{"token":"synthetic-csrf-token"}"#;
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            drop(socket);
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(socket.read_u8().await.unwrap());
            }
            authentication.lend_value_bytes(|value| {
                assert!(request::matches(
                    head.as_slice(),
                    "POST /aem/bin/slingshot/agent/submit HTTP/1.1",
                    value
                ))
            });
            let expected = submission.wire_body().unwrap();
            let mut body = vec![0; expected.len()];
            socket.read_exact(&mut body).await.unwrap();
            assert_eq!(
                body, expected,
                "the local peer did not receive the original bound submission"
            );
            let response = format!(
                "HTTP/1.1 {status} Capacity\r\nContent-Type: application/json\r\nRetry-After: {WIRE_RETRY_SECONDS}\r\nContent-Length: {}\r\n\r\n{answer}",
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
                                REQUEST_INSTANT_MILLISECONDS,
                                || Ok(()),
                            )
                            .await
                    } else {
                        transport
                            .send_submission_with_fresh_token(
                                identity,
                                submission,
                                authentication,
                                REQUEST_INSTANT_MILLISECONDS,
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
        let expected = if RETRYABLE_STATUSES.contains(&status) {
            SubmissionOutcome::RetryAfter {
                milliseconds: retry_delay_milliseconds(Some(WIRE_RETRY_MILLISECONDS)),
            }
        } else {
            SubmissionOutcome::SubmissionUnknown {
                cause: slingshot_agent_connection::command_submission::UnknownCause::Body,
            }
        };
        assert_eq!(
            outcome,
            expected,
            "status {status}, asynchronous={asynchronous}, bytes={}",
            answer.len()
        );
        assert!(
            outcome.requires_reconciliation(),
            "a retryable response bypassed lookup-first recovery"
        );
        assert!(!outcome.provably_recorded(), "an error response was treated as recorded work");
        assert!(
            timeout(Duration::from_millis(NO_REPEAT_OBSERVATION_MILLISECONDS), listener.accept())
                .await
                .is_err(),
            "a capacity response caused an automatic second POST"
        );
    }
}
