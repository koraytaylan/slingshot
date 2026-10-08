//! Repeated artifact reads release delivered bodies and request reservations.

#![cfg(target_os = "linux")]

use serde_json::Value;
use slingshot_command_line::machine_outcome_envelope::MachineOutcomeEnvelope;
use slingshot_command_line::model_context_protocol::application::{Served, ServerApplication};
use slingshot_command_line::model_context_protocol::operation_execution::{
    FetchedArtifact, ResourceNamespace, ToolRunner,
};
use slingshot_command_line::model_context_protocol::standard_stream_transport::{
    LineSink, Written, maximum_line_bytes, maximum_queued_bytes,
};
use slingshot_command_line::model_context_protocol::tool_catalog::ToolDescriptor;

const TARGET: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WARMUP_READS: usize = 64;
const MEASURED_READS: usize = 400;
const SMALL_ARTIFACT_BYTES: usize = 128;
const LINE_HEADROOM_DIVISOR: usize = 8;
const RESIDENT_HEADROOM_MULTIPLIER: usize = 2;
const BYTES_PER_KIBIBYTE: usize = 1024;
const SINGLE_DELIVERY: usize = 1;

struct ArtifactSource {
    bytes: Vec<u8>,
}

impl ToolRunner for ArtifactSource {
    fn run(
        &mut self,
        _tool: &ToolDescriptor,
        _arguments: &Value,
    ) -> Result<MachineOutcomeEnvelope, String> {
        Err("this fixture only reads artifacts".to_owned())
    }

    fn artifact_bytes(
        &mut self,
        _namespace: &ResourceNamespace,
        _operation_identifier: &str,
        artifact_identifier: &str,
        maximum_bytes: u64,
    ) -> Result<FetchedArtifact, String> {
        use sha2::{Digest as _, Sha256};
        assert!(self.bytes.len() as u64 <= maximum_bytes);
        Ok(FetchedArtifact {
            artifact_identifier: artifact_identifier.to_owned(),
            author_target_identity_digest: TARGET.to_owned(),
            byte_length: self.bytes.len() as u64,
            content_digest: hex::encode(Sha256::digest(&self.bytes)),
            media_type: "text/plain".to_owned(),
            bytes: self.bytes.clone(),
        })
    }
}

struct DiscardingSink {
    expected_bytes: usize,
    delivered: usize,
}

impl LineSink for DiscardingSink {
    fn write_line(&mut self, line: &str) -> Written {
        let document: Value = serde_json::from_str(line).unwrap();
        assert_eq!(document["id"], "reusable");
        assert_eq!(
            document["result"]["contents"][0]["text"].as_str().unwrap().len(),
            self.expected_bytes
        );
        self.delivered += SINGLE_DELIVERY;
        Written::Complete
    }
}

fn resident_bytes() -> usize {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse::<usize>()
        .unwrap()
        * BYTES_PER_KIBIBYTE
}

#[test]
fn small_and_large_artifact_sessions_release_every_delivered_body() {
    let request = format!(
        r#"{{"id":"reusable","method":"resources/read","params":{{"protocolVersion":"2026-07-28","uri":"slingshot://profiles/local/environments/author/targets/{TARGET}/operations/held/artifacts/body"}}}}"#
    );
    for size in [SMALL_ARTIFACT_BYTES, maximum_line_bytes() / LINE_HEADROOM_DIVISOR] {
        let mut application =
            ServerApplication::over(Some(Box::new(ArtifactSource { bytes: vec![b'x'; size] })));
        let mut sink = DiscardingSink { expected_bytes: size, delivered: 0 };
        let mut read = || {
            assert!(matches!(application.serve_line(request.as_bytes()), Served::Answered(_)));
            assert_eq!(application.active(), SINGLE_DELIVERY);
            assert!(application.write_output(&mut sink));
            assert_eq!(application.active(), 0);
        };
        for _ in 0..WARMUP_READS {
            read();
        }
        let before = resident_bytes();
        for _ in 0..MEASURED_READS {
            read();
        }
        let growth = resident_bytes().saturating_sub(before);
        assert!(
            growth < maximum_queued_bytes() * RESIDENT_HEADROOM_MULTIPLIER,
            "delivered artifact bodies grew resident memory by {growth} bytes"
        );
        assert_eq!(sink.delivered, WARMUP_READS + MEASURED_READS);
    }
}
