//! Running one tool call as one durable operation.
//!
//! A tool call reaches the same daemon, the same registry command, and the same
//! operation identity a command line would reach, so the two surfaces cannot
//! disagree about what a request did.
