//! Authentication family root.
//!
//! The module map assigns this family the credential parsing, signed
//! assertions, and access-token exchange the Author transport needs. This
//! commit declares the family root and its five members.

pub mod access_token_cache;
pub mod async_access_token_cache;
pub mod async_identity_management_exchange;
pub mod cloud_service_credentials;
pub mod environment_provider;
pub mod runtime_snapshot;
pub mod identity_management_exchange;
pub mod identity_management_connector;
pub mod identity_management_http1;
pub mod identity_management_http2_headers;
pub mod identity_management_http2_response;
pub mod identity_management_http2;
pub mod identity_management_client;
pub mod token_assertion;
