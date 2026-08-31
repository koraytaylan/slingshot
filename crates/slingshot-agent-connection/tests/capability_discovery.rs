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
            command_semantic_contract_version: "2.0.0".to_owned(),
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
