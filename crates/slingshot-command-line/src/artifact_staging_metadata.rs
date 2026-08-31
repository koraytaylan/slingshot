//! Remembering what a partial download was, so a retry can finish it.
//!
//! The module map assigns this leaf the sidecar facts a staged transfer
//! carries: what it is of, how long it should be, and what it should digest to.
//! A retry that could not tell would have to start again or, worse, publish
//! what it happened to have.
