//! Capability probes owned by the configuration crate.

mod base64_encoding;
#[cfg(unix)]
mod descriptor_relative_filesystem;
#[cfg(target_os = "macos")]
mod extended_access_control_lists;
#[cfg(unix)]
mod operating_system_account_database;
mod platform_directories;
#[cfg(target_os = "linux")]
mod posix_access_control_lists;
mod secret_buffers;
mod temporary_files;
mod toml_documents;
mod uniform_resource_locators;
#[cfg(target_os = "windows")]
mod windows_file_identity;
#[cfg(target_os = "windows")]
mod windows_security_identifiers;
