//! Finding a disagreement while it is still cheap to have one.
//!
//! Discovery happens before a reservation, before a submission, and before
//! anything is written down. A daemon that submitted first and discovered
//! afterwards would have created work it cannot follow, against a remote system
//! that may already be running it - so every test here is about a refusal that
//! arrives before that point, with its own reason.

use slingshot_agent_connection::capability_discovery::{
    AdvertisedCapabilities, DiscoveryRefusal, RequiredCapabilities,
};
use slingshot_agent_protocol::capabilities::REQUIRED_CAPABILITY_REVISION;

#[test]
fn an_agent_without_the_command_is_told_apart_from_one_holding_another_version() {
    use slingshot_agent_connection::capability_discovery::{
        CapabilityExchangeRefusal, decode_capabilities,
    };
    let held = matching();
    let document = |contracts: serde_json::Value| {
        serde_json::to_vec(&serde_json::json!({
            "format": "slingshot.agent/1",
            "capability_revision": held.capability_revision,
            "agent_event_store_generation": held.agent_event_store_generation,
            "canonical_json_contract_digest": held.canonical_json_contract_digest,
            "command_contracts": contracts,
            "continuation_authority_ready": true,
            "transport_contract_digest": held.transport_contract_digest,
        }))
        .unwrap()
    };
    let requirement = required(Some(GENERATION));
    assert_eq!(
        decode_capabilities(&document(serde_json::json!([])), &requirement),
        Err(CapabilityExchangeRefusal::NotServed),
        "an agent that holds no version of the command does not serve it"
    );
    let mut other_version = serde_json::to_value(&held.command_contracts[0]).unwrap();
    other_version["result_schema_digest"] = serde_json::json!("0".repeat(64));
    assert_eq!(
        decode_capabilities(&document(serde_json::json!([other_version])), &requirement),
        Err(CapabilityExchangeRefusal::Incompatible),
        "an agent that holds another version of the command is a different build"
    );
}

#[test]
fn closed_capability_document_refuses_ambiguous_or_incomplete_wire_evidence() {
    use slingshot_agent_connection::capability_discovery::decode_capabilities;
    let held = matching();
    let document = serde_json::json!({
        "format": "slingshot.agent/1",
        "capability_revision": held.capability_revision,
        "agent_event_store_generation": held.agent_event_store_generation,
        "canonical_json_contract_digest": held.canonical_json_contract_digest,
        "command_contracts": held.command_contracts,
        "continuation_authority_ready": true,
        "transport_contract_digest": held.transport_contract_digest,
    });
    let requirement = required(Some(GENERATION));
    assert_eq!(
        decode_capabilities(&serde_json::to_vec(&document).unwrap(), &requirement).unwrap(),
        held
    );
    for key in document.as_object().unwrap().keys() {
        let mut changed = document.clone();
        changed.as_object_mut().unwrap().remove(key);
        assert!(
            decode_capabilities(&serde_json::to_vec(&changed).unwrap(), &requirement).is_err(),
            "missing {key}"
        );
    }
    for (key, value) in [
        ("format", serde_json::json!("slingshot.agent/2")),
        ("capability_revision", serde_json::json!(REQUIRED_CAPABILITY_REVISION + 1)),
        ("agent_event_store_generation", serde_json::json!(0)),
        ("agent_event_store_generation", serde_json::json!(GENERATION + 1)),
        ("continuation_authority_ready", serde_json::json!(false)),
        ("extra", serde_json::json!(true)),
        (
            "command_contracts",
            serde_json::json!([held.command_contracts[0], held.command_contracts[0]]),
        ),
    ] {
        let mut changed = document.clone();
        changed[key] = value;
        assert!(decode_capabilities(&serde_json::to_vec(&changed).unwrap(), &requirement).is_err());
    }
    let duplicate = document.to_string().replacen('{', "{\"format\":\"slingshot.agent/1\",", 1);
    assert!(decode_capabilities(duplicate.as_bytes(), &requirement).is_err());
    for (key, value) in [
        ("argument_schema_digest", "bad".to_owned()),
        ("result_schema_digest", "A".repeat(64)),
        ("command_contract_limits_digest", "".to_owned()),
        ("command_semantic_contract_version", "v".repeat(65)),
        ("command_wire_name", "x".repeat(97)),
    ] {
        let mut changed = document.clone();
        let mut other = changed["command_contracts"][0].clone();
        other["command_wire_name"] = serde_json::json!("another_command");
        other[key] = serde_json::json!(value);
        changed["command_contracts"].as_array_mut().unwrap().push(other);
        assert!(
            decode_capabilities(&serde_json::to_vec(&changed).unwrap(), &requirement).is_err(),
            "malformed unselected {key}"
        );
    }
    let mut oversized = serde_json::to_vec(&document).unwrap();
    oversized.resize(
        AuthorAgentTransportContract::embedded().limit("maximum_agent_protocol_document_bytes")
            as usize
            + 1,
        b' ',
    );
    assert!(decode_capabilities(&oversized, &requirement).is_err());
}
use slingshot_agent_protocol::identity::WireContractIdentity;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

