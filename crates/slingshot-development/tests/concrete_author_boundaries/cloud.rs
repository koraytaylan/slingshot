//! Real owned Cloud exchange and selected-author TLS; only IMS TCP dialing is local.
use super::*;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use slingshot_agent_connection::authentication::{
    cloud_service_credentials::CloudServiceCredentials, token_assertion::ServiceCredentialAssertion,
};
use slingshot_domain::{
    profile_authentication_contract::ConfigurationFailureCode,
    secret_value::SensitiveConfigurationDocument,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

const ROOT: &[u8] = include_bytes!(
    "../../../slingshot-agent-connection/tests/fixtures/selected-author-tls/ims-test-root.pem"
);
const SECOND: u64 = 1788111373;
struct Roots;
impl PlatformTrustSource for Roots {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        let mut records = Store.records()?;
        records.push(ProviderRecord {
            der: CertificateDer::from_pem_slice(ROOT).unwrap().as_ref().to_vec(),
            decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
        });
        Ok(records)
    }
}
struct Clock(AtomicU64);
impl MonotonicClock for Clock {
    fn reading_milliseconds(&self) -> u64 {
        self.0.fetch_add(100, Ordering::SeqCst)
    }
}
struct Utc;
impl CoordinatedUniversalTimeClock for Utc {
    fn sample(&self) -> Option<u64> {
        Some(SECOND)
    }
}
fn credentials() -> CloudServiceCredentials {
    CloudServiceCredentials::parse(&SensitiveConfigurationDocument::from_bytes(
        include_bytes!("../../../slingshot-test-support/fixtures/cloud-credentials/valid.json")
            .to_vec(),
    ))
    .unwrap()
}
fn snapshot(author_address: &str, publisher: &str, trust_ims: bool) -> SelectedEnvironmentSnapshot {
    snapshot_with_credentials(
        author_address,
        publisher,
        trust_ims,
        include_bytes!("../../../slingshot-test-support/fixtures/cloud-credentials/valid.json"),
    )
}

