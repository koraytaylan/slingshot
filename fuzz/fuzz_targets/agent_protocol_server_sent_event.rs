//! Feeding arbitrary bytes to the agent event-stream decoder.
//!
//! A stream arrives in whatever pieces the transport chose, so the decoder is
//! fed whatever the fuzzer produces and must answer without panicking however
//! the bytes were split. What it must never do is keep state whose meaning
//! nobody can vouch for: a refusal discards every partial line and event it was
//! holding.

#![no_main]

use libfuzzer_sys::fuzz_target;
use slingshot_agent_connection::author_hypertext_transfer_protocol_policy::ResponseHead;
use slingshot_agent_connection::server_sent_event_decoder::{
    DecoderBounds, ServerSentEventDecoder, StreamExpectation,
};
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;
use slingshot_domain::command::schema::canonical_contract_digest;

fuzz_target!(|bytes: &[u8]| {
    let head = ResponseHead {
        alternative_service_offered: false,
        content_coding: None,
        informational: false,
        location: None,
        protocol_version: "HTTP/1.1".to_owned(),
        trailers_declared: false,
    };
    let Ok(command_contract) = SelectedCommandContractIdentity::installed("load_content_as_json")
    else {
        return;
    };
    let expectation = StreamExpectation {
        agent_event_store_generation: 1,
        daemon_subscription_identifier: "subscription".to_owned(),
        expected_provenance: ExpectedProvenance {
            canonical_json_contract_digest: canonical_contract_digest(),
            command_contract,
            transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
        },
        submitted_command_digest: "0".repeat(64),
    };
    let Ok(mut decoder) = ServerSentEventDecoder::attached(
        &head,
        "text/event-stream",
        DecoderBounds::embedded(),
        expectation,
    ) else {
        return;
    };
    let _ = decoder.push(bytes);
});