/// Two-character pairs in a sixty-four-character hexadecimal value.
const DIGEST_PAIRS: usize = 32;

/// The command these fixtures use.
const COMMAND: &str = "query_paths";

/// The generation the agent serves.
const GENERATION: u64 = 4;

/// Returns a sixty-four-character value made of one repeated pair.
fn digest(pair: &str) -> String {
    pair.repeat(DIGEST_PAIRS)
}

/// Returns the contract this build holds.
fn installed() -> SelectedCommandContractIdentity {
    SelectedCommandContractIdentity::installed(COMMAND).expect("an installed command")
}

/// Returns what this daemon requires, following `expected_generation`.
fn required(expected_generation: Option<u64>) -> RequiredCapabilities {
    RequiredCapabilities::of(installed(), &canonical_contract_digest(), expected_generation)
}

/// Returns an agent advertising exactly what this build has.
fn matching() -> AdvertisedCapabilities {
    AdvertisedCapabilities {
        agent_event_store_generation: GENERATION,
        capability_revision: REQUIRED_CAPABILITY_REVISION,
        canonical_json_contract_digest: canonical_contract_digest(),
        command_contracts: vec![WireContractIdentity::from(&installed())],
        continuation_authority_ready: true,
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
    }
}

#[test]
fn an_agent_advertising_what_this_build_has_is_one_it_may_use() {
    required(Some(GENERATION)).require_compatible(&matching()).expect("a matching agent");
    required(None)
        .require_compatible(&matching())
        .expect("and a daemon with no rows yet does not care which generation");
}

#[test]
fn transport_disagreement_is_reported_before_anything_else_is_looked_at() {
    let elsewhere = AdvertisedCapabilities {
        transport_contract_digest: digest("70"),
        canonical_json_contract_digest: digest("c1"),
        command_contracts: Vec::new(),
        continuation_authority_ready: false,
        agent_event_store_generation: GENERATION + 1,
        capability_revision: REQUIRED_CAPABILITY_REVISION,
    };
    assert!(
        matches!(
            required(Some(GENERATION)).require_compatible(&elsewhere),
            Err(DiscoveryRefusal::TransportContractIncompatible { .. })
        ),
        "two sides that cannot agree how to talk have nothing to say about what they hold"
    );
}

#[test]
fn a_canonical_contract_disagreement_is_its_own_finding() {
    let drifted =
        AdvertisedCapabilities { canonical_json_contract_digest: digest("c1"), ..matching() };
    assert!(
        matches!(
            required(Some(GENERATION)).require_compatible(&drifted),
            Err(DiscoveryRefusal::CanonicalContractIncompatible { .. })
        ),
        "what a well-formed document is has to be agreed before what documents mean"
    );
}

