//! Endpoint identities of one runtime namespace.
//!
//! An endpoint is named from a hash of its runtime root plus the namespace
//! digest, so two processes that resolve the same target reach the same
//! endpoint and distinct roots do not collide. Unix paths use a short,
//! owner-only directory outside the caller-selected root because the operating
//! system bounds the complete socket path. The address type is
//! platform-specific and typed, so a Unix socket path can never be passed where
//! a Windows pipe name is expected.

use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
use base64::Engine;
#[cfg(unix)]
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

use slingshot_local_protocol::foundation_contract::FoundationContract;

use crate::platform_runtime::failure::PlatformFailure;

/// File-name suffix of the endpoint a Unix runtime namespace listens on.
pub const UNIX_SOCKET_SUFFIX: &str = ".socket";

/// Prefix of the short, per-runtime-root endpoint directory under `/tmp`.
///
/// Unix-domain socket limits apply to the complete path, not just the socket
/// filename. Keeping this directory outside the caller-selected runtime root
/// makes endpoint naming independent of home-directory or configuration-root
/// length while the root digest keeps distinct roots from colliding.
#[cfg(unix)]
pub const UNIX_ENDPOINT_DIRECTORY_PREFIX: &str = "slingshot-";

#[cfg(unix)]
const ENDPOINT_ROOT_DOMAIN: &[u8] = b"slingshot.endpoint-root/1";

#[cfg(unix)]
const NAMESPACE_DIGEST_HEX_CHARACTERS: usize = 64;

#[cfg(unix)]
const NAMESPACE_DIGEST_BYTE_LENGTH: usize = 32;

#[cfg(unix)]
const ROOT_DIGEST_HEX_CHARACTERS: usize = 32;

/// Prefix every Windows named pipe of this product carries.
pub const WINDOWS_PIPE_PREFIX: &str = r"\\.\pipe\slingshot-";

/// Address one runtime namespace is reachable at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointAddress {
    /// Path of a Unix domain socket.
    #[cfg(unix)]
    UnixDomainSocket(PathBuf),
    /// Name of a Windows named pipe.
    #[cfg(windows)]
    WindowsNamedPipe(String),
}

impl EndpointAddress {
    /// Returns the display form a diagnostic or readiness record carries.
    #[must_use]
    pub fn display(&self) -> String {
        #[cfg(unix)]
        let Self::UnixDomainSocket(path) = self;
        #[cfg(unix)]
        return path.display().to_string();
        #[cfg(windows)]
        let Self::WindowsNamedPipe(name) = self;
        #[cfg(windows)]
        return name.clone();
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
    let digest_bytes =
        hex::decode(namespace_digest).map_err(|_| PlatformFailure::EndpointNameTooLong {
            length: namespace_digest.len(),
            limit: NAMESPACE_DIGEST_HEX_CHARACTERS,
        })?;
    if digest_bytes.len() != NAMESPACE_DIGEST_BYTE_LENGTH {
        return Err(PlatformFailure::EndpointNameTooLong {
            length: namespace_digest.len(),
            limit: NAMESPACE_DIGEST_HEX_CHARACTERS,
        });
    }
    let endpoint_root = endpoint_root(runtime_root);
    // URL-safe base64 carries the complete 256-bit namespace digest in 43
    // filename bytes, versus 64 hexadecimal bytes, so identity is preserved
    // without exceeding macOS's complete-path socket bound.
    let compact_digest = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest_bytes);
    let path = endpoint_root.join(format!("{compact_digest}{UNIX_SOCKET_SUFFIX}"));
    let limit = contract.namespace.unix_socket_address_bytes as usize;
    let length = path.as_os_str().len();
    if length > limit {
        return Err(PlatformFailure::EndpointNameTooLong { length, limit });
    }
    Ok(EndpointAddress::UnixDomainSocket(path))
}

/// Returns the short per-root directory in which Unix endpoints live.
#[cfg(unix)]
#[must_use]
pub fn endpoint_root(runtime_root: &Path) -> PathBuf {
    let mut digest = Sha256::new();
    digest.update(ENDPOINT_ROOT_DOMAIN);
    digest.update(runtime_root.as_os_str().as_bytes());
    // 128 bits keeps collisions negligible while leaving enough room for the
    // namespace digest and suffix under macOS's 100-byte contract bound.
    let root_digest: String =
        hex::encode(digest.finalize()).chars().take(ROOT_DIGEST_HEX_CHARACTERS).collect();
    PathBuf::from("/tmp").join(format!("{UNIX_ENDPOINT_DIRECTORY_PREFIX}{root_digest}"))
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
    let _unused_on_this_row = runtime_root;
    let name = format!("{WINDOWS_PIPE_PREFIX}{namespace_digest}");
    let limit = contract.namespace.windows_named_pipe_name_code_units as usize;
    let length = name.encode_utf16().count();
    if length > limit {
        return Err(PlatformFailure::EndpointNameTooLong { length, limit });
    }
    Ok(EndpointAddress::WindowsNamedPipe(name))
}
