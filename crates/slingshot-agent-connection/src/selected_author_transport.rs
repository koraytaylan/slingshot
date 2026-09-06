//! Direct, selected-author connection establishment.
//!
//! This is intentionally a small transport foundation rather than a general
//! HTTP client. It has no proxy configuration, no caller-supplied URI, and no
//! system-root lookup: the only socket it can open is the one immutable author
//! connection selected at startup. Route codecs build HTTP exchanges on the
//! returned stream and must still apply their own request and response bounds.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use rustls::{ClientConfig, RootCertStore};
use rustls_pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::time::Duration;
use tokio_rustls::{TlsConnector, client::TlsStream};

use crate::authentication::environment_provider::SelectedAuthorConnection;
use crate::authentication::environment_provider::SelectedAuthorConnectionRefusal;
use crate::author_hypertext_transfer_protocol_policy::ExchangeDeadlines;
use slingshot_domain::operation_executor::ExecutionIdentity;

/// A stream connected directly to the one selected author.
pub enum SelectedAuthorStream {
    /// A startup-warned cleartext author connection.
    Cleartext(TcpStream),
    /// A connection authenticated with the selected author's frozen roots.
    Protected(TlsStream<TcpStream>),
}

impl ::core::fmt::Debug for SelectedAuthorStream {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str("SelectedAuthorStream([redacted])")
    }
}

impl AsyncRead for SelectedAuthorStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.as_mut().get_mut() {
            Self::Cleartext(stream) => Pin::new(stream).poll_read(context, buffer),
            Self::Protected(stream) => Pin::new(stream).poll_read(context, buffer),
        }
    }
}

impl AsyncWrite for SelectedAuthorStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.as_mut().get_mut() {
            Self::Cleartext(stream) => Pin::new(stream).poll_write(context, bytes),
            Self::Protected(stream) => Pin::new(stream).poll_write(context, bytes),
        }
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.as_mut().get_mut() {
            Self::Cleartext(stream) => Pin::new(stream).poll_flush(context),
            Self::Protected(stream) => Pin::new(stream).poll_flush(context),
        }
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.as_mut().get_mut() {
            Self::Cleartext(stream) => Pin::new(stream).poll_shutdown(context),
            Self::Protected(stream) => Pin::new(stream).poll_shutdown(context),
        }
    }
}

/// Why a selected-author connection could not be established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SelectedAuthorTransportFailure {
    /// The immutable author trust input held no usable certificate.
    #[error("the selected author trust policy has no usable certificate")]
    TrustUnavailable,
    /// The selected address cannot become a TLS server name.
    #[error("the selected author host cannot become a TLS server name")]
    ServerNameInvalid,
    /// Name resolution and TCP connection did not complete in their shared phase.
    #[error("the selected author did not connect before its deadline")]
    ConnectDeadlineExceeded,
    /// The direct TCP attempt failed before a connection existed.
    #[error("the selected author TCP connection failed")]
    ConnectFailed,
    /// TLS negotiation did not complete in its independent phase.
    #[error("the selected author TLS handshake did not complete before its deadline")]
    TransportLayerSecurityDeadlineExceeded,
    /// The selected author did not authenticate under its frozen trust policy.
    #[error("the selected author TLS handshake failed")]
    TransportLayerSecurityFailed,
    /// The authenticated peer did not negotiate the requested application protocol.
    #[error("the selected author did not negotiate the required HTTP protocol")]
    ApplicationProtocolUnavailable,
}

/// The only application protocols accepted from a selected author.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectedHttpProtocol {
    /// HTTP/1.1, including a protected peer that does not advertise ALPN.
    Http1,
    /// HTTP/2 selected by TLS ALPN.
    Http2,
}

/// One negotiated connection and the codec required for that same socket.
/// Keeping the stream prevents negotiation from becoming a probe followed by
/// a second connection whose protocol could differ.
pub struct NegotiatedAuthorStream {
    /// The protocol accepted on this connection.
    protocol: SelectedHttpProtocol,
    /// The authenticated selected-author connection; no HTTP bytes were sent.
    stream: SelectedAuthorStream,
}

impl NegotiatedAuthorStream {
    /// Consumes this negotiation result for dispatch on its original socket.
    pub fn into_parts(self) -> (SelectedHttpProtocol, SelectedAuthorStream) {
        (self.protocol, self.stream)
    }
}

impl core::fmt::Debug for NegotiatedAuthorStream {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("NegotiatedAuthorStream([redacted])")
    }
}

