//! Authentication and Author network transport.
//!
//! The workspace dependency contract lets this crate depend on
//! `slingshot-configuration`, `slingshot-agent-protocol`, and
//! `slingshot-domain`. This commit declares the crate's authentication family
//! and its transport-policy leaf as documentation-only structure.

pub mod authentication;
pub mod author_cross_site_request_forgery_protection;
pub mod author_hypertext_transfer_protocol_policy;
pub mod capability_discovery;
pub mod request_authentication;
pub mod transport_policy;
