//! Explicit daemon start convergence.
//!
//! The module map assigns this module the connect-first, elect, recheck, and
//! single detached spawn protocol that makes concurrent start callers
//! converge on one daemon. This commit declares the module alone.
