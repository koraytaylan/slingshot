//! Outer concrete-provider transcript. No simulated AuthorPorts or token source.
//! Production startup is a separate outstanding composition gate.
#[path = "concrete_author_boundaries/cloud.rs"]
mod cloud;
use sha2::{Digest, Sha256};
use slingshot_agent_connection::{
    authentication::{
        environment_provider::{
            AsyncEnvironmentAuthenticationProvider, SelectedEnvironmentSnapshot,
            SnapshotAuthentication, SnapshotMaterial,
        },
        identity_management_exchange::MonotonicClock,
        token_assertion::CoordinatedUniversalTimeClock,
    },
    selected_author_transport::SelectedAuthorTransport,
    transport_policy::{AuthorTrustInput, IdentityManagementTrustInput},
};
use slingshot_configuration::{
    additional_certificate_authority::AdditionalAuthorCertificates,
    platform_trust::{
        PlatformTrustSnapshot, PlatformTrustSource, ProviderDecision, ProviderRecord,
    },
    profile_loader::{ConfigurationDiagnostic, load_profiles},
    profile_selection::{RequestedSelection, resolve},
    testing::credential_filesystem::ScriptedFilesystem,
};
use slingshot_domain::{
    operation_executor::ExecutionIdentity,
    profile::{EnvironmentAuthentication, EnvironmentName, ProfileName},
    secret_value::SecretValue,
    selected_environment_revision::{
        AuthenticationPrincipalIdentity, AuthorTargetIdentityDigest, CanonicalMetascopeSet,
        RevisionFields, SelectedEnvironmentRevision,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    time::{Duration, timeout},
};

struct Store;
impl PlatformTrustSource for Store {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        let certificates = AdditionalAuthorCertificates::parse(include_bytes!(
            "../../slingshot-agent-connection/tests/fixtures/selected-author-tls/root.pem"
        ))
        .unwrap();
        Ok(certificates
            .certificates()
            .iter()
            .map(|der| ProviderRecord {
                der: der.clone(),
                decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
            })
            .collect())
    }
}
struct NoTokenClocks;
impl MonotonicClock for NoTokenClocks {
    fn reading_milliseconds(&self) -> u64 {
        panic!("Basic requested a token clock")
    }
}
impl CoordinatedUniversalTimeClock for NoTokenClocks {
    fn sample(&self) -> Option<u64> {
        panic!("Basic requested an assertion")
    }
}
fn provider(author: &str, publisher: &str) -> AsyncEnvironmentAuthenticationProvider {
    let profile = include_str!(
        "../../slingshot-test-support/fixtures/profile-directories/ordered/profiles/mike.toml"
    )
    .replace("http://author.example.com", author)
    .replace("http://publish.example.com", publisher)
    .replace("allow_insecure_author_transport = true\n", "");
    let inventory = format!(
        "format_version = 1\n[[sources]]\nreference = \"profiles/mike.toml\"\nsha256 = \"{}\"\n",
        hex::encode(Sha256::digest(profile.as_bytes()))
    );
    let loaded = load_profiles(
        ScriptedFilesystem::new()
            .with_directory("profiles")
            .with_source("profiles/mike.toml", profile.as_bytes())
            .with_source("configuration-snapshot.toml", inventory.as_bytes()),
    )
    .unwrap();
    let selection = resolve(
        &loaded,
        &RequestedSelection {
            profile: Some(ProfileName::parse("remote-site").unwrap()),
            environment: Some(EnvironmentName::parse("staging").unwrap()),
        },
    )
    .unwrap();
    let chosen = selection.environment_of(&loaded);
    assert_eq!(chosen.publisher_metadata().as_text(), publisher);
    let EnvironmentAuthentication::BasicCredentials { user_name, .. } = chosen.authentication()
    else {
        panic!("fixture changed authentication")
    };
    let principal = AuthenticationPrincipalIdentity::basic("basic", user_name.as_text()).unwrap();
    let platform = PlatformTrustSnapshot::take(&Store).unwrap();
    let ims = IdentityManagementTrustInput::from_platform(&platform).unwrap();
    let trust = AuthorTrustInput::from_platform_and_extension(&platform, None).unwrap();
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
        authentication_method: "basic".into(),
        credential_source_reference: None,
        certificate_source_reference: None,
        proxy_policy: "direct_without_ambient_discovery".into(),
        allow_insecure_author_transport: false,
        canonical_metascope_set: CanonicalMetascopeSet::empty(),
        identity_management_trust_policy_identity: ims.identity(),
        author_trust_policy_identity: trust.identity(),
    })
    .unwrap();
    AsyncEnvironmentAuthenticationProvider::new_async(SelectedEnvironmentSnapshot::assemble(
        &selection,
        SnapshotMaterial {
            author: chosen.author_connection_target().clone(),
            publisher: chosen.publisher_metadata().clone(),
            deployment: chosen.deployment(),
            authentication: SnapshotAuthentication::BasicCredentials {
                user_name: user_name.clone(),
                password: SecretValue::from_text("not-a-real-password".into()),
            },
            principal,
            target,
            revision,
            identity_management_trust: ims,
            author_trust: trust,
        },
    ))
    .unwrap()
}

#[tokio::test]
async fn ambient_proxy_child() {
    let Ok(author) = std::env::var("SLINGSHOT_BOUNDARY_AUTHOR") else {
        return;
    };
    let publisher = std::env::var("SLINGSHOT_BOUNDARY_PUBLISHER").unwrap();
    let provider = provider(&author, &publisher);
    let transport = SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
    let identity = ExecutionIdentity {
        attempt: 1,
        operation_identifier: "outer-proof".into(),
        author_target_identity_digest: provider.snapshot().target().to_string(),
        selected_environment_revision: provider.snapshot().revision().to_string(),
    };
    let result = transport
        .discover_capabilities_authenticated_async(
            &identity,
            "query_paths",
            Some(7),
            &provider,
            &NoTokenClocks,
            &NoTokenClocks,
        )
        .await;
    println!("boundary-accepted:{}", result.is_ok());
    println!("{provider:?} {result:?}");
}

