//! Authentication family root.
//!
//! The module map assigns this family the credential parsing, signed
//! assertions, and access-token exchange the Author transport needs. This
//! commit declares the family root and its five members.

pub mod access_token_cache;
pub mod cloud_service_credentials;
pub mod environment_provider;
pub mod identity_management_exchange;
pub mod token_assertion;
