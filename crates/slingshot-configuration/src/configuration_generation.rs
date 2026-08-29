//! One committed generation of the configuration root.
//!
//! The module map assigns this module the coordinator that reads the commit
//! inventory before and after every listed source, proves the discovered and
//! transitively referenced source set matches that inventory exactly, and
//! refuses a mixed generation. This commit declares the module as structure
//! alone.
