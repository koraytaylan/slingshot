//! The admitted submission's logical snapshot is checked before physical lookups.

use super::*;
use slingshot_agent_connection::command_submission::Submission;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, timeout};

const EXCHANGE_TIMEOUT_SECONDS: u64 = 5;

impl preflight::Preflight<'_> {
    /// Checks the logical snapshot and returns the shared physical-matrix response template.
    pub(super) async fn checked_snapshot(&self, submission: &Submission) -> String {
        let Self { listener, transport, identity, authentication, .. } = *self;
        let snapshot_body = serde_json::json!({
            "provenance": submission.provenance,
            "agent_event_store_generation": submission.operation.agent_event_store_generation,
            "agent_operation_identifier": submission.operation.agent_operation_identifier,
            "author_target_identity_digest": submission.operation.author_target_identity_digest,
            "selected_environment_revision": submission.operation.selected_environment_revision,
            "daemon_subscription_identifier": submission.daemon_subscription_identifier,
            "submitted_command_digest": submission.submitted_command_digest,
            "subscription_watermark":"cursor-010", "physical_sling_job_identifiers": ["job-one"],
            "granted_retention_milliseconds": 120000,
            "attempt": 1, "progress": 10, "sequence": 2, "kind": "progress"
        })
        .to_string();
        let peer = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            while !bytes.ends_with(b"\r\n\r\n") {
                bytes.push(socket.read_u8().await.unwrap());
            }
            let expected = format!(
                "GET /aem/bin/slingshot/agent/snapshot?agent_operation_identifier={} HTTP/1.1",
                submission.operation.agent_operation_identifier
            );
            authentication.lend_value_bytes(|value| {
                assert!(request::matches(&bytes, &expected, value));
            });
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{snapshot_body}",
                snapshot_body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        };
        let (snapshot, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
            tokio::join!(transport.lookup_snapshot(identity, submission, authentication), peer)
        })
        .await
        .unwrap();
        let receipt = snapshot.unwrap();
        assert!(receipt.remaining_retention_milliseconds <= 120000);
        let snapshot = receipt.snapshot;
        assert_eq!(snapshot.progress, 10);
        assert_eq!(snapshot.sequence.value(), 2);
        assert_eq!(snapshot.granted_retention_milliseconds, 120000);
        snapshot_body
    }
}
