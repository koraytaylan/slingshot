//! Concrete direct IMS client: frozen platform trust, one socket, one exchange.

use super::{
    async_identity_management_exchange::{
        AsyncIdentityManagementTransport, IdentityManagementReceipt,
    },
    identity_management_connector::{
        IdentityManagementConnection, IdentityManagementConnector, IdentityManagementHttpProtocol,
    },
    identity_management_exchange::{ExchangeFailure, MonotonicClock},
};
use crate::transport_policy::IdentityManagementTrustInput;
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};
use std::{future::Future, pin::Pin};
use tokio::time::{Duration, Instant, timeout_at};

/// The concrete immutable service-credential transport. Its constructor accepts
/// no endpoint, proxy, author trust extension, redirect or reload configuration.
pub struct IdentityManagementClient {
    connector: IdentityManagementConnector,
    #[cfg(any(test, feature = "test-support"))]
    test_socket_address: Option<std::net::SocketAddr>,
}
impl core::fmt::Debug for IdentityManagementClient {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("IdentityManagementClient([redacted])")
    }
}
impl IdentityManagementClient {
    /// Freezes the already verified platform-only IMS trust policy.
    pub fn new(trust: &IdentityManagementTrustInput) -> Result<Self, ExchangeFailure> {
        Ok(Self {
            connector: IdentityManagementConnector::new(trust)?,
            #[cfg(any(test, feature = "test-support"))]
            test_socket_address: None,
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn set_test_socket(&mut self, address: std::net::SocketAddr) {
        self.test_socket_address = Some(address);
    }
}
impl AsyncIdentityManagementTransport for IdentityManagementClient {
    fn exchange<'a>(
        &'a self,
        body: &'a [u8],
        clock: &'a (dyn MonotonicClock + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<IdentityManagementReceipt, ExchangeFailure>> + Send + 'a>>
    {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(address) = self.test_socket_address {
            return Box::pin(exchange_connecting(
                async move {
                    let socket = crate::connection_phase::within(
                        Duration::from_millis(
                            ProfileAuthenticationContract::embedded()
                                .limits
                                .identity_management_connect_timeout_milliseconds,
                        ),
                        async {
                            tokio::net::TcpStream::connect(address).await.map_err(|_| {
                                ExchangeFailure::new(
                                    ConfigurationFailureCode::IdentityManagementTransportFailed,
                                )
                            })
                        },
                        ExchangeFailure::new(
                            ConfigurationFailureCode::IdentityManagementConnectTimeout,
                        ),
                    )
                    .await?;
                    self.connector.connect_test_socket(socket).await
                },
                body,
                clock,
            ));
        }
        Box::pin(exchange_connecting(self.connector.connect(), body, clock))
    }
}

// Private composition seam: production always supplies the fixed-endpoint
// connector. Tests replace only TCP dialing, retaining TLS identity and codecs.
async fn exchange_connecting(
    connection: impl Future<Output = Result<IdentityManagementConnection, ExchangeFailure>>,
    body: &[u8],
    clock: &(dyn MonotonicClock + Sync),
) -> Result<IdentityManagementReceipt, ExchangeFailure> {
    if body.len() as u64
        > ProfileAuthenticationContract::embedded()
            .limits
            .maximum_identity_management_request_body_bytes
    {
        return Err(ExchangeFailure::new(
            ConfigurationFailureCode::IdentityManagementResponseHeadLimitExceeded,
        ));
    }
    with_overall_deadline(async {
        let (protocol, stream) = connection.await?.into_parts();
        match protocol {
            IdentityManagementHttpProtocol::Http1 => {
                super::identity_management_http1::exchange_http1(stream, body, clock).await
            }
            IdentityManagementHttpProtocol::Http2 => {
                super::identity_management_http2::exchange_http2(stream, body, clock).await
            }
        }
    })
    .await
}

async fn with_overall_deadline<T>(
    exchange: impl Future<Output = Result<T, ExchangeFailure>>,
) -> Result<T, ExchangeFailure> {
    let end = Instant::now()
        + Duration::from_millis(
            ProfileAuthenticationContract::embedded()
                .limits
                .identity_management_overall_timeout_milliseconds,
        );
    let expired =
        || ExchangeFailure::new(ConfigurationFailureCode::IdentityManagementOverallTimeout);
    let result = timeout_at(end, exchange).await.map_err(|_| expired())?;
    // A ready phase result at the exact overall boundary cannot evade expiry.
    if Instant::now() >= end {
        return Err(expired());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Dropped<'a>(&'a AtomicUsize);
    impl Drop for Dropped<'_> {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn overall_expiry_drops_pending_exchange_and_rejects_equality() {
        for ready_at_boundary in [false, true] {
            let drops = AtomicUsize::new(0);
            let exchange = async {
                let _guard = Dropped(&drops);
                if ready_at_boundary {
                    tokio::time::sleep(Duration::from_millis(
                        ProfileAuthenticationContract::embedded()
                            .limits
                            .identity_management_overall_timeout_milliseconds,
                    ))
                    .await;
                } else {
                    std::future::pending::<()>().await;
                }
                Ok(())
            };
            assert_eq!(
                with_overall_deadline(exchange).await.unwrap_err().code,
                ConfigurationFailureCode::IdentityManagementOverallTimeout
            );
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
    }
    #[tokio::test]
    async fn an_earlier_phase_failure_keeps_its_stable_code() {
        let failure = ExchangeFailure::new(ConfigurationFailureCode::IdentityManagementTlsFailed);
        assert_eq!(
            with_overall_deadline(async { Err::<(), _>(failure) }).await.unwrap_err(),
            failure
        );
    }
}

#[cfg(test)]
#[path = "identity_management_client_wire_tests.rs"]
mod wire_tests;