fn snapshot_with_credentials(
    author_address: &str,
    publisher: &str,
    trust_ims: bool,
    credential_bytes: &[u8],
) -> SelectedEnvironmentSnapshot {
    let profile = include_str!(
        "../../../slingshot-test-support/fixtures/profile-directories/ordered/profiles/zulu.toml"
    )
    .replace("https://author.example.com", author_address)
    .replace("https://publish.example.com", publisher)
    .replace(
        "[environments.production]\n",
        "[environments.production]\nadditional_ca_certificate_file = \"certificates/author.pem\"\n",
    );
    let mut inventory = "format_version = 1\n".to_owned();
    for (reference, bytes) in [
        ("certificates/author.pem", ROOT),
        ("credentials/alpha.json", credential_bytes),
        ("profiles/zulu.toml", profile.as_bytes()),
    ] {
        inventory.push_str(&format!(
            "[[sources]]\nreference = \"{reference}\"\nsha256 = \"{}\"\n",
            hex::encode(Sha256::digest(bytes))
        ));
    }
    let loaded = load_profiles(
        ScriptedFilesystem::new()
            .with_directory("profiles")
            .with_directory("credentials")
            .with_directory("certificates")
            .with_source("certificates/author.pem", ROOT)
            .with_source("credentials/alpha.json", credential_bytes)
            .with_source("profiles/zulu.toml", profile.as_bytes())
            .with_source("configuration-snapshot.toml", inventory.as_bytes()),
    )
    .unwrap();
    let selection = resolve(
        &loaded,
        &RequestedSelection {
            profile: Some(ProfileName::parse("alpha-site").unwrap()),
            environment: Some(EnvironmentName::parse("production").unwrap()),
        },
    )
    .unwrap();
    let chosen = selection.environment_of(&loaded);
    let platform = if trust_ims {
        PlatformTrustSnapshot::take(&Roots)
    } else {
        PlatformTrustSnapshot::take(&Store)
    }
    .unwrap();
    let ims = IdentityManagementTrustInput::from_platform(&platform).unwrap();
    let extension = AdditionalAuthorCertificates::parse(ROOT).unwrap();
    let trust = AuthorTrustInput::from_platform_and_extension(&platform, Some(&extension)).unwrap();
    let credentials = CloudServiceCredentials::parse(&SensitiveConfigurationDocument::from_bytes(
        credential_bytes.to_vec(),
    ))
    .unwrap();
    let principal = credentials.principal();
    let metascopes = CanonicalMetascopeSet::from_values(
        &credentials
            .metascopes()
            .values()
            .unwrap()
            .iter()
            .map(|s| (*s).to_owned())
            .collect::<Vec<_>>(),
    );
    let target = AuthorTargetIdentityDigest::build(
        chosen.deployment().as_text(),
        chosen.author_connection_target().as_text(),
        principal,
    )
    .unwrap();
    let revision = SelectedEnvironmentRevision::build(&RevisionFields {
        profile_name: selection.profile_name().as_text().into(),
        environment_name: selection.environment_name().as_text().into(),
        profile_source_reference: selection.profile_source().as_text().into(),
        selection_source_reference: None,
        author_target_identity: target,
        publisher_base_address: publisher.into(),
        authentication_method: chosen.authentication().method().into(),
        credential_source_reference: Some("credentials/alpha.json".into()),
        certificate_source_reference: Some("certificates/author.pem".into()),
        proxy_policy: "direct_without_ambient_discovery".into(),
        allow_insecure_author_transport: false,
        canonical_metascope_set: metascopes,
        identity_management_trust_policy_identity: ims.identity(),
        author_trust_policy_identity: trust.identity(),
    })
    .unwrap();
    SelectedEnvironmentSnapshot::assemble(
        &selection,
        SnapshotMaterial {
            author: chosen.author_connection_target().clone(),
            publisher: chosen.publisher_metadata().clone(),
            deployment: chosen.deployment(),
            authentication: SnapshotAuthentication::ServiceCredentials {
                credentials: Box::new(credentials),
            },
            principal,
            target,
            revision,
            identity_management_trust: ims,
            author_trust: trust,
        },
    )
}
fn server(ims: bool) -> tokio_rustls::TlsAcceptor {
    let leaf = if ims {
        include_bytes!("../../../slingshot-agent-connection/tests/fixtures/selected-author-tls/ims-test-leaf.pem").as_slice()
    } else {
        include_bytes!("../../../slingshot-agent-connection/tests/fixtures/selected-author-tls/ims-ca-author-leaf.pem").as_slice()
    };
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_protocol_versions(&[&rustls::version::TLS13]).unwrap().with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from_pem_slice(leaf).unwrap()], PrivateKeyDer::from_pem_slice(
            include_bytes!("../../../slingshot-agent-connection/tests/fixtures/selected-author-tls/test-only-private-key.pem")).unwrap()).unwrap();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    tokio_rustls::TlsAcceptor::from(Arc::new(config))
}
async fn head(peer: &mut (impl tokio::io::AsyncRead + Unpin)) -> String {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        bytes.push(peer.read_u8().await.unwrap());
        assert!(bytes.len() < 8192);
    }
    String::from_utf8(bytes).unwrap()
}
async fn reply(
    peer: &mut (impl tokio::io::AsyncWrite + Unpin),
    status: u16,
    body: &str,
    extra: &str,
    truncated: bool,
) {
    peer.write_all(format!("HTTP/1.1 {status} Reply\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra}\r\n", body.len()).as_bytes()).await.unwrap();
    peer.write_all(&body.as_bytes()[..body.len() - usize::from(truncated)]).await.unwrap();
    peer.shutdown().await.unwrap();
}

#[tokio::test]
async fn ambient_cloud_proxy_child() {
    let Ok(author) = std::env::var("SLINGSHOT_CLOUD_BOUNDARY_AUTHOR") else {
        return;
    };
    let publisher = std::env::var("SLINGSHOT_CLOUD_BOUNDARY_PUBLISHER").unwrap();
    let ims = std::env::var("SLINGSHOT_CLOUD_BOUNDARY_IMS").unwrap().parse().unwrap();
    let provider = AsyncEnvironmentAuthenticationProvider::new_async_test_socket(
        snapshot(&author, &publisher, true),
        ims,
    )
    .unwrap();
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let identity = ExecutionIdentity {
        attempt: 1,
        operation_identifier: "cloud-boundary".into(),
        author_target_identity_digest: provider.snapshot().target().to_string(),
        selected_environment_revision: provider.snapshot().revision().to_string(),
    };
    let clock = Clock(AtomicU64::new(0));
    assert!(provider.authenticate(&publisher, &clock, &Utc).await.is_err());
    assert_eq!(clock.0.load(Ordering::SeqCst), 0);
    let result = transport
        .discover_capabilities_authenticated_async(
            &identity,
            "query_paths",
            Some(7),
            &provider,
            &clock,
            &Utc,
        )
        .await;
    println!("cloud-boundary-accepted:{}", result.is_ok());
    println!("{provider:?} {result:?}");
}

