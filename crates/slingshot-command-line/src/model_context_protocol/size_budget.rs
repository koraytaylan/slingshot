//! How large an answer may be before it is published rather than inlined.
//!
//! The bound belongs in one place: an answer that is too large is externalized
//! with an address rather than truncated, because a truncated answer is not a
//! smaller one - it is an unparseable one, and a client cannot tell the
//! difference between a truncated answer and a corrupt connection.
//!
//! # Every budget is stated, and their sum is checked at compile time
//!
//! A message is not one number. It is an envelope, the same envelope again as
//! escaped text, an optional link, and whatever the era's decoration adds, and
//! the interesting question is whether all of that together still fits what the
//! transport writes. Adding the pieces up here means a change to any one of
//! them fails the build rather than one very large answer at a customer.
//!
//! # Escaping is budgeted at its worst case
//!
//! A byte can become six characters when it is escaped into a string. Budgeting
//! for the average would make the bound hold for ordinary answers and fail for
//! exactly the answers that are unusual - which is when a caller most needs the
//! answer to arrive.

use crate::machine_outcome_envelope::MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES;
use crate::model_context_protocol::standard_stream_transport::MAXIMUM_QUEUED_BYTES;

/// The largest structured acknowledgement a workflow client accepts, in bytes.
///
/// Pinned by the executor on the other side rather than chosen here, which is
/// why the envelope bound has to be strictly below it: an envelope exactly at
/// the cap would leave no room for the message that carries it.
pub const PINNED_ACKNOWLEDGEMENT_CAP: u64 = 4_096;

/// How many characters one byte may become when it is escaped into a string.
pub const WORST_CASE_ESCAPE_FACTOR: u64 = 6;

/// How many bytes one target-qualified address may carry.
pub const MAXIMUM_ADDRESS_BYTES: u64 = 512;

/// How many bytes one era's decoration adds to a result.
pub const MAXIMUM_DECORATION_BYTES: u64 = 256;

/// How many bytes one protocol message may carry around its result.
pub const MAXIMUM_MESSAGE_OVERHEAD_BYTES: u64 = 512;

/// How large one complete message may be, at worst, for one answer.
pub const WORST_CASE_MESSAGE_BYTES: u64 = MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES
    + MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES * WORST_CASE_ESCAPE_FACTOR
    + MAXIMUM_ADDRESS_BYTES
    + MAXIMUM_DECORATION_BYTES
    + MAXIMUM_MESSAGE_OVERHEAD_BYTES;

/// An envelope leaves room for the message that carries it.
const _: () = assert!(MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES < PINNED_ACKNOWLEDGEMENT_CAP);

/// The worst message one answer produces still fits what the transport queues.
const _: () = assert!(WORST_CASE_MESSAGE_BYTES < MAXIMUM_QUEUED_BYTES as u64);

/// Where one answer is carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carriage {
    /// Inside the message, because it fits.
    Inline,
    /// Beside it, addressed, because it does not.
    Externalized,
}

/// Returns where one answer of `bytes` is carried.
///
/// The comparison is inclusive at the bound: an answer exactly at it fits, and
/// one byte more does not. A bound that excluded its own value would make the
/// number in the documentation wrong by one, which is the kind of wrong nobody
/// finds until it matters.
#[must_use]
pub fn carriage_of(bytes: u64) -> Carriage {
    if bytes <= MAXIMUM_MACHINE_OUTCOME_ENVELOPE_BYTES {
        Carriage::Inline
    } else {
        Carriage::Externalized
    }
}

/// Returns how large the message carrying one answer of `bytes` is, at worst.
#[must_use]
pub fn worst_case_message_of(bytes: u64) -> u64 {
    bytes
        .saturating_add(bytes.saturating_mul(WORST_CASE_ESCAPE_FACTOR))
        .saturating_add(MAXIMUM_ADDRESS_BYTES)
        .saturating_add(MAXIMUM_DECORATION_BYTES)
        .saturating_add(MAXIMUM_MESSAGE_OVERHEAD_BYTES)
}

/// Reports whether one answer of `bytes` can be carried at all.
#[must_use]
pub fn is_carriable(bytes: u64) -> bool {
    carriage_of(bytes) == Carriage::Externalized
        || worst_case_message_of(bytes) < MAXIMUM_QUEUED_BYTES as u64
}
