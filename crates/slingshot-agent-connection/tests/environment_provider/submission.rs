//! Selected-author submission integration checks.

use super::*;

const EVENT_GENERATION: u64 = 7;
const EXCHANGE_TIMEOUT_SECONDS: u64 = 5;
const NO_REPEAT_OBSERVATION_MILLISECONDS: u64 = 10;

#[path = "submission/preflight.rs"]
mod preflight;
#[path = "submission/absent.rs"]
mod absent;
#[path = "submission/request.rs"]
mod request;
#[path = "submission/token.rs"]
mod token;
#[path = "submission/physical.rs"]
mod physical;
#[path = "submission/logical.rs"]
mod logical;
#[path = "submission/http_one.rs"]
mod http_one;
#[path = "submission/http_two_peer.rs"]
mod http_two_peer;

/// The concrete POST binds identity and preserves canonical text on the wire.
#[tokio::test]
async fn selected_submission_sends_bound_bytes_once_and_validates_the_answer() {
    use slingshot_agent_connection::command_submission::{ExpectedArtifactManifest, Submission};
    use slingshot_agent_protocol::{
        identity::WireOperationIdentity, wire_contract::ExpectedProvenance,
    };
    use slingshot_domain::{
        agent_identity::AgentEventStoreGeneration,
        author_agent_transport_contract::AuthorAgentTransportContract,
        command::schema::canonical_contract_digest,
        selected_command_contract_identity::SelectedCommandContractIdentity,
    };
    use tokio::time::{Duration, timeout};

    for (media, wrong_echo, accepted) in [
        ("application/json", false, true),
        ("Application/JSON; CHARSET=\"UTF-8\"", false, true),
        ("application/json; charset=latin1", false, false),
        ("application/json; charset=utf-8; charset=utf-8", false, false),
        ("application/json", true, false),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let mut files = profile_files();
        replace_profile(&mut files, "profiles/mike.toml", |text| {
            text.replace("http://author.example.com", &format!("{endpoint}/aem"))
                .replace("allow_insecure_author_transport = true\n", "")
        });
        let async_provider = slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(
            snapshot_from_loaded_with_platform(loaded_from_files(files.clone()), CLEARTEXT_PROFILE, CLEARTEXT_ENVIRONMENT, platform())).unwrap();
        let provider = provider_from_loaded(
            loaded_from_files(files),
            CLEARTEXT_PROFILE,
            CLEARTEXT_ENVIRONMENT,
        );
        let transport =
            SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
        let source = CountingSource { exchanges: Cell::new(0) };
        let (authentication, _) = provider
            .authenticate(provider.snapshot().author().as_text(), READING, &source)
            .unwrap();
        let identity = ExecutionIdentity {
            attempt: 1,
            author_target_identity_digest: provider.snapshot().target().to_string(),
            selected_environment_revision: provider.snapshot().revision().to_string(),
            operation_identifier: "local-operation".to_owned(),
        };
        let expected = ExpectedProvenance {
            canonical_json_contract_digest: canonical_contract_digest(),
            command_contract: SelectedCommandContractIdentity::installed("query_paths").unwrap(),
            transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
        };
        let preflight = preflight::Preflight {
            listener: &listener,
            transport: &transport,
            identity: &identity,
            authentication: &authentication,
            async_provider: &async_provider,
            provider: &provider,
            source: &source,
            expected: &expected,
        };
        preflight.discover().await;
        let arguments = r#"{"root_path":"/content/é"}"#;
        let submission = preflight.checked_submission(arguments).await;
        let answer = serde_json::json!({
            "provenance": submission.provenance,
            "selected_environment_revision": submission.operation.selected_environment_revision,
            "agent_event_store_generation": 7,
            "agent_operation_identifier": if wrong_echo { "another-operation" } else { &submission.operation.agent_operation_identifier },
            "author_target_identity_digest": identity.author_target_identity_digest,
            "already_accepted": false,
            "daemon_subscription_identifier": "subscription-one",
            "granted_retention_milliseconds": 120000,
            "physical_sling_job_identifiers": ["job-one"],
            "retired": false,
            "submitted_command_digest": submission.submitted_command_digest,
        }).to_string();
        preflight
            .verify_http_one(http_one::Case {
                submission: &submission,
                endpoint: &endpoint,
                arguments,
                media,
                wrong_echo,
                accepted,
                answer: &answer,
            })
            .await;
        if accepted && media == "application/json" {
            preflight.verify_refused_caller(&submission).await;
        }
        for automatic in [false, true] {
            let endpoint = if automatic {
                format!("https://{}", listener.local_addr().unwrap())
            } else {
                endpoint.clone()
            };
            let mut files = profile_files();
            replace_profile(&mut files, "profiles/mike.toml", |text| {
                text.replace("http://author.example.com", &format!("{endpoint}/aem"))
                    .replace("allow_insecure_author_transport = true\n", "")
            });
            use rustls_pki_types::{CertificateDer, pem::PemObject};
            let root = CertificateDer::from_pem_slice(include_bytes!(
                "../fixtures/selected-author-tls/root.pem"
            ))
            .unwrap();
            let platform = PlatformTrustSnapshot::take(&ScriptedStore {
                records: vec![ProviderRecord {
                    der: root.as_ref().to_vec(),
                    decision: ProviderDecision::UnconditionallyTrustedForServerAuthentication,
                }],
            })
            .unwrap();
            let provider = provider_from_loaded_with_platform(
                loaded_from_files(files),
                CLEARTEXT_PROFILE,
                CLEARTEXT_ENVIRONMENT,
                platform,
            );
            let transport =
                SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
            let (authentication, _) = provider
                .authenticate(provider.snapshot().author().as_text(), READING, &source)
                .unwrap();
            let identity = ExecutionIdentity {
                author_target_identity_digest: provider.snapshot().target().to_string(),
                selected_environment_revision: provider.snapshot().revision().to_string(),
                ..identity.clone()
            };
            let submission = Submission::build(
                &expected,
                WireOperationIdentity::of(
                    &identity.author_target_identity_digest,
                    &identity.selected_environment_revision,
                    &identity.operation_identifier,
                    AgentEventStoreGeneration::of(EVENT_GENERATION),
                ),
                "subscription-one",
                arguments,
                ExpectedArtifactManifest::empty(),
            )
            .unwrap();
            let mut response: serde_json::Value = serde_json::from_str(&answer).unwrap();
            response["author_target_identity_digest"] =
                identity.author_target_identity_digest.clone().into();
            response["selected_environment_revision"] =
                identity.selected_environment_revision.clone().into();
            response["agent_operation_identifier"] = if wrong_echo {
                "wrong-operation".into()
            } else {
                submission.operation.agent_operation_identifier.clone().into()
            };
            response["submitted_command_digest"] =
                submission.submitted_command_digest.clone().into();
            let answer = response.to_string();
            let snapshot=serde_json::json!({
            "provenance":submission.provenance,"agent_event_store_generation":7,
            "agent_operation_identifier":submission.operation.agent_operation_identifier,
            "author_target_identity_digest":identity.author_target_identity_digest,
            "selected_environment_revision":identity.selected_environment_revision,
            "daemon_subscription_identifier":"subscription-one",
            "submitted_command_digest":submission.submitted_command_digest,
            "subscription_watermark":"cursor-010","physical_sling_job_identifiers":["job-one"],
            "granted_retention_milliseconds":120000,"attempt":1,"progress":10,"sequence":2,"kind":"progress",
        }).to_string();
            let capability=serde_json::json!({
            "format":"slingshot.agent/1","agent_event_store_generation":7,
            "canonical_json_contract_digest":expected.canonical_json_contract_digest,
            "transport_contract_digest":expected.transport_contract_digest,
            "command_contracts":[slingshot_agent_protocol::identity::WireContractIdentity::from(&expected.command_contract)],
            "continuation_authority_ready":true,
        }).to_string();
            http_two_peer::exercise_case(
                &listener,
                &authentication,
                automatic,
                &endpoint,
                &submission,
                media,
                &answer,
                &snapshot,
                &capability,
                &transport,
                &identity,
                accepted,
            )
            .await;
        }
        preflight.reject_invalid_tokens(&submission).await;
        if accepted {
            let snapshot_body = preflight.checked_snapshot(&submission).await;
            preflight.check_physical(&submission, &snapshot_body).await;
            preflight.lookup_absent(&submission).await;
        }
        assert!(
            timeout(Duration::from_millis(NO_REPEAT_OBSERVATION_MILLISECONDS), listener.accept())
                .await
                .is_err(),
            "a second POST was attempted"
        );
    }
}
