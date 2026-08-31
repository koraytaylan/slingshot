//! Turning an outcome into an exit status a script can branch on.
//!
//! The module map assigns this leaf the closed mapping from what happened to
//! what the process returns. The distinctions it keeps are the ones a caller
//! must act on differently: refused, failed, unresolved, and interrupted.