/// The product's direct connector for one immutable selected author.
pub struct SelectedAuthorTransport {
    /// The only origin this connector can dial.
    connection: SelectedAuthorConnection,
    /// Frozen HTTP/1.1 TLS configuration, absent only for selected permitted
    /// cleartext transport.
    transport_layer_security: Option<Arc<ClientConfig>>,
    /// Same frozen trust/version/provider policy with only h2 ALPN permitted.
    transport_layer_security_http2: Option<Arc<ClientConfig>>,
    /// The same frozen policy offering only h2 and HTTP/1.1.
    transport_layer_security_negotiated: Option<Arc<ClientConfig>>,
}

impl ::core::fmt::Debug for SelectedAuthorTransport {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        formatter.write_str("SelectedAuthorTransport([redacted])")
    }
}

impl SelectedAuthorTransport {
    /// Rejects another provider before its token source can be invoked.
    pub(crate) fn require_provider<Cache>(
        &self,
        provider: &crate::authentication::environment_provider::EnvironmentAuthenticationProvider<
            Cache,
        >,
    ) -> Result<(), SelectedAuthorConnectionRefusal> {
        if provider.snapshot().target() != self.connection.target() {
            return Err(SelectedAuthorConnectionRefusal::AnotherTarget);
        }
        if provider.snapshot().revision() != self.connection.revision() {
            return Err(SelectedAuthorConnectionRefusal::AnotherRevision);
        }
        Ok(())
    }

    pub(crate) fn require_authentication(
        &self,
        authentication: &crate::authentication::environment_provider::RequestAuthentication,
    ) -> Result<(), SelectedAuthorConnectionRefusal> {
        authentication.require_connection(&self.connection)
    }
    /// Builds a direct connector from one selected author connection.
    ///
    /// This consumes no environment variables and loads no trust material: it
    /// snapshots the already verified author-only roots into the TLS client
    /// configuration exactly once.
    ///
    /// # Errors
    ///
    /// Returns [`SelectedAuthorTransportFailure::TrustUnavailable`] when the
    /// selected protected author does not carry parseable trust roots.
    pub fn new(
        connection: SelectedAuthorConnection,
    ) -> Result<Self, SelectedAuthorTransportFailure> {
        let (
            transport_layer_security,
            transport_layer_security_http2,
            transport_layer_security_negotiated,
        ) = if connection.requires_transport_layer_security() {
            let mut http1 = client_configuration(&connection)?;
            http1.alpn_protocols = vec![b"http/1.1".to_vec()];
            let mut http2 = http1.clone();
            http2.alpn_protocols = vec![b"h2".to_vec()];
            let mut negotiated = http1.clone();
            negotiated.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
            (Some(Arc::new(http1)), Some(Arc::new(http2)), Some(Arc::new(negotiated)))
        } else {
            (None, None, None)
        };
        Ok(Self {
            connection,
            transport_layer_security,
            transport_layer_security_http2,
            transport_layer_security_negotiated,
        })
    }

    /// Returns an endpoint below the one selected author origin.
    #[must_use]
    pub fn endpoint(&self, segments: &[&str]) -> String {
        self.connection.endpoint(segments)
    }

    /// The selected normalized origin without a context path or trailing slash.
    pub fn origin(&self) -> String {
        let author = self.connection.author();
        let scheme = if author.is_protected() { "https" } else { "http" };
        match author.port() {
            Some(port) => format!("{scheme}://{}:{port}", author.host()),
            None => format!("{scheme}://{}", author.host()),
        }
    }

    /// Requires an operation to be bound to this connector's selected target
    /// and selected-environment revision.
    pub fn require_execution(
        &self,
        identity: &ExecutionIdentity,
    ) -> Result<(), SelectedAuthorConnectionRefusal> {
        self.connection.require_execution(identity)
    }

    /// Opens exactly one direct socket to the selected author under the
    /// contract's separate connect and TLS deadlines.
    ///
    /// DNS resolution is deliberately part of the connect phase: `TcpStream`
    /// resolves the selected host itself, without a proxy resolver or an
    /// alternate endpoint supplied by a caller.
    pub async fn connect(&self) -> Result<SelectedAuthorStream, SelectedAuthorTransportFailure> {
        self.connect_selected(Some(false)).await
    }

    /// Connects the future HTTP/2 driver to the same immutable author. Protected
    /// peers must negotiate exactly h2; no silent downgrade or upgrade request
    /// is attempted. A selected permitted cleartext author uses HTTP/2 prior
    /// knowledge, whose preface/settings exchange the driver must validate
    /// before sending a request. This method sends no HTTP application bytes.
    pub async fn connect_http2(
        &self,
    ) -> Result<SelectedAuthorStream, SelectedAuthorTransportFailure> {
        self.connect_selected(Some(true)).await
    }