#[tokio::test]
async fn owned_cloud_discovery_refresh_and_refusals_use_only_the_selected_tls_peers() {
    use slingshot_agent_protocol::identity::WireContractIdentity;
    use slingshot_domain::{
        author_agent_transport_contract::AuthorAgentTransportContract,
        command::schema::canonical_contract_digest,
        selected_command_contract_identity::SelectedCommandContractIdentity,
    };
    for (ambient_proxies, status, truncated) in [false, true].into_iter().flat_map(|ambient| {
        [(200, false), (401, false), (302, false), (200, true), (403, false), (401, true)]
            .map(|(status, truncated)| (ambient, status, truncated))
    }) {
        let author = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ims = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut traps = Vec::new();
        for name in ["publisher", "proxy", "redirect", "hostile IMS"] {
            traps.push((name, TcpListener::bind("127.0.0.1:0").await.unwrap()));
        }
        let publisher = format!("https://{}", traps[0].1.local_addr().unwrap());
        let provider = AsyncEnvironmentAuthenticationProvider::new_async_test_socket(
            snapshot(
                &format!("https://{}/context", author.local_addr().unwrap()),
                &publisher,
                true,
            ),
            ims.local_addr().unwrap(),
        )
        .unwrap();
        let transport =
            SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
        let identity = ExecutionIdentity {
            attempt: 1,
            operation_identifier: "cloud-boundary".into(),
            author_target_identity_digest: provider.snapshot().target().to_string(),
            selected_environment_revision: provider.snapshot().revision().to_string(),
        };
        let clock = Clock(AtomicU64::new(0));
        assert!(provider.authenticate(&publisher, &clock, &Utc).await.is_err());
        assert_eq!(clock.0.load(Ordering::SeqCst), 0);
        let exchanges = if status == 401 && !truncated { 2 } else { 1 };
        let ims_peer = async {
            for generation in 1..=exchanges {
                let mut peer = server(true).accept(ims.accept().await.unwrap().0).await.unwrap();
                assert_eq!(peer.get_ref().1.server_name(), Some("ims-na1.adobelogin.com"));
                let request = head(&mut peer).await;
                assert!(request.starts_with("POST /ims/exchange/jwt HTTP/1.1\r\n"));
                assert!(request.to_ascii_lowercase().contains("host: ims-na1.adobelogin.com\r\n"));
                let length: usize = request
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .map(str::to_owned)
                    })
                    .unwrap()
                    .parse()
                    .unwrap();
                assert!(length < 8192);
                let mut body = vec![0; length];
                peer.read_exact(&mut body).await.unwrap();
                // Verify ordered fields and the complete signed assertion.
                let credentials = credentials();
                let fields: Vec<_> = url::form_urlencoded::parse(&body).collect();
                assert_eq!(
                    fields.iter().map(|p| p.0.as_ref()).collect::<Vec<_>>(),
                    ["client_id", "client_secret", "jwt_token"]
                );
                assert_eq!(
                    fields[0].1.as_bytes(),
                    credentials.technical_account_client_identifier().as_bytes()
                );
                assert_eq!(
                    fields[1].1.as_bytes(),
                    credentials.client_secret().expose_secret_bytes()
                );
                ServiceCredentialAssertion::build(&credentials, &Utc)
                    .unwrap()
                    .lend_compact_bytes(|expected| assert_eq!(fields[2].1.as_bytes(), expected));
                reply(&mut peer, 200, &format!("{{\"access_token\":\"outer-cloud-{generation}\",\"token_type\":\"bearer\",\"expires_in\":3600000}}"), "", false).await;
            }
        };
        let author_peer = async {
            for generation in 1..=exchanges {
                let mut peer =
                    server(false).accept(author.accept().await.unwrap().0).await.unwrap();
                let request = head(&mut peer).await;
                assert!(
                    request
                        .starts_with("GET /context/bin/slingshot-agent/capabilities HTTP/1.1\r\n")
                );
                assert!(
                    request
                        .contains(&format!("Authorization: Bearer outer-cloud-{generation}\r\n"))
                );
                let body = serde_json::json!({"format":"slingshot.agent/1","agent_event_store_generation":7,
                    "canonical_json_contract_digest":canonical_contract_digest(),"transport_contract_digest":AuthorAgentTransportContract::embedded_digest(),
                    "command_contracts":[WireContractIdentity::from(&SelectedCommandContractIdentity::installed("query_paths").unwrap())],"continuation_authority_ready":true}).to_string();
                let redirect = if status == 302 {
                    format!("Location: https://{}/stolen\r\n", traps[2].1.local_addr().unwrap())
                } else {
                    String::new()
                };
                reply(
                    &mut peer,
                    if generation == 2 { 200 } else { status },
                    &body,
                    &redirect,
                    truncated,
                )
                .await;
            }
        };
        let request = async {
            if ambient_proxies {
                let proxy = format!("http://{}", traps[1].1.local_addr().unwrap());
                let mut child = tokio::process::Command::new(std::env::current_exe().unwrap());
                child
                    .args(["--exact", "cloud::ambient_cloud_proxy_child", "--nocapture"])
                    .env(
                        "SLINGSHOT_CLOUD_BOUNDARY_AUTHOR",
                        format!("https://{}/context", author.local_addr().unwrap()),
                    )
                    .env("SLINGSHOT_CLOUD_BOUNDARY_PUBLISHER", &publisher)
                    .env("SLINGSHOT_CLOUD_BOUNDARY_IMS", ims.local_addr().unwrap().to_string())
                    .env("NO_PROXY", "")
                    .env("no_proxy", "")
                    .kill_on_drop(true);
                for name in [
                    "HTTP_PROXY",
                    "HTTPS_PROXY",
                    "ALL_PROXY",
                    "http_proxy",
                    "https_proxy",
                    "all_proxy",
                ] {
                    child.env(name, &proxy);
                }
                let output = child.output().await.unwrap();
                assert!(output.status.success(), "proxy-isolated Cloud child failed");
                let transcript = format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                let accepted = transcript.matches("cloud-boundary-accepted:true").count();
                let refused = transcript.matches("cloud-boundary-accepted:false").count();
                assert_eq!(accepted + refused, 1, "child omitted or repeated outcome");
                (accepted == 1, transcript)
            } else {
                let result = transport
                    .discover_capabilities_authenticated_async(
                        &identity,
                        "query_paths",
                        Some(7),
                        &provider,
                        &clock,
                        &Utc,
                    )
                    .await;
                (result.is_ok(), format!("{provider:?} {result:?}"))
            }
        };
        let ((accepted, transcript), (), ()) = timeout(Duration::from_secs(10), async {
            tokio::join!(request, ims_peer, author_peer)
        })
        .await
        .unwrap();
        assert_eq!(accepted, matches!(status, 200 | 401) && !truncated);
        let credentials = credentials();
        let mut scanner =
            slingshot_development::profile_authentication_harness::SecretScanner::looking_for(&[
                "outer-cloud-1",
                "outer-cloud-2",
                credentials.technical_account_client_identifier(),
                credentials.technical_account_identifier(),
                credentials.organization_identifier(),
                "p8e-not-a-real-client-secret",
                "-----BEGIN PRIVATE KEY-----",
            ]);
        scanner.observe(transcript);
        scanner.require_clean().unwrap();
        for (name, trap) in traps.into_iter().chain([("author", author), ("IMS", ims)]) {
            assert!(
                timeout(Duration::from_millis(10), trap.accept()).await.is_err(),
                "unexpected connection to {name}"
            );
        }
    }
}

