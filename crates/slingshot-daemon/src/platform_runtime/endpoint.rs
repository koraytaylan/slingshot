//! Endpoint identities of one runtime namespace.
//!
//! An endpoint is named from the namespace digest alone, so two processes that
//! resolve the same target reach the same endpoint and two different targets
//! never collide. The address type is platform-specific and typed, so a Unix
//! socket path can never be passed where a Windows pipe name is expected.

use std::path::{Path, PathBuf};

use slingshot_local_protocol::foundation_contract::FoundationContract;

use crate::platform_runtime::failure::PlatformFailure;

/// File-name suffix of the endpoint a Unix runtime namespace listens on.
pub const UNIX_SOCKET_SUFFIX: &str = ".socket";

/// Prefix every Windows named pipe of this product carries.
pub const WINDOWS_PIPE_PREFIX: &str = r"\\.\pipe\slingshot-";

/// Address one runtime namespace is reachable at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointAddress {
    /// Path of a Unix domain socket.
    UnixDomainSocket(PathBuf),
    /// Name of a Windows named pipe.
    WindowsNamedPipe(String),
}

impl EndpointAddress {
    /// Returns the display form a diagnostic or readiness record carries.
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::UnixDomainSocket(path) => path.display().to_string(),
            Self::WindowsNamedPipe(name) => name.clone(),
        }
    }
}

/// Builds the endpoint address of one runtime namespace.
///
/// # Errors
///
/// Returns [`PlatformFailure::EndpointNameTooLong`] when the address is beyond
/// the bound the foundation contract declares for this platform.
#[cfg(unix)]
pub fn endpoint_address(
    contract: &FoundationContract,
    runtime_root: &Path,
    namespace_digest: &str,
) -> Result<EndpointAddress, PlatformFailure> {
    let path = runtime_root.join(format!("{namespace_digest}{UNIX_SOCKET_SUFFIX}"));
    let limit = contract.namespace.unix_socket_address_bytes as usize;
    let length = path.as_os_str().len();
    if length > limit {
        return Err(PlatformFailure::EndpointNameTooLong { length, limit });
    }
    Ok(EndpointAddress::UnixDomainSocket(path))
}

/// Builds the endpoint address of one runtime namespace.
///
/// # Errors
///
/// Returns [`PlatformFailure::EndpointNameTooLong`] when the address is beyond
/// the bound the foundation contract declares for this platform.
#[cfg(windows)]
pub fn endpoint_address(
    contract: &FoundationContract,
    runtime_root: &Path,
    namespace_digest: &str,
) -> Result<EndpointAddress, PlatformFailure> {
    let _unused = runtime_root;
    let name = format!("{WINDOWS_PIPE_PREFIX}{namespace_digest}");
    let limit = contract.namespace.windows_named_pipe_name_code_units as usize;
    let length = name.encode_utf16().count();
    if length > limit {
        return Err(PlatformFailure::EndpointNameTooLong { length, limit });
    }
    Ok(EndpointAddress::WindowsNamedPipe(name))
}
