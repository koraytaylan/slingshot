//! Fixed-endpoint, platform-trust-only identity-management TLS establishment.

use super::identity_management_exchange::{ExchangeFailure, identity_management_endpoint};
use crate::transport_policy::IdentityManagementTrustInput;
use rustls::{ClientConfig, RootCertStore};
use rustls_pki_types::{CertificateDer, ServerName};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode as Code, ProfileAuthenticationContract,
};
use std::sync::Arc;
use tokio::{net::TcpStream, time::Duration};
use tokio_rustls::{TlsConnector, client::TlsStream};

/// The only negotiated protocols the identity-management route can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityManagementHttpProtocol {
    /// HTTP/1.1, including a TLS peer that omits ALPN.
    Http1,
    /// HTTP/2 on the same authenticated connection.
    Http2,
}

/// One TLS-authenticated connection; no HTTP request has been written yet.
pub struct IdentityManagementConnection {
    protocol: IdentityManagementHttpProtocol,
    stream: TlsStream<TcpStream>,
}
impl core::fmt::Debug for IdentityManagementConnection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("IdentityManagementConnection([redacted])")
    }
}
impl IdentityManagementConnection {
    /// Dispatches the chosen codec on this socket without probing/reconnecting.
    pub fn into_parts(self) -> (IdentityManagementHttpProtocol, TlsStream<TcpStream>) {
        (self.protocol, self.stream)
    }
}

/// Direct connector with immutable platform roots and no address/proxy override.
pub struct IdentityManagementConnector {
    configuration: Arc<ClientConfig>,
}
impl core::fmt::Debug for IdentityManagementConnector {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("IdentityManagementConnector([redacted])")
    }
}
impl IdentityManagementConnector {
    /// Freezes the route-specific platform trust. Author extensions cannot be
    /// supplied through this type; ambient trust and proxies are never loaded.
    pub fn new(trust: &IdentityManagementTrustInput) -> Result<Self, ExchangeFailure> {
        let mut roots = RootCertStore::empty();
        for der in trust.roots() {
            roots
                .add(CertificateDer::from(der.clone()))
                .map_err(|_| failure(Code::IdentityManagementTlsFailed))?;
        }
        if roots.is_empty() {
            return Err(failure(Code::IdentityManagementTlsFailed));
        }
        let mut configuration =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
                .map_err(|_| failure(Code::IdentityManagementTlsFailed))?
                .with_root_certificates(roots)
                .with_no_client_auth();
        configuration.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Ok(Self { configuration: Arc::new(configuration) })
    }

    /// Resolves/dials only the manifest IMS host and port, then authenticates it
    /// with independent connect and handshake deadlines. Dropping this future
    /// drops any connected socket; it never spawns a background connection task.
    /// The caller must additionally own the whole-exchange deadline.
    pub async fn connect(&self) -> Result<IdentityManagementConnection, ExchangeFailure> {
        let endpoint = url::Url::parse(&identity_management_endpoint())
            .map_err(|_| failure(Code::IdentityManagementTransportFailed))?;
        let host =
            endpoint.host_str().ok_or_else(|| failure(Code::IdentityManagementTransportFailed))?;
        let limits = &ProfileAuthenticationContract::embedded().limits;
        let port = u16::try_from(limits.identity_management_port)
            .map_err(|_| failure(Code::IdentityManagementTransportFailed))?;
        let socket = crate::connection_phase::within(
            Duration::from_millis(limits.identity_management_connect_timeout_milliseconds),
            async {
                TcpStream::connect((host, port))
                    .await
                    .map_err(|_| failure(Code::IdentityManagementTransportFailed))
            },
            failure(Code::IdentityManagementConnectTimeout),
        )
        .await?;
        self.handshake(
            socket,
            host.to_owned(),
            Duration::from_millis(limits.identity_management_tls_handshake_timeout_milliseconds),
        )
        .await
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(super) async fn connect_test_socket(
        &self,
        socket: TcpStream,
    ) -> Result<IdentityManagementConnection, ExchangeFailure> {
        let endpoint = url::Url::parse(&identity_management_endpoint()).unwrap();
        self.handshake(
            socket,
            endpoint.host_str().unwrap().to_owned(),
            Duration::from_millis(
                ProfileAuthenticationContract::embedded()
                    .limits
                    .identity_management_tls_handshake_timeout_milliseconds,
            ),
        )
        .await
    }

    async fn handshake(
        &self,
        socket: TcpStream,
        host: String,
        duration: Duration,
    ) -> Result<IdentityManagementConnection, ExchangeFailure> {
        let name =
            ServerName::try_from(host).map_err(|_| failure(Code::IdentityManagementTlsFailed))?;
        let stream = crate::connection_phase::within(
            duration,
            async {
                TlsConnector::from(self.configuration.clone())
                    .connect(name, socket)
                    .await
                    .map_err(|_| failure(Code::IdentityManagementTlsFailed))
            },
            failure(Code::IdentityManagementTlsHandshakeTimeout),
        )
        .await?;
        let protocol = match stream.get_ref().1.alpn_protocol() {
            Some(b"h2") => IdentityManagementHttpProtocol::Http2,
            Some(b"http/1.1") | None => IdentityManagementHttpProtocol::Http1,
            Some(_) => return Err(failure(Code::IdentityManagementTransportFailed)),
        };
        Ok(IdentityManagementConnection { protocol, stream })
    }
}
fn failure(code: Code) -> ExchangeFailure {
    ExchangeFailure::new(code)
}

#[cfg(test)]
#[path = "identity_management_connector_tests.rs"]
mod tests;
