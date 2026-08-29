//! Resolution of the configuration root from the operating-system account.
//!
//! The module map assigns this module the account-derived home directory, the
//! literal configuration components appended to it, and the absolute directory
//! handles every later read opens through. This commit declares the module as
//! structure alone.
