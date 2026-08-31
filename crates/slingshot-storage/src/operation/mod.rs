//! Operation ledger family root.
//!
//! The module map assigns this family the durable operation records. A family
//! root declares its children and nothing else, so what the reading of many
//! records at once looks like lives in the leaf beside this line.

pub mod listing;
pub mod remote_submission;
