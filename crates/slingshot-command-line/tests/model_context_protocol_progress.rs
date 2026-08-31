//! What a watching client is told, and what stops being told to it.
//!
//! Two properties carry the weight. Progress never goes backwards, because a
//! durable sequence repeats after a reconnect and a client watching a bar move
//! backwards learns something false. And cancelling cancels nothing remote: a
//! client that walks away stops receiving an answer, and the work it started
//! keeps running for whoever asks next.

use slingshot_command_line::model_context_protocol::progress_and_cancellation::{
    Cancelled, ProgressRegistry, Reported,
};

/// The request every case watches.
const REQUEST: &str = "request-one";

/// The token that request correlates its progress with.
const TOKEN: &str = "token-one";

/// The revision a watch begins from.
const FIRST_REVISION: u64 = 4;

/// A revision after it.
const LATER_REVISION: u64 = 5;

/// A revision after that.
const LATEST_REVISION: u64 = 6;

#[test]
fn a_report_that_says_something_new_is_forwarded() {
    let mut registry = ProgressRegistry::new();
    registry.attach(REQUEST, TOKEN, FIRST_REVISION);
    assert_eq!(registry.report(REQUEST, LATER_REVISION), Reported::Forwarded);
    assert_eq!(registry.report(REQUEST, LATEST_REVISION), Reported::Forwarded);
    assert_eq!(registry.token_of(REQUEST), Some(TOKEN));
}

#[test]
fn a_report_that_repeats_or_precedes_what_was_said_is_dropped() {
    let mut registry = ProgressRegistry::new();
    registry.attach(REQUEST, TOKEN, FIRST_REVISION);
    assert_eq!(
        registry.report(REQUEST, FIRST_REVISION),
        Reported::Dropped,
        "a repeat says nothing"
    );
    registry.report(REQUEST, LATEST_REVISION);
    assert_eq!(
        registry.report(REQUEST, LATER_REVISION),
        Reported::Dropped,
        "a client watching a bar move backwards learns something false"
    );
    assert_eq!(registry.report(REQUEST, LATEST_REVISION), Reported::Dropped);
}

#[test]
fn a_report_about_something_nobody_is_watching_reaches_nobody() {
    let mut registry = ProgressRegistry::new();
    assert_eq!(registry.report("never-attached", LATER_REVISION), Reported::Detached);
}

#[test]
fn cancelling_detaches_the_client_and_asks_nothing_remote_to_stop() {
    let mut registry = ProgressRegistry::new();
    registry.attach(REQUEST, TOKEN, FIRST_REVISION);
    assert_eq!(registry.cancel(REQUEST), Cancelled::Detached);
    assert_eq!(registry.watching(), 0);
    assert_eq!(
        registry.remote_cancellations(),
        0,
        "stop telling me and undo it are not the same request"
    );
    assert_eq!(registry.report(REQUEST, LATEST_REVISION), Reported::Detached);
    assert_eq!(registry.cancel(REQUEST), Cancelled::Unknown, "one cancellation, not two");
}

#[test]
fn detaching_everything_is_idempotent_and_still_asks_nothing_remote_to_stop() {
    let mut registry = ProgressRegistry::new();
    for identifier in ["one", "two", "three"] {
        registry.attach(identifier, TOKEN, FIRST_REVISION);
    }
    let detached = registry.detach_all();
    assert_eq!(detached, vec!["one".to_owned(), "three".to_owned(), "two".to_owned()]);
    assert_eq!(registry.watching(), 0);
    assert!(registry.detach_all().is_empty(), "a second detachment finds nothing");
    assert_eq!(registry.remote_cancellations(), 0);
}

#[test]
fn a_reconnecting_client_resumes_from_where_it_was_told_to() {
    let mut registry = ProgressRegistry::new();
    registry.attach(REQUEST, TOKEN, FIRST_REVISION);
    registry.report(REQUEST, LATER_REVISION);
    registry.attach(REQUEST, TOKEN, LATER_REVISION);
    assert_eq!(
        registry.report(REQUEST, LATER_REVISION),
        Reported::Dropped,
        "the durable sequence repeats after a reconnect, and the repeat says nothing"
    );
    assert_eq!(registry.report(REQUEST, LATEST_REVISION), Reported::Forwarded);
}
