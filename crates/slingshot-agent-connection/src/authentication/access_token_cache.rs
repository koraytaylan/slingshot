//! In-memory lease of one cloud access token.
//!
//! The module map assigns this family member the scheduled refresh skew, the
//! minimum usable lease, and the race-safe single-flight refresh a conditional
//! unauthorized response triggers. This commit declares the module as structure
//! alone.