    /// Negotiates only h2 or HTTP/1.1 on one selected TLS connection. A peer
    /// without ALPN, or permitted cleartext, uses HTTP/1.1 without an upgrade
    /// probe. No retry, alternate endpoint or protocol fallback is performed.
    pub async fn connect_negotiated(
        &self,
    ) -> Result<NegotiatedAuthorStream, SelectedAuthorTransportFailure> {
        let stream = self.connect_selected(None).await?;
        let protocol = match &stream {
            SelectedAuthorStream::Protected(stream)
                if stream.get_ref().1.alpn_protocol() == Some(b"h2".as_slice()) =>
            {
                SelectedHttpProtocol::Http2
            }
            _ => SelectedHttpProtocol::Http1,
        };
        Ok(NegotiatedAuthorStream { protocol, stream })
    }

    async fn connect_selected(
        &self,
        http2: Option<bool>,
    ) -> Result<SelectedAuthorStream, SelectedAuthorTransportFailure> {
        let deadlines = ExchangeDeadlines::embedded();
        let author = self.connection.author();
        let port = author.port().unwrap_or(if author.is_protected() { 443 } else { 80 });
        // URI serialization brackets IPv6 literals; socket resolution and TLS
        // IP-address verification require the literal without those brackets.
        let host = author
            .host()
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(author.host());
        let stream = crate::connection_phase::within(
            Duration::from_millis(deadlines.connect_milliseconds),
            async {
                TcpStream::connect((host, port))
                    .await
                    .map_err(|_| SelectedAuthorTransportFailure::ConnectFailed)
            },
            SelectedAuthorTransportFailure::ConnectDeadlineExceeded,
        )
        .await?;
        let configuration = match http2 {
            Some(true) => &self.transport_layer_security_http2,
            Some(false) => &self.transport_layer_security,
            None => &self.transport_layer_security_negotiated,
        };
        let Some(configuration) = configuration else {
            return Ok(SelectedAuthorStream::Cleartext(stream));
        };
        let server_name = ServerName::try_from(host.to_owned())
            .map_err(|_| SelectedAuthorTransportFailure::ServerNameInvalid)?;
        // Only application-protocol selection differs. Root bytes, hostname
        // validation, provider, TLS versions and deadlines remain identical.
        let stream = crate::connection_phase::within(
            Duration::from_millis(deadlines.transport_layer_security_milliseconds),
            async {
                TlsConnector::from(configuration.clone())
                    .connect(server_name, stream)
                    .await
                    .map_err(|_| SelectedAuthorTransportFailure::TransportLayerSecurityFailed)
            },
            SelectedAuthorTransportFailure::TransportLayerSecurityDeadlineExceeded,
        )
        .await?;
        let negotiated = stream.get_ref().1.alpn_protocol();
        let acceptable = match http2 {
            Some(true) => negotiated == Some(b"h2".as_slice()),
            Some(false) => matches!(negotiated, None | Some(b"http/1.1")),
            None => matches!(negotiated, None | Some(b"h2") | Some(b"http/1.1")),
        };
        if !acceptable {
            return Err(SelectedAuthorTransportFailure::ApplicationProtocolUnavailable);
        }
        Ok(SelectedAuthorStream::Protected(stream))
    }
}

/// Creates a TLS configuration solely from the selected author roots.
fn client_configuration(
    connection: &SelectedAuthorConnection,
) -> Result<ClientConfig, SelectedAuthorTransportFailure> {
    let mut roots = RootCertStore::empty();
    let mut accepted = false;
    for source in connection.trust().roots() {
        // `AuthorTrustInput` is built from the verified platform/additional
        // authority types, both of which retain certificate DER rather than
        // their PEM source documents. Treating these bytes as PEM would make a
        // valid selected trust policy unusable at runtime.
        roots
            .add(CertificateDer::from(source.clone()))
            .map_err(|_| SelectedAuthorTransportFailure::TrustUnavailable)?;
        accepted = true;
    }
    if !accepted {
        return Err(SelectedAuthorTransportFailure::TrustUnavailable);
    }
    // Do not inherit a process-global provider or its protocol defaults.
    let configuration =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
            .map_err(|_| SelectedAuthorTransportFailure::TrustUnavailable)?
            .with_root_certificates(roots)
            .with_no_client_auth();
    Ok(configuration)
}
