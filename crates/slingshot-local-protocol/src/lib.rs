//! Daemon request, response, event, and framing contracts.
//!
//! The workspace dependency contract lets this crate depend only on
//! `slingshot-domain`. This commit declares the retained control surface: the
//! canonical foundation contract, the length-prefixed framing, the stable
//! request and response envelopes, and the retained ping and nonce-bound stop.

pub mod control;
pub mod envelope;
pub mod foundation_contract;
pub mod framing;
pub mod message;
pub mod ping;
