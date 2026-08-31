//! Where a conversation with an author can break, and what may be said after.
//!
//! The failure is never the interesting part. What matters is the answer
//! afterwards: provably did not run, may have run, or ran and the answer was
//! lost. Those are three different facts with three different consequences, and
//! a transport that collapsed them would make a caller either repeat work that
//! already happened or abandon work that succeeded.
//!
//! The dividing line is the first byte of the request. Nothing sent means
//! nothing ran; anything sent means the far side may have acted, and no local
//! evidence can say otherwise.

use slingshot_test_support::network_fault_script::{
    EVERY_NETWORK_FAULT, ExecutionKnowledge, NetworkFault, NetworkFaultScript, RequestPhase,
};

/// How many times a stubborn script breaks before it works.
const STUBBORN_OCCURRENCES: u32 = 3;

#[test]
fn nothing_sent_means_nothing_ran_and_anything_sent_means_a_question() {
    for fault in EVERY_NETWORK_FAULT {
        let held = fault.knowledge();
        let before_any_byte = matches!(
            fault,
            NetworkFault::NameUnresolved
                | NetworkFault::ConnectionRefused
                | NetworkFault::HandshakeFailed
                | NetworkFault::BeforeRequestBytes
        );
        assert_eq!(
            held == ExecutionKnowledge::ConfirmedNotExecuted,
            before_any_byte,
            "{fault:?} disagrees about whether anything was sent"
        );
    }
}

#[test]
fn an_answer_that_began_arriving_means_the_work_ran() {
    for fault in [
        NetworkFault::AfterResponseHead,
        NetworkFault::DuringResponseBody,
        NetworkFault::AfterCompleteResponse,
    ] {
        assert_eq!(
            fault.knowledge(),
            ExecutionKnowledge::RemoteOutcomeUnknown,
            "{fault:?} lost an answer to work that had already been done"
        );
    }
}

#[test]
fn only_a_fault_before_the_first_byte_permits_sending_it_again() {
    for fault in EVERY_NETWORK_FAULT {
        assert_eq!(
            fault.permits_plain_retry(),
            fault.knowledge() == ExecutionKnowledge::ConfirmedNotExecuted,
            "{fault:?} would be sent again on a guess"
        );
        assert_eq!(fault.requires_lookup(), !fault.permits_plain_retry());
    }
    let looked_up = EVERY_NETWORK_FAULT.iter().filter(|fault| fault.requires_lookup()).count();
    assert_eq!(looked_up, RECONCILED_FAULTS, "every fault past the first byte is reconciled");
}

/// How many faults have to be reconciled rather than retried.
const RECONCILED_FAULTS: usize = 5;

#[test]
fn connecting_and_securing_are_separate_phases_with_separate_blame() {
    assert_eq!(NetworkFault::NameUnresolved.phase(), RequestPhase::Connecting);
    assert_eq!(NetworkFault::ConnectionRefused.phase(), RequestPhase::Connecting);
    assert_eq!(
        NetworkFault::HandshakeFailed.phase(),
        RequestPhase::Securing,
        "a refused certificate is not a slow network"
    );
    let phases: std::collections::BTreeSet<String> =
        EVERY_NETWORK_FAULT.iter().map(|fault| format!("{:?}", fault.phase())).collect();
    assert_eq!(phases.len(), EVERY_PHASE_COUNT, "every phase has at least one fault");
}

/// How many phases one request has.
const EVERY_PHASE_COUNT: usize = 4;

#[test]
fn a_script_breaks_as_many_times_as_it_says_and_then_stops() {
    let once = NetworkFaultScript::once(NetworkFault::AfterRequestComplete);
    assert!(once.breaks_on(0));
    assert!(!once.breaks_on(1), "a script that broke forever would prove nothing about recovery");

    let stubborn = NetworkFaultScript {
        fault: NetworkFault::DuringResponseBody,
        occurrences: STUBBORN_OCCURRENCES,
    };
    for attempt in 0..STUBBORN_OCCURRENCES {
        assert!(stubborn.breaks_on(attempt), "attempt {attempt} should have broken");
    }
    assert!(!stubborn.breaks_on(STUBBORN_OCCURRENCES));
}

#[test]
fn every_fault_is_named_once_and_the_inventory_is_closed() {
    let named: std::collections::BTreeSet<String> =
        EVERY_NETWORK_FAULT.iter().map(|fault| format!("{fault:?}")).collect();
    assert_eq!(named.len(), EVERY_NETWORK_FAULT.len(), "a fault is named twice");
    let knowledge: std::collections::BTreeSet<String> =
        EVERY_NETWORK_FAULT.iter().map(|fault| format!("{:?}", fault.knowledge())).collect();
    assert_eq!(knowledge.len(), EVERY_KNOWLEDGE_COUNT, "every kind of answer is reachable");
}

/// How many honest answers there are about execution.
const EVERY_KNOWLEDGE_COUNT: usize = 3;
