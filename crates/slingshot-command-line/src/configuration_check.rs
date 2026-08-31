//! Reporting what the selected configuration actually resolves to.
//!
//! The module map assigns this leaf the read-only inspection a caller runs
//! before trusting anything else: which profile was found, which environment it
//! names, and what refuses. It performs no work against an author, so its
//! answer is about this machine rather than about a remote one.