#[tokio::test]
async fn author_extension_accepts_author_tls_but_cannot_authorize_ims() {
    let author = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ims = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let snapshot = snapshot(
        &format!("https://{}", author.local_addr().unwrap()),
        "https://publisher.example.com",
        false,
    );
    let transport = SelectedAuthorTransport::new(snapshot.author_connection()).unwrap();
    let provider = AsyncEnvironmentAuthenticationProvider::new_async_test_socket(
        snapshot,
        ims.local_addr().unwrap(),
    )
    .unwrap();
    // Same CA, valid endpoint names on both leaves: only the author trusts it.
    timeout(Duration::from_secs(10), async {
        let (connection, peer) = tokio::join!(transport.connect(), async {
            server(false).accept(author.accept().await.unwrap().0).await
        });
        assert!(connection.is_ok());
        assert!(peer.is_ok());
    })
    .await
    .unwrap();
    let clock = Clock(AtomicU64::new(0));
    let endpoint = provider.author_endpoint(&["bin", "slingshot-agent", "capabilities"]);
    let (result, ()) = timeout(Duration::from_secs(10), async {
        tokio::join!(provider.authenticate(&endpoint, &clock, &Utc), async {
            if let Ok(mut peer) = server(true).accept(ims.accept().await.unwrap().0).await {
                let mut byte = [0];
                assert!(
                    !matches!(peer.read(&mut byte).await, Ok(1)),
                    "credentials sent to untrusted IMS"
                );
            }
        })
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().code, ConfigurationFailureCode::IdentityManagementTlsFailed);
    assert_eq!(clock.0.load(Ordering::SeqCst), 100);
    for listener in [author, ims] {
        assert!(timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    }
}

#[test]
fn conformance_override_rejects_non_loopback_before_construction() {
    for address in ["192.0.2.1:443", "[2001:db8::1]:443", "0.0.0.0:443", "[::]:443"] {
        let result = AsyncEnvironmentAuthenticationProvider::new_async_test_socket(
            snapshot("https://127.0.0.1", "https://publisher.example.com", true),
            address.parse().unwrap(),
        );
        assert_eq!(
            result.unwrap_err().code,
            ConfigurationFailureCode::AuthenticationTargetMismatch
        );
    }
}

#[tokio::test]
async fn rotated_secret_is_loaded_only_by_a_new_provider_and_never_shares_its_cache() {
    use slingshot_agent_protocol::identity::WireContractIdentity;
    use slingshot_domain::{
        author_agent_transport_contract::AuthorAgentTransportContract,
        command::schema::canonical_contract_digest,
        selected_command_contract_identity::SelectedCommandContractIdentity,
    };
    let author = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ims = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut traps = Vec::new();
    for name in ["publisher", "proxy", "redirect", "hostile IMS"] {
        traps.push((name, TcpListener::bind("127.0.0.1:0").await.unwrap()));
    }
    let endpoint = format!("https://{}/rotation", author.local_addr().unwrap());
    let publisher = format!("https://{}", traps[0].1.local_addr().unwrap());
    let mut source =
        include_bytes!("../../../slingshot-test-support/fixtures/cloud-credentials/valid.json")
            .to_vec();
    let old = AsyncEnvironmentAuthenticationProvider::new_async_test_socket(
        snapshot_with_credentials(&endpoint, &publisher, true, &source),
        ims.local_addr().unwrap(),
    )
    .unwrap();
    source = String::from_utf8(source)
        .unwrap()
        .replace("p8e-not-a-real-client-secret", "rotated-not-a-real-client-secret")
        .into_bytes();
    let new = AsyncEnvironmentAuthenticationProvider::new_async_test_socket(
        snapshot_with_credentials(&endpoint, &publisher, true, &source),
        ims.local_addr().unwrap(),
    )
    .unwrap();
    // Secret rotation is not a new principal or semantic revision. Cache
    // ownership must nevertheless remain separate between runtime instances.
    assert_eq!(old.snapshot().target(), new.snapshot().target());
    assert_eq!(old.snapshot().revision(), new.snapshot().revision());
    let transport = SelectedAuthorTransport::new(old.snapshot().author_connection()).unwrap();
    let identity = ExecutionIdentity {
        attempt: 1,
        operation_identifier: "rotation-proof".into(),
        author_target_identity_digest: old.snapshot().target().to_string(),
        selected_environment_revision: old.snapshot().revision().to_string(),
    };
    let clock = Clock(AtomicU64::new(0));
    let token_endpoint = old.author_endpoint(&["bin", "slingshot-agent", "capabilities"]);
    let requests = async {
        let mut transcript = String::new();
        for (index, provider) in [&old, &new, &old, &old, &new].into_iter().enumerate() {
            if index == 1 {
                let (_, lease) = old.authenticate(&token_endpoint, &clock, &Utc).await.unwrap();
                assert_eq!(
                    new.refresh_after_unauthorized(lease.unwrap(), &clock, &Utc)
                        .await
                        .unwrap_err()
                        .code,
                    ConfigurationFailureCode::AuthenticationTargetMismatch
                );
            }
            if index == 3 {
                let (_, lease) = old.authenticate(&token_endpoint, &clock, &Utc).await.unwrap();
                old.refresh_after_unauthorized(lease.unwrap(), &clock, &Utc).await.unwrap();
            }
            let result = transport
                .discover_capabilities_authenticated_async(
                    &identity,
                    "query_paths",
                    Some(7),
                    provider,
                    &clock,
                    &Utc,
                )
                .await;
            assert!(result.is_ok());
            transcript.push_str(&format!("{provider:?} {result:?}"));
        }
        transcript
    };
    let ims_peer = async {
        for (index, secret) in [
            "p8e-not-a-real-client-secret",
            "rotated-not-a-real-client-secret",
            "p8e-not-a-real-client-secret",
        ]
        .into_iter()
        .enumerate()
        {
            let mut peer = server(true).accept(ims.accept().await.unwrap().0).await.unwrap();
            assert_eq!(peer.get_ref().1.server_name(), Some("ims-na1.adobelogin.com"));
            let request = head(&mut peer).await;
            assert!(request.starts_with("POST /ims/exchange/jwt HTTP/1.1\r\n"));
            let length: usize = request
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase().strip_prefix("content-length: ").map(str::to_owned)
                })
                .unwrap()
                .parse()
                .unwrap();
            assert!(length < 8192);
            let mut form = vec![0; length];
            peer.read_exact(&mut form).await.unwrap();
            let fields: Vec<_> = url::form_urlencoded::parse(&form).collect();
            assert_eq!(
                fields.iter().map(|field| field.0.as_ref()).collect::<Vec<_>>(),
                ["client_id", "client_secret", "jwt_token"]
            );
            assert_eq!(fields[1].1, secret);
            let credentials = credentials();
            assert_eq!(fields[0].1, credentials.technical_account_client_identifier());
            ServiceCredentialAssertion::build(&credentials, &Utc)
                .unwrap()
                .lend_compact_bytes(|expected| assert_eq!(fields[2].1.as_bytes(), expected));
            reply(&mut peer, 200, &format!("{{\"access_token\":\"rotation-token-{index}\",\"token_type\":\"bearer\",\"expires_in\":3600000}}"), "", false).await;
        }
    };
    let author_peer = async {
        let body = serde_json::json!({"format":"slingshot.agent/1","agent_event_store_generation":7,
            "canonical_json_contract_digest":canonical_contract_digest(),"transport_contract_digest":AuthorAgentTransportContract::embedded_digest(),
            "command_contracts":[WireContractIdentity::from(&SelectedCommandContractIdentity::installed("query_paths").unwrap())],"continuation_authority_ready":true}).to_string();
        for token in [0, 1, 0, 2, 1] {
            let mut peer = server(false).accept(author.accept().await.unwrap().0).await.unwrap();
            let request = head(&mut peer).await;
            assert!(
                request.starts_with("GET /rotation/bin/slingshot-agent/capabilities HTTP/1.1\r\n")
            );
            assert!(request.contains(&format!("Authorization: Bearer rotation-token-{token}\r\n")));
            reply(&mut peer, 200, &body, "", false).await;
        }
    };
    let (transcript, (), ()) =
        timeout(Duration::from_secs(10), async { tokio::join!(requests, ims_peer, author_peer) })
            .await
            .unwrap();
    let mut scanner =
        slingshot_development::profile_authentication_harness::SecretScanner::looking_for(&[
            "p8e-not-a-real-client-secret",
            "rotated-not-a-real-client-secret",
            "rotation-token-0",
            "rotation-token-1",
            "rotation-token-2",
            "a1b2c3d4e5f6",
            "6E1B0F2A5C3D4E5F@techacct.adobe.com",
            "1A2B3C4D5E6F7A8B9C0D1E2F@AdobeOrg",
        ]);
    scanner.observe(transcript);
    scanner.require_clean().unwrap();
    for (name, listener) in traps.into_iter().chain([("author", author), ("IMS", ims)]) {
        assert!(
            timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
            "unexpected {name} connection"
        );
    }
}

