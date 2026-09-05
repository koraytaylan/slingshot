//! Provider-owned credentials/cache/client with only test TCP dialing substituted.
use super::*;
use crate::{
    authentication::environment_provider::{
        AsyncEnvironmentAuthenticationProvider, SelectedEnvironmentSnapshot,
        SnapshotAuthentication, SnapshotMaterial,
    },
    transport_policy::AuthorTrustInput,
};
use sha2::{Digest, Sha256};
use slingshot_configuration::{
    additional_certificate_authority::AdditionalAuthorCertificates,
    profile_loader::load_profiles,
    profile_selection::{RequestedSelection, resolve},
    testing::credential_filesystem::ScriptedFilesystem,
};
use slingshot_domain::{
    profile::{EnvironmentName, ProfileName},
    selected_environment_revision::{
        AuthorTargetIdentityDigest, CanonicalMetascopeSet, RevisionFields,
        SelectedEnvironmentRevision,
    },
};

fn snapshot(ims_trusted: bool) -> SelectedEnvironmentSnapshot {
    let profile = include_str!(
        "../../../slingshot-test-support/fixtures/profile-directories/ordered/profiles/zulu.toml"
    )
    .replace(
        "[environments.production]\n",
        "[environments.production]\nadditional_ca_certificate_file = \"certificates/author.pem\"\n",
    );
    let ims_root = include_bytes!("../../tests/fixtures/selected-author-tls/ims-test-root.pem");
    let certificate_digest: String =
        Sha256::digest(ims_root).iter().map(|b| format!("{b:02x}")).collect();
    let digest: String =
        Sha256::digest(profile.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    let credential_bytes =
        include_bytes!("../../../slingshot-test-support/fixtures/cloud-credentials/valid.json");
    let credential_digest: String =
        Sha256::digest(credential_bytes).iter().map(|b| format!("{b:02x}")).collect();
    let inventory = format!(
        "format_version = 1\n[[sources]]\nreference = \"certificates/author.pem\"\nsha256 = \"{certificate_digest}\"\n[[sources]]\nreference = \"credentials/alpha.json\"\nsha256 = \"{credential_digest}\"\n[[sources]]\nreference = \"profiles/zulu.toml\"\nsha256 = \"{digest}\"\n"
    );
    let loaded = load_profiles(
        ScriptedFilesystem::new()
            .with_directory("profiles")
            .with_directory("credentials")
            .with_directory("certificates")
            .with_source("certificates/author.pem", ims_root)
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
    let root = CertificateDer::from_pem_slice(if ims_trusted {
        ims_root.as_slice()
    } else {
        include_bytes!("../../tests/fixtures/selected-author-tls/root.pem")
    })
    .unwrap();
    let platform = PlatformTrustSnapshot::take(&Store(root.as_ref().to_vec())).unwrap();
    let ims = IdentityManagementTrustInput::from_platform(&platform).unwrap();
    // Always install the IMS fixture CA as an author extension as well. When
    // absent from platform trust it must not become an IMS trust anchor.
    let extension = AdditionalAuthorCertificates::parse(ims_root).unwrap();
    let author =
        AuthorTrustInput::from_platform_and_extension(&platform, Some(&extension)).unwrap();
    let credentials = credentials();
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
        profile_name: selection.profile_name().as_text().to_owned(),
        environment_name: selection.environment_name().as_text().to_owned(),
        profile_source_reference: selection.profile_source().as_text().to_owned(),
        selection_source_reference: None,
        author_target_identity: target,
        publisher_base_address: chosen.publisher_metadata().as_text().to_owned(),
        authentication_method: chosen.authentication().method().to_owned(),
        credential_source_reference: Some("credentials/alpha.json".into()),
        certificate_source_reference: chosen
            .additional_certificate_authority_file()
            .map(|reference| reference.as_text().to_owned()),
        proxy_policy: "direct_without_ambient_discovery".into(),
        allow_insecure_author_transport: false,
        canonical_metascope_set: metascopes,
        identity_management_trust_policy_identity: ims.identity(),
        author_trust_policy_identity: author.identity(),
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
            author_trust: author,
        },
    )
}

