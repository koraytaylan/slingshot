//! Profile documents, target resolution, and credential references.
//!
//! The workspace dependency contract lets this crate depend only on
//! `slingshot-domain`. This commit declares the crate's module families and the
//! configuration-root, credential, generation, and trust leaves as
//! documentation-only structure.

pub mod additional_certificate_authority;
pub mod configuration_generation;
pub mod configuration_root;
pub mod credential_filesystem;
pub mod credential_path;
pub mod platform_trust;
pub mod profile_loader;
pub mod profile_selection;
pub mod testing;
