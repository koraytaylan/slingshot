//! Fake Adobe Identity Management Services endpoint.
//!
//! The module map assigns this module the in-process server that answers a
//! bounded exchange request with a scripted head, body, and trailer section, so
//! every response decision is provable without network access. This commit
//! declares the module as structure alone.