#[tokio::test]
async fn pending_cloud_exchange_does_not_block_or_contaminate_basic_and_caches_stay_owned() {
    use slingshot_agent_protocol::identity::WireContractIdentity;
    use slingshot_domain::{
        author_agent_transport_contract::AuthorAgentTransportContract,
        command::schema::canonical_contract_digest,
        selected_command_contract_identity::SelectedCommandContractIdentity,
    };
    let basic_author = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cloud_author = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ims = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut traps = Vec::new();
    for name in ["Basic publisher", "Cloud publisher", "proxy", "redirect", "hostile IMS"] {
        traps.push((name, TcpListener::bind("127.0.0.1:0").await.unwrap()));
    }
    let basic_endpoint = format!("http://{}/basic", basic_author.local_addr().unwrap());
    let cloud_endpoint = format!("https://{}/cloud", cloud_author.local_addr().unwrap());
    let basic =
        super::provider(&basic_endpoint, &format!("http://{}", traps[0].1.local_addr().unwrap()));
    let cloud = AsyncEnvironmentAuthenticationProvider::new_async_test_socket(
        snapshot(&cloud_endpoint, &format!("https://{}", traps[1].1.local_addr().unwrap()), true),
        ims.local_addr().unwrap(),
    )
    .unwrap();
    let basic_transport =
        SelectedAuthorTransport::new(basic.snapshot().author_connection()).unwrap();
    let cloud_transport =
        SelectedAuthorTransport::new(cloud.snapshot().author_connection()).unwrap();
    let identity = |provider: &AsyncEnvironmentAuthenticationProvider| ExecutionIdentity {
        attempt: 1,
        operation_identifier: "concurrent-boundary".into(),
        author_target_identity_digest: provider.snapshot().target().to_string(),
        selected_environment_revision: provider.snapshot().revision().to_string(),
    };
    let basic_identity = identity(&basic);
    let cloud_identity = identity(&cloud);
    let clock = Clock(AtomicU64::new(0));
    assert!(basic.authenticate(&cloud_endpoint, &NoTokenClocks, &NoTokenClocks).await.is_err());
    assert!(cloud.authenticate(&basic_endpoint, &clock, &Utc).await.is_err());
    assert!(
        basic_transport
            .discover_capabilities_authenticated_async(
                &basic_identity,
                "query_paths",
                Some(7),
                &cloud,
                &NoTokenClocks,
                &NoTokenClocks,
            )
            .await
            .is_err()
    );
    assert!(
        cloud_transport
            .discover_capabilities_authenticated_async(
                &cloud_identity,
                "query_paths",
                Some(7),
                &basic,
                &NoTokenClocks,
                &NoTokenClocks,
            )
            .await
            .is_err()
    );
    assert_eq!(clock.0.load(Ordering::SeqCst), 0);
    let body = serde_json::json!({"format":"slingshot.agent/1","agent_event_store_generation":7,
        "canonical_json_contract_digest":canonical_contract_digest(),"transport_contract_digest":AuthorAgentTransportContract::embedded_digest(),
        "command_contracts":[WireContractIdentity::from(&SelectedCommandContractIdentity::installed("query_paths").unwrap())],"continuation_authority_ready":true}).to_string();
    let (ims_started, ready) = tokio::sync::oneshot::channel();
    let (basic_completed, release_ims) = tokio::sync::oneshot::channel();
    let ims_peer = async {
        let mut peer = server(true).accept(ims.accept().await.unwrap().0).await.unwrap();
        let request = head(&mut peer).await;
        assert!(request.starts_with("POST /ims/exchange/jwt HTTP/1.1\r\n"));
        assert_eq!(peer.get_ref().1.server_name(), Some("ims-na1.adobelogin.com"));
        let length: usize = request
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase().strip_prefix("content-length: ").map(str::to_owned)
            })
            .unwrap()
            .parse()
            .unwrap();
        assert!(length < 8192);
        let mut form = vec![0; length];
        peer.read_exact(&mut form).await.unwrap();
        let fields: Vec<_> = url::form_urlencoded::parse(&form).collect();
        assert_eq!(
            fields.iter().map(|field| field.0.as_ref()).collect::<Vec<_>>(),
            ["client_id", "client_secret", "jwt_token"]
        );
        let credentials = credentials();
        assert_eq!(fields[0].1, credentials.technical_account_client_identifier());
        assert_eq!(fields[1].1.as_bytes(), credentials.client_secret().expose_secret_bytes());
        ServiceCredentialAssertion::build(&credentials, &Utc)
            .unwrap()
            .lend_compact_bytes(|expected| assert_eq!(fields[2].1.as_bytes(), expected));
        ims_started.send(()).unwrap();
        // The Cloud exchange cannot complete until a full Basic result has
        // arrived. A shared authentication lock would deadlock this handshake.
        release_ims.await.unwrap();
        reply(
            &mut peer,
            200,
            r#"{"access_token":"isolated-cloud-token","token_type":"bearer","expires_in":3600000}"#,
            "",
            false,
        )
        .await;
    };
    let basic_peer = async {
        for _ in 0..2 {
            let mut peer = basic_author.accept().await.unwrap().0;
            let request = head(&mut peer).await;
            assert!(
                request.starts_with("GET /basic/bin/slingshot-agent/capabilities HTTP/1.1\r\n")
            );
            assert!(
                request.contains("Authorization: Basic YWRtaW46bm90LWEtcmVhbC1wYXNzd29yZA==\r\n")
            );
            assert!(!request.contains("Bearer"));
            reply(&mut peer, 200, &body, "", false).await;
        }
    };
    let cloud_peer = async {
        for _ in 0..2 {
            let mut peer =
                server(false).accept(cloud_author.accept().await.unwrap().0).await.unwrap();
            let request = head(&mut peer).await;
            assert!(
                request.starts_with("GET /cloud/bin/slingshot-agent/capabilities HTTP/1.1\r\n")
            );
            assert!(request.contains("Authorization: Bearer isolated-cloud-token\r\n"));
            assert!(!request.contains("Basic"));
            reply(&mut peer, 200, &body, "", false).await;
        }
    };
    let basic_requests = async {
        ready.await.unwrap();
        let mut release = Some(basic_completed);
        let mut transcript = String::new();
        for _ in 0..2 {
            let result = basic_transport
                .discover_capabilities_authenticated_async(
                    &basic_identity,
                    "query_paths",
                    Some(7),
                    &basic,
                    &NoTokenClocks,
                    &NoTokenClocks,
                )
                .await;
            assert!(result.is_ok());
            transcript.push_str(&format!("{basic:?} {result:?}"));
            if let Some(release) = release.take() {
                release.send(()).unwrap();
            }
        }
        transcript
    };
    let cloud_requests = async {
        let mut transcript = String::new();
        for _ in 0..2 {
            let result = cloud_transport
                .discover_capabilities_authenticated_async(
                    &cloud_identity,
                    "query_paths",
                    Some(7),
                    &cloud,
                    &clock,
                    &Utc,
                )
                .await;
            assert!(result.is_ok());
            transcript.push_str(&format!("{cloud:?} {result:?}"));
        }
        transcript
    };
    let (basic_transcript, cloud_transcript, (), (), ()) =
        timeout(Duration::from_secs(10), async {
            tokio::join!(basic_requests, cloud_requests, basic_peer, cloud_peer, ims_peer)
        })
        .await
        .unwrap();
    let mut scanner =
        slingshot_development::profile_authentication_harness::SecretScanner::looking_for(&[
            "admin",
            "not-a-real-password",
            "YWRtaW46bm90LWEtcmVhbC1wYXNzd29yZA==",
            "isolated-cloud-token",
            "a1b2c3d4e5f6",
            "p8e-not-a-real-client-secret",
            "6E1B0F2A5C3D4E5F@techacct.adobe.com",
            "1A2B3C4D5E6F7A8B9C0D1E2F@AdobeOrg",
        ]);
    scanner.observe(basic_transcript);
    scanner.observe(cloud_transcript);
    scanner.require_clean().unwrap();
    // Two Cloud requests used exactly one owned exchange; Basic used none.
    for (name, listener) in traps.into_iter().chain([
        ("Basic author", basic_author),
        ("Cloud author", cloud_author),
        ("IMS", ims),
    ]) {
        assert!(
            timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
            "unexpected additional {name} connection"
        );
    }
}