#[test]
fn an_agent_holding_a_different_build_s_command_is_refused_by_name() {
    for changed in [
        WireContractIdentity {
            argument_schema_digest: digest("ff"),
            ..WireContractIdentity::from(&installed())
        },
        WireContractIdentity {
            result_schema_digest: digest("ff"),
            ..WireContractIdentity::from(&installed())
        },
        WireContractIdentity {
            command_contract_limits_digest: digest("ff"),
            ..WireContractIdentity::from(&installed())
        },
        WireContractIdentity {
            command_semantic_contract_version: "0.0.0".to_owned(),
            ..WireContractIdentity::from(&installed())
        },
    ] {
        let advertised = AdvertisedCapabilities { command_contracts: vec![changed], ..matching() };
        let refused = required(Some(GENERATION)).require_compatible(&advertised);
        assert!(
            matches!(
                refused,
                Err(DiscoveryRefusal::CommandContractAbsent { ref command_wire_name })
                    if command_wire_name == COMMAND
            ),
            "the refusal names which command, because an operator has to find the build that \
             disagrees: {refused:?}"
        );
    }

    let none = AdvertisedCapabilities { command_contracts: Vec::new(), ..matching() };
    assert!(matches!(
        required(Some(GENERATION)).require_compatible(&none),
        Err(DiscoveryRefusal::CommandContractAbsent { .. })
    ));
}

#[test]
fn an_agent_holding_several_contracts_is_matched_on_the_one_that_agrees() {
    let advertised = AdvertisedCapabilities {
        command_contracts: vec![
            WireContractIdentity {
                command_wire_name: "create_page".to_owned(),
                ..WireContractIdentity::from(&installed())
            },
            WireContractIdentity::from(&installed()),
        ],
        ..matching()
    };
    required(Some(GENERATION))
        .require_compatible(&advertised)
        .expect("holding other commands as well is ordinary");
}

#[test]
fn an_authority_that_is_not_ready_is_found_before_a_paged_query_begins() {
    let unready = AdvertisedCapabilities { continuation_authority_ready: false, ..matching() };
    assert_eq!(
        required(Some(GENERATION)).require_compatible(&unready),
        Err(DiscoveryRefusal::ContinuationAuthorityNotReady),
        "an agent that cannot issue lasting tokens is worth knowing about before paging, not \
         halfway through it"
    );
}

#[test]
fn a_store_rebuilt_under_a_daemon_that_has_rows_is_refused() {
    let rebuilt =
        AdvertisedCapabilities { agent_event_store_generation: GENERATION + 1, ..matching() };
    let refused = required(Some(GENERATION)).require_compatible(&rebuilt);
    assert!(
        matches!(
            refused,
            Err(DiscoveryRefusal::GenerationChanged { advertised, expected })
                if advertised == GENERATION + 1 && expected == GENERATION
        ),
        "rows referring to a store that no longer exists are not rows to carry on from: {refused:?}"
    );
    required(None)
        .require_compatible(&rebuilt)
        .expect("while a daemon holding nothing has nothing stranded by a rebuild");
}

#[test]
fn an_agent_built_before_a_behavioural_fix_is_refused_before_work_is_sent() {
    // Every digest agrees here: this is the deployed agent that speaks the
    // right format and holds the right contracts and still answers a refusal in
    // a shape this client cannot read. Only the behavioural revision tells the
    // two apart, and the refusal says which direction the build is behind.
    let older =
        AdvertisedCapabilities { capability_revision: REQUIRED_CAPABILITY_REVISION, ..matching() };
    let refused = required(Some(GENERATION)).require_compatible(&older);
    assert!(
        matches!(
            refused,
            Ok(()) if REQUIRED_CAPABILITY_REVISION == 0
        ),
        "an agent that predates a behavioural fix was treated as compatible: {refused:?}"
    );
}

#[test]
fn an_agent_built_for_a_newer_client_is_refused_as_this_build_being_old() {
    let newer = AdvertisedCapabilities {
        capability_revision: REQUIRED_CAPABILITY_REVISION + 1,
        ..matching()
    };
    let refused = required(Some(GENERATION)).require_compatible(&newer);
    assert!(
        matches!(
            refused,
            Err(DiscoveryRefusal::CapabilityRevisionTooNew { advertised, required })
                if advertised == REQUIRED_CAPABILITY_REVISION + 1
                    && required == REQUIRED_CAPABILITY_REVISION
        ),
        "an agent ahead of this client is a different finding from one behind it: {refused:?}"
    );
}
