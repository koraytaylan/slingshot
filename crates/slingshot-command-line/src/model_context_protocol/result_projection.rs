//! Turning one command result into what a protocol client receives.
//!
//! The projection is exact: a client and a command line reading the same result
//! see the same values, because both read the one validated document rather than
//! two renderings of it.