#[tokio::test]
async fn owned_provider_signs_refreshes_and_reuses_one_cache_without_source_injection() {
    for h2 in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let provider = AsyncEnvironmentAuthenticationProvider::new_async_test_socket(
            snapshot(true),
            listener.local_addr().unwrap(),
        )
        .unwrap();
        let acceptor =
            server(&rustls::version::TLS13, Some(if h2 { b"h2" } else { b"http/1.1" }), false);
        let credentials = credentials();
        let second = vectors()["sampled_second"].as_u64().unwrap();
        let utc = Utc(AtomicU64::new(second));
        let clock = Clock(AtomicU64::new(0));
        let endpoint = provider.author_endpoint(&["bin", "slingshot-agent", "capabilities"]);
        assert!(provider.authenticate("https://publish.example.com", &clock, &utc).await.is_err());
        assert_eq!(clock.0.load(Ordering::SeqCst), 0);
        assert_eq!(utc.0.load(Ordering::SeqCst), second);
        let request = async {
            let (authentication, first) =
                provider.authenticate(&endpoint, &clock, &utc).await.unwrap();
            authentication.lend_value_bytes(|b| assert_eq!(b, b"Bearer fixture-1"));
            let first = first.unwrap();
            let (_, cached) = provider.authenticate(&endpoint, &clock, &utc).await.unwrap();
            assert_eq!(cached.unwrap().generation(), 1);
            let (authentication, replacement) =
                provider.refresh_after_unauthorized(first.clone(), &clock, &utc).await.unwrap();
            authentication.lend_value_bytes(|b| assert_eq!(b, b"Bearer fixture-2"));
            assert_eq!(replacement.generation(), 2);
            let (_, stale) =
                provider.refresh_after_unauthorized(first, &clock, &utc).await.unwrap();
            assert_eq!(stale.generation(), 2);
            let foreign = AsyncEnvironmentAuthenticationProvider::new_async_test_socket(
                snapshot(true),
                listener.local_addr().unwrap(),
            )
            .unwrap();
            assert_eq!(
                foreign
                    .refresh_after_unauthorized(replacement, &clock, &utc)
                    .await
                    .unwrap_err()
                    .code,
                ConfigurationFailureCode::AuthenticationTargetMismatch
            );
        };
        let peer = async {
            for number in 1..=2 {
                let (socket, _) = listener.accept().await.unwrap();
                let mut peer = acceptor.accept(socket).await.unwrap();
                signed_request(&mut peer, h2, &credentials, second + number as u64 - 1).await;
                answer(&mut peer, h2, number).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), async { tokio::join!(request, peer) })
            .await
            .unwrap();
        assert_eq!(utc.0.load(Ordering::SeqCst), second + 2);
        assert!(tokio::time::timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    }
}

#[tokio::test]
async fn author_extension_cannot_authorize_provider_owned_ims_exchange() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = AsyncEnvironmentAuthenticationProvider::new_async_test_socket(
        snapshot(false),
        listener.local_addr().unwrap(),
    )
    .unwrap();
    let acceptor = server(&rustls::version::TLS13, Some(b"http/1.1"), false);
    let second = vectors()["sampled_second"].as_u64().unwrap();
    let utc = Utc(AtomicU64::new(second));
    let clock = Clock(AtomicU64::new(0));
    let endpoint = provider.author_endpoint(&["bin", "slingshot-agent", "capabilities"]);
    let peer = async {
        let (socket, _) = listener.accept().await.unwrap();
        if let Ok(mut peer) = acceptor.accept(socket).await {
            let mut byte = [0];
            assert!(!matches!(peer.read(&mut byte).await, Ok(1)));
        }
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(provider.authenticate(&endpoint, &clock, &utc), peer)
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().code, ConfigurationFailureCode::IdentityManagementTlsFailed);
    assert_eq!(
        clock.0.load(Ordering::SeqCst),
        100,
        "only cache lookup sampled; no wire request anchor"
    );
    assert!(tokio::time::timeout(Duration::from_millis(10), listener.accept()).await.is_err());
}

#[tokio::test]
async fn cancelled_owned_provider_refresh_fails_joiners_and_requires_a_new_exchange() {
    for h2 in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let provider = AsyncEnvironmentAuthenticationProvider::new_async_test_socket(
            snapshot(true),
            listener.local_addr().unwrap(),
        )
        .unwrap();
        let acceptor =
            server(&rustls::version::TLS13, Some(if h2 { b"h2" } else { b"http/1.1" }), false);
        let credentials = credentials();
        let second = vectors()["sampled_second"].as_u64().unwrap();
        let utc = Utc(AtomicU64::new(second));
        let clock = Clock(AtomicU64::new(0));
        let endpoint = provider.author_endpoint(&["bin", "slingshot-agent", "capabilities"]);
        let (sent, ready) = tokio::sync::oneshot::channel();
        let request = async {
            let (_, lease) = provider.authenticate(&endpoint, &clock, &utc).await.unwrap();
            let mut owner =
                Box::pin(provider.refresh_after_unauthorized(lease.unwrap(), &clock, &utc));
            tokio::select! {
                result = &mut owner => panic!("unfinished provider refresh returned {result:?}"),
                result = ready => result.unwrap(),
            }
            let mut joined = Box::pin(provider.authenticate(&endpoint, &clock, &utc));
            std::future::poll_fn(|cx| {
                assert!(joined.as_mut().poll(cx).is_pending());
                Poll::Ready(())
            })
            .await;
            drop(owner);
            assert_eq!(
                joined.await.unwrap_err().code,
                ConfigurationFailureCode::IdentityManagementCancelled
            );
            let (authentication, lease) =
                provider.authenticate(&endpoint, &clock, &utc).await.unwrap();
            authentication.lend_value_bytes(|b| assert_eq!(b, b"Bearer fixture-3"));
            assert_eq!(lease.unwrap().generation(), 2);
        };
        let peer = async {
            let mut sent = Some(sent);
            for number in 1..=3 {
                let (socket, _) = listener.accept().await.unwrap();
                let mut peer = acceptor.accept(socket).await.unwrap();
                signed_request(&mut peer, h2, &credentials, second + number as u64 - 1).await;
                if number == 2 {
                    sent.take().unwrap().send(()).unwrap();
                    let mut byte = [0];
                    assert!(!matches!(peer.read(&mut byte).await, Ok(1)));
                } else {
                    answer(&mut peer, h2, number).await;
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(10), async { tokio::join!(request, peer) })
            .await
            .unwrap();
        assert_eq!(utc.0.load(Ordering::SeqCst), second + 3);
        assert!(tokio::time::timeout(Duration::from_millis(10), listener.accept()).await.is_err());
    }
}
