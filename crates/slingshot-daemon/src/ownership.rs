//! Exclusive runtime-namespace ownership.
//!
//! The module map assigns this module the daemon-lifetime owner lock, the
//! atomic readiness record, and the readiness nonce that authorizes a
//! cooperative stop. This commit declares the module alone.