#[tokio::test]
async fn concrete_basic_discovery_is_author_only_and_refusals_never_redirect_or_retry() {
    use slingshot_agent_protocol::identity::WireContractIdentity;
    use slingshot_domain::{
        author_agent_transport_contract::AuthorAgentTransportContract,
        command::schema::canonical_contract_digest,
        selected_command_contract_identity::SelectedCommandContractIdentity,
    };
    for ambient_proxies in [false, true] {
        for (status, truncated) in [(200, false), (302, false), (401, false), (200, true)] {
            let author = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut traps = Vec::new();
            for name in ["publisher", "proxy", "redirect", "IMS", "hostile IMS"] {
                traps.push((name, TcpListener::bind("127.0.0.1:0").await.unwrap()));
            }
            let author_address = format!("http://{}/context", author.local_addr().unwrap());
            let publisher = format!("http://{}", traps[0].1.local_addr().unwrap());
            let redirect = format!("http://{}/stolen", traps[2].1.local_addr().unwrap());
            let provider = provider(&author_address, &publisher);
            let transport =
                SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
            let identity = ExecutionIdentity {
                attempt: 1,
                operation_identifier: "outer-proof".into(),
                author_target_identity_digest: provider.snapshot().target().to_string(),
                selected_environment_revision: provider.snapshot().revision().to_string(),
            };
            let mut moved = identity.clone();
            moved.selected_environment_revision = "another-revision".into();
            assert!(
                transport
                    .discover_capabilities_authenticated_async(
                        &moved,
                        "query_paths",
                        Some(7),
                        &provider,
                        &NoTokenClocks,
                        &NoTokenClocks
                    )
                    .await
                    .is_err()
            );
            assert!(
                provider.authenticate(&publisher, &NoTokenClocks, &NoTokenClocks).await.is_err()
            );
            assert!(timeout(Duration::from_millis(10), author.accept()).await.is_err());
            let body=serde_json::json!({"format":"slingshot.agent/1","agent_event_store_generation":7,
            "canonical_json_contract_digest":canonical_contract_digest(),"transport_contract_digest":AuthorAgentTransportContract::embedded_digest(),
            "command_contracts":[WireContractIdentity::from(&SelectedCommandContractIdentity::installed("query_paths").unwrap())],"continuation_authority_ready":true}).to_string();
            let peer = async {
                let (mut socket, _) = author.accept().await.unwrap();
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    head.push(socket.read_u8().await.unwrap());
                    assert!(head.len() < 8192);
                }
                let head = String::from_utf8(head).unwrap();
                assert!(
                    head.starts_with("GET /context/bin/slingshot-agent/capabilities HTTP/1.1\r\n")
                );
                assert!(
                    head.contains("Authorization: Basic YWRtaW46bm90LWEtcmVhbC1wYXNzd29yZA==\r\n")
                );
                socket.write_all(format!("HTTP/1.1 {status} Reply\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}\r\n",body.len(),if status==302{format!("Location: {redirect}\r\n")}else{String::new()}).as_bytes()).await.unwrap();
                socket
                    .write_all(if truncated {
                        &body.as_bytes()[..body.len() - 1]
                    } else {
                        body.as_bytes()
                    })
                    .await
                    .unwrap();
                socket.shutdown().await.unwrap();
            };
            let request = async {
                if ambient_proxies {
                    let proxy = format!("http://{}", traps[1].1.local_addr().unwrap());
                    let mut child = tokio::process::Command::new(std::env::current_exe().unwrap());
                    child
                        .args(["--exact", "ambient_proxy_child", "--nocapture"])
                        .env("SLINGSHOT_BOUNDARY_AUTHOR", &author_address)
                        .env("SLINGSHOT_BOUNDARY_PUBLISHER", &publisher)
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
                    assert!(output.status.success(), "proxy-isolated child failed");
                    let transcript = format!(
                        "{}{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let accepted = transcript.contains("boundary-accepted:true");
                    assert_ne!(
                        accepted,
                        transcript.contains("boundary-accepted:false"),
                        "child omitted or repeated outcome"
                    );
                    (accepted, transcript)
                } else {
                    let result = transport
                        .discover_capabilities_authenticated_async(
                            &identity,
                            "query_paths",
                            Some(7),
                            &provider,
                            &NoTokenClocks,
                            &NoTokenClocks,
                        )
                        .await;
                    (result.is_ok(), format!("{provider:?} {result:?}"))
                }
            };
            let ((accepted, transcript), ()) =
                timeout(Duration::from_secs(10), async { tokio::join!(request, peer) })
                    .await
                    .unwrap();
            assert_eq!(accepted, status == 200 && !truncated);
            let mut scanner =
                slingshot_development::profile_authentication_harness::SecretScanner::looking_for(
                    &["admin", "not-a-real-password", "YWRtaW46bm90LWEtcmVhbC1wYXNzd29yZA=="],
                );
            scanner.observe(transcript);
            scanner.require_clean().unwrap();
            assert!(
                timeout(Duration::from_millis(10), author.accept()).await.is_err(),
                "author request retried"
            );
            for (name, trap) in traps {
                assert!(
                    timeout(Duration::from_millis(10), trap.accept()).await.is_err(),
                    "traffic reached {name}"
                );
            }
        }
    }
}
