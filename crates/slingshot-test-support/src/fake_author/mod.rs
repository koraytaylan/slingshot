//! An author the tests can drive, on loopback, without an AEM instance.
//!
//! The whole point is to exercise the language-neutral contract rather than to
//! imitate a Java runtime. Nothing here claims that AEM or a provider secret
//! service executed anything: what it claims is that the wire contract this
//! workspace writes down can be spoken by something else, and that a client
//! holding up its end reaches the outcomes the contract says it should.
//!
//! Behaviour comes from a validated script rather than from code paths a test
//! reaches by arranging conditions. That difference matters at the edges: a
//! script can produce a duplicate physical job record, a lost lease, a stream
//! that stops mid-event, or a generation that has been rebuilt, all on demand
//! and in a fixed order. Arranging those by timing would make the tests
//! probabilistic, which is the one thing a transport suite cannot afford.
//!
//! Nothing waits on a clock. Delays, heartbeats, closures, and releases are all
//! driven through explicit checkpoints, so a slow machine runs the same test as
//! a fast one.

pub mod authority;
pub mod outbox;
pub mod recording;
pub mod script;
pub mod server;
