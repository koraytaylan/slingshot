//! Selected-author refresh integration checks.

use super::*;

#[path = "refresh/response.rs"]
mod response;

const SUCCESS_STATUS: u16 = 200;
const UNAUTHORIZED_STATUS: u16 = 401;
const FORBIDDEN_STATUS: u16 = 403;
const MISSING_STATUS: u16 = 404;
const REFRESH_EXCHANGE_COUNT: usize = 2;
const POST_AFTER_TOKEN_REFRESH_INDEX: usize = 2;
const EVENT_GENERATION: u64 = 7;
const FIRST_RESPONSE_DELAY_MILLISECONDS: u64 = 20;
const EXCHANGE_TIMEOUT_SECONDS: u64 = 10;

/// A complete rejection is the only wire evidence authorizing a Cloud read retry.
#[tokio::test]
async fn authenticated_requests_refresh_reads_but_never_repeat_job_posts() {
    use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    use slingshot_agent_connection::selected_author_authenticated_read::AuthenticatedReadFailure;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};

    struct Source {
        exchanges: Cell<usize>,
        fail_refresh: bool,
    }
    struct TokenTransport(usize);
    impl IdentityManagementTransport for TokenTransport {
        fn exchange(&self, _: &[u8]) -> Result<DecodedResponse, ExchangeFailure> {
            let mut response = UsableTransport.exchange(&[])?;
            response.body = format!("{{\"access_token\":\"test-token-{}\",\"token_type\":\"bearer\",\"expires_in\":3600000}}", self.0).into_bytes();
            Ok(response)
        }
    }
    impl AccessTokenSource for Source {
        fn exchange(&self) -> Result<AccessToken, ExchangeFailure> {
            let count = self.exchanges.get() + 1;
            self.exchanges.set(count);
            if self.fail_refresh && count == REFRESH_EXCHANGE_COUNT {
                return Err(ExchangeFailure::new(
                    ConfigurationFailureCode::AuthenticationTargetMismatch,
                ));
            }
            let credentials = credentials();
            IdentityManagementExchange::new(TokenTransport(count), FixedReading)
                .exchange(&credentials, &assertion(&credentials))
        }
    }

    for (cloud, asynchronous) in [(false, false), (true, false), (false, true), (true, true)] {
        for scenario in [
            "success",
            "twice",
            "forbidden",
            "truncated",
            "refresh-failure",
            "logical-lookup",
            "post-401",
            "post-403",
            "post-refresh-failure",
            "post-guard",
            "post-token401",
            "post-token-invalid",
            "artifact",
            "artifact-short",
            "artifact-twice",
            "artifact-401-short",
            "high-water",
            "physical-lookup",
            "event",
            "event-twice",
            "event-short",
            "event-401-short",
        ] {
            let post = scenario.starts_with("post-");
            if asynchronous
                && !post
                && !scenario.starts_with("artifact")
                && !scenario.starts_with("event")
            {
                continue;
            }
            // A cached Cloud token exercises post-byte refresh refusal without
            // dialing external IMS. CSRF refresh success has separate coverage.
            if asynchronous && cloud && scenario == "post-token401" {
                continue;
            }
            let artifact = scenario.starts_with("artifact");
            let event = scenario.starts_with("event");
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("https://{}", listener.local_addr().unwrap());
            let mut files = profile_files();
            replace_profile(
                &mut files,
                if cloud { "profiles/zulu.toml" } else { "profiles/mike.toml" },
                |text| {
                    text.replace(
                        if cloud {
                            "https://author.example.com"
                        } else {
                            "http://author.example.com"
                        },
                        &endpoint,
                    )
                    .replace("allow_insecure_author_transport = true\n", "")
                },
            );
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
            let async_provider = slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(
            snapshot_from_loaded_with_platform(loaded_from_files(files.clone()),
                if cloud { PROTECTED_PROFILE } else { CLEARTEXT_PROFILE },
                if cloud { PROTECTED_ENVIRONMENT } else { CLEARTEXT_ENVIRONMENT }, platform.clone())).unwrap();
            if asynchronous && cloud {
                async_cases::prime_runtime_cache(&async_provider).await;
            }
            let provider = provider_from_loaded_with_platform(
                loaded_from_files(files),
                if cloud { PROTECTED_PROFILE } else { CLEARTEXT_PROFILE },
                if cloud { PROTECTED_ENVIRONMENT } else { CLEARTEXT_ENVIRONMENT },
                platform,
            );
            let transport =
                SelectedAuthorTransport::new(provider.snapshot().author_connection()).unwrap();
            let source = Source {
                exchanges: Cell::new(0),
                fail_refresh: matches!(scenario, "refresh-failure" | "post-refresh-failure"),
            };
            let configuration = rustls::ServerConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![
                    CertificateDer::from_pem_slice(include_bytes!(
                        "../fixtures/selected-author-tls/leaf.pem"
                    ))
                    .unwrap(),
                ],
                PrivateKeyDer::from_pem_slice(include_bytes!(
                    "../fixtures/selected-author-tls/test-only-private-key.pem"
                ))
                .unwrap(),
            )
            .unwrap();
            let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(configuration));
            let identity = ExecutionIdentity {
                attempt: 1,
                author_target_identity_digest: provider.snapshot().target().to_string(),
                selected_environment_revision: provider.snapshot().revision().to_string(),
                operation_identifier: "authenticated-lookup".into(),
            };
            let provenance = slingshot_agent_protocol::wire_contract::ExpectedProvenance {
            canonical_json_contract_digest: slingshot_domain::command::schema::canonical_contract_digest(),
            command_contract: slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed("query_paths").unwrap(),
            transport_contract_digest: slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest(),
        };
            let submission = slingshot_agent_connection::command_submission::Submission::build(
                &provenance,
                slingshot_agent_protocol::identity::WireOperationIdentity::of(
                    &identity.author_target_identity_digest,
                    &identity.selected_environment_revision,
                    &identity.operation_identifier,
                    slingshot_domain::agent_identity::AgentEventStoreGeneration::of(
                        EVENT_GENERATION,
                    ),
                ),
                "subscription-one",
                r#"{"root_path":"/content"}"#,
                slingshot_agent_connection::command_submission::ExpectedArtifactManifest::empty(),
            )
            .unwrap();
            let retries = cloud
                && !asynchronous
                && matches!(
                    scenario,
                    "success"
                        | "twice"
                        | "logical-lookup"
                        | "artifact"
                        | "artifact-twice"
                        | "high-water"
                        | "physical-lookup"
                        | "event"
                        | "event-twice"
                );
            let peer = async {
                let mut previous = None;
                let last = if matches!(scenario, "post-guard" | "post-token-invalid") {
                    0
                } else if scenario == "post-token401" {
                    if cloud { POST_AFTER_TOKEN_REFRESH_INDEX } else { 0 }
                } else if scenario == "high-water" {
                    // One token fetch (preceded by a UNAUTHORIZED_STATUS refresh when cloud)
                    // plus the POST itself.
                    usize::from(cloud) + 1
                } else {
                    usize::from(retries || post)
                };
                for attempt in 0..=last {
                    let high_water_token_attempts = usize::from(cloud) + 1;
                    let high_water_posting =
                        scenario == "high-water" && attempt >= high_water_token_attempts;
                    let posting = post
                        && attempt
                            == if scenario == "post-token401" {
                                POST_AFTER_TOKEN_REFRESH_INDEX
                            } else {
                                1
                            };
                    let (socket, _) = listener.accept().await.unwrap();
                    let mut socket = acceptor.accept(socket).await.unwrap();
                    let mut head = Vec::new();
                    while !head.ends_with(b"\r\n\r\n") {
                        head.push(socket.read_u8().await.unwrap());
                        assert!(head.len() < 8192);
                    }
                    let head = String::from_utf8(head).unwrap();
                    let expected_route = if event {
                        assert!(
                            head.to_ascii_lowercase().contains("last-event-id: cursor-before\r\n")
                        );
                        format!(
                            "GET /bin/slingshot/agent/events?agent_event_store_generation=7&agent_operation_identifier={}&daemon_subscription_identifier=subscription-one HTTP/1.1\r\n",
                            submission.operation.agent_operation_identifier
                        )
                    } else if scenario == "physical-lookup" {
                        "GET /bin/slingshot/agent/jobs?sling_job_identifier=job-one HTTP/1.1\r\n"
                            .into()
                    } else if high_water_posting {
                        assert!(!head.to_ascii_lowercase().contains("last-event-id:"));
                        "POST /bin/slingshot/agent/subscriptions/high-water HTTP/1.1\r\n".into()
                    } else if scenario == "high-water" {
                        assert!(!head.to_ascii_lowercase().contains("last-event-id:"));
                        "GET /libs/granite/csrf/token.json HTTP/1.1\r\n".into()
                    } else if artifact {
                        format!(
                            "GET /bin/slingshot/agent/artifact?agent_operation_identifier={}&artifact_slot=content_package HTTP/1.1\r\n",
                            submission.operation.agent_operation_identifier
                        )
                    } else if post {
                        if !posting {
                            "GET /libs/granite/csrf/token.json HTTP/1.1\r\n".into()
                        } else {
                            "POST /bin/slingshot/agent/submit HTTP/1.1\r\n".into()
                        }
                    } else if scenario == "logical-lookup" {
                        format!(
                            "GET /bin/slingshot/agent/snapshot?agent_operation_identifier={} HTTP/1.1\r\n",
                            submission.operation.agent_operation_identifier
                        )
                    } else {
                        "GET /bin/slingshot/agent/capabilities?probe=one%20two HTTP/1.1\r\n".into()
                    };
                    assert!(head.starts_with(&expected_route));
                    if high_water_posting {
                        let lower = head.to_ascii_lowercase();
                        let at = lower
                            .as_bytes()
                            .windows(b"content-length:".len())
                            .position(|bytes| bytes == b"content-length:")
                            .expect("a POST declares its length");
                        let tail = &head.as_bytes()[at + b"content-length:".len()..];
                        let end = tail.iter().position(|byte| *byte == b'\r').unwrap();
                        let declared: usize =
                            std::str::from_utf8(&tail[..end]).unwrap().trim().parse().unwrap();
                        let mut sent = vec![0; declared];
                        socket.read_exact(&mut sent).await.unwrap();
                        assert_eq!(sent, br#"{"agent_event_store_generation":7,"daemon_subscription_identifier":"subscription-one"}"#, "refresh must preserve the exact high-water capture body");
                        assert!(
                            sent.windows(b"daemon_subscription_identifier".len())
                                .any(|bytes| bytes == b"daemon_subscription_identifier" as &[u8])
                        );
                        assert!(
                            sent.windows(b"agent_event_store_generation".len())
                                .any(|bytes| bytes == b"agent_event_store_generation" as &[u8])
                        );
                        assert!(head.to_ascii_lowercase().contains("csrf-token: test-csrf\r\n"));
                        assert!(
                            head.to_ascii_lowercase()
                                .contains("content-type: application/json\r\n")
                        );
                    }
                    if posting {
                        let expected = submission.wire_body().unwrap();
                        let mut body = vec![0; expected.len()];
                        socket.read_exact(&mut body).await.unwrap();
                        assert_eq!(body, expected);
                        assert!(head.to_ascii_lowercase().contains("csrf-token: test-csrf\r\n"));
                        assert!(head.to_ascii_lowercase().contains("idempotency-key:"));
                    }
                    let normalized = if cloud && asynchronous {
                        assert!(head.contains("Bearer not-a-real-access-token"));
                        head
                    } else if cloud {
                        let generation = if scenario == "post-token401" {
                            if attempt == 0 { 1 } else { REFRESH_EXCHANGE_COUNT }
                        } else if high_water_posting {
                            usize::from(cloud) + 1
                        } else if post {
                            1
                        } else {
                            attempt + 1
                        };
                        assert!(head.contains(&format!("Bearer test-token-{generation}")));
                        head.replace(&format!("test-token-{generation}"), "test-token")
                    } else {
                        assert!(head.contains("Basic "));
                        head
                    };
                    if !post && scenario != "high-water" {
                        if let Some(previous) = &previous {
                            assert_eq!(&normalized, previous);
                        }
                    }
                    previous = Some(normalized);
                    let status = response::status(
                        scenario,
                        cloud,
                        attempt,
                        posting,
                        high_water_token_attempts,
                    );
                    let body =
                        response::body(scenario, status, cloud, high_water_posting, &submission);
                    let length = if matches!(
                        scenario,
                        "truncated"
                            | "artifact-short"
                            | "artifact-401-short"
                            | "event-short"
                            | "event-401-short"
                    ) {
                        body.len() + 1
                    } else {
                        body.len()
                    };
                    if attempt == 0 && retries {
                        tokio::time::sleep(Duration::from_millis(
                            FIRST_RESPONSE_DELAY_MILLISECONDS,
                        ))
                        .await;
                    }
                    let media = if event && status == SUCCESS_STATUS {
                        "text/event-stream"
                    } else if artifact && status == SUCCESS_STATUS {
                        "application/zip"
                    } else {
                        "application/json"
                    };
                    socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: {media}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}").as_bytes()).await.unwrap();
                    socket.shutdown().await.unwrap();
                }
            };
            let fields = http::HeaderMap::new();
            let guard_calls = Cell::new(0);
            let mut artifact_bytes = Vec::new();
            let mut heartbeats = 0;
            let (result, ()) = timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECONDS), async {
            tokio::join!(async {
                if event {
                    use slingshot_agent_connection::{server_sent_event_decoder::{EventStreamCursor,StreamItem},selected_author_events::EventHttpOutcome};
                    let cursor=EventStreamCursor::new("cursor-before",96).unwrap();
                    let resolver=|_:&str| panic!("heartbeat must not resolve an operation");
                    let consume=|item| {assert!(matches!(item,StreamItem::Heartbeat));heartbeats+=1;Ok(())};
                    let outcome=if asynchronous && cloud {
                        transport.events_authenticated_async(&identity,"subscription-one",7,Some(&cursor),&async_provider,&async_cases::Clock,&async_cases::UnavailableUtc,resolver,consume).await
                    } else if asynchronous {
                        transport.events_authenticated_async(&identity,"subscription-one",7,Some(&cursor),&async_provider,&async_cases::NoClocks,&async_cases::NoClocks,resolver,consume).await
                    } else {transport.events_authenticated(&identity,"subscription-one",7,Some(&cursor),&provider,&source,READING,resolver,consume).await};
                    outcome.map(|outcome| match outcome {
                            EventHttpOutcome::Closed=>(SUCCESS_STATUS,0),EventHttpOutcome::Response(response)=>(response.status,0),_=>panic!("unexpected reset"),
                        }).map_err(AuthenticatedReadFailure::Transport)
                } else if scenario=="physical-lookup" {
                    transport.lookup_physical_job_authenticated(&identity,&submission,"job-one",8,&provider,&source,READING).await.map(|outcome| {
                        let slingshot_agent_connection::selected_author_lookup::PhysicalLookupReceipt::Missing(proof)=outcome else {panic!("missing physical job became a snapshot")};
                        assert_eq!(proof.generation(),8);assert_eq!(proof.sling_job_identifier(),"job-one");assert_eq!(submission.operation.agent_event_store_generation,7);(MISSING_STATUS,0)
                    }).map_err(|_|AuthenticatedReadFailure::Transport(slingshot_agent_connection::selected_author_http::FiniteHttpFailure::Head))
                } else if scenario=="high-water" {
                    transport.capture_high_water_authenticated(&identity,"subscription-one",7,&provider,&source,READING).await.map(|outcome| {
                        use slingshot_agent_connection::subscription_high_water::HighWaterOutcome;
                        match outcome {
                            HighWaterOutcome::Captured(capture) => {assert_eq!(capture.cursor().as_text(),"captured-position");assert_eq!(capture.generation(),7);assert_eq!(capture.subscription(),"subscription-one");(SUCCESS_STATUS,0)},
                            HighWaterOutcome::Response(response) => (response.status,0),
                            _ => panic!("capture became an unexpected reset"),
                        }
                    }).map_err(AuthenticatedReadFailure::Transport)
                } else if artifact {
                    use sha2::Digest;
                    let expected=slingshot_agent_connection::artifact_download::ExpectedArtifact {
                        artifact_digest:sha2::Sha256::digest(b"abc").iter().map(|byte|format!("{byte:02x}")).collect(),
                        artifact_slot:"content_package".into(),byte_length:3,media_type:"application/zip".into(),
                    };
                    let sink=|bytes:&[u8]| {artifact_bytes.extend_from_slice(bytes);Ok(())};
                    let outcome=if asynchronous && cloud {
                        transport.artifact_authenticated_async(&identity,&submission,&expected,&"3".repeat(64),&async_provider,&async_cases::Clock,&async_cases::UnavailableUtc,sink).await
                    } else if asynchronous {
                        transport.artifact_authenticated_async(&identity,&submission,&expected,&"3".repeat(64),&async_provider,&async_cases::NoClocks,&async_cases::NoClocks,sink).await
                    } else {transport.artifact_authenticated(&identity,&submission,&expected,&"3".repeat(64),&provider,&source,READING,sink).await};
                    outcome
                        .map(|outcome| {let slingshot_agent_connection::selected_author_http::ArtifactHttpOutcome::Transferred(receipt)=outcome else {panic!("artifact was not transferred")};(3,receipt.elapsed_milliseconds())})
                        .map_err(AuthenticatedReadFailure::Transport)
                } else if post {
                    let guard = || {
                        guard_calls.set(guard_calls.get()+1);
                        if scenario == "post-guard" {Err(slingshot_agent_connection::selected_author_submission::SubmissionSendRefusal::Identity)} else {Ok(())}
                    };
                    let outcome = if asynchronous && cloud {
                        transport.send_submission_authenticated_async_guarded(&identity,&submission,&async_provider,
                            &async_cases::Clock,&async_cases::UnavailableUtc,1,guard).await
                    } else if asynchronous {
                        transport.send_submission_authenticated_async_guarded(&identity,&submission,&async_provider,
                            &async_cases::NoClocks,&async_cases::NoClocks,1,guard).await
                    } else {
                        transport.send_submission_authenticated_guarded(&identity,&submission,&provider,&source,READING,1,guard).await
                    };
                    outcome
                        .map(|outcome| { assert!(matches!(outcome,slingshot_agent_connection::command_submission::SubmissionOutcome::SubmissionUnknown {..})); (0,0) })
                        .map_err(|_| AuthenticatedReadFailure::Transport(slingshot_agent_connection::selected_author_http::FiniteHttpFailure::Request))
                } else if scenario == "logical-lookup" {
                    use slingshot_agent_connection::{selected_author_lookup::OperationLookupReceipt, job_snapshot_reconciliation::LookupAnswer};
                    transport.lookup_operation_authenticated(&identity,&submission,&provider,&source,READING).await
                        .map(|receipt| { assert!(matches!(receipt,OperationLookupReceipt::Absent(LookupAnswer::Missing))); (MISSING_STATUS,0) })
                        .map_err(|_| AuthenticatedReadFailure::Transport(slingshot_agent_connection::selected_author_http::FiniteHttpFailure::Head))
                } else {
                    transport.authenticated_finite_get(&provider, &source, READING,
                        &["bin", "slingshot", "agent", "capabilities"], &[("probe", "one two")], &fields).await
                        .map(|receipt| (receipt.response.status,receipt.elapsed_milliseconds))
                }
            }, peer)
        }).await.unwrap();
            if event {
                if scenario.ends_with("short") || (asynchronous && cloud) {
                    assert!(result.is_err());
                } else {
                    assert_eq!(
                        result.unwrap().0,
                        if cloud && scenario == "event" {
                            SUCCESS_STATUS
                        } else {
                            UNAUTHORIZED_STATUS
                        }
                    );
                }
                assert_eq!(
                    heartbeats,
                    usize::from(
                        scenario == "event-short"
                            || (scenario == "event" && cloud && !asynchronous)
                    )
                );
            } else if scenario == "physical-lookup" {
                if cloud {
                    assert_eq!(result.unwrap().0, MISSING_STATUS);
                } else {
                    assert!(result.is_err());
                }
            } else if scenario == "high-water" {
                assert_eq!(
                    result.unwrap().0,
                    if cloud { SUCCESS_STATUS } else { UNAUTHORIZED_STATUS }
                );
            } else if artifact {
                if cloud && !asynchronous && scenario == "artifact" {
                    let receipt = result.unwrap();
                    assert_eq!(receipt.0, 3);
                    assert!(receipt.1 >= FIRST_RESPONSE_DELAY_MILLISECONDS);
                    assert_eq!(artifact_bytes, b"abc");
                } else {
                    assert!(result.is_err());
                    if scenario != "artifact-short" {
                        assert!(artifact_bytes.is_empty());
                    }
                }
            } else if post {
                if matches!(scenario, "post-guard" | "post-token-invalid")
                    || (scenario == "post-token401" && !cloud)
                {
                    assert!(result.is_err());
                } else {
                    assert_eq!(result.unwrap(), (0, 0));
                }
            } else if scenario == "truncated" {
                assert!(matches!(result, Err(AuthenticatedReadFailure::Transport(_))));
            } else if cloud && scenario == "refresh-failure" {
                assert!(matches!(result, Err(AuthenticatedReadFailure::Authentication(_))));
            } else if scenario == "logical-lookup" {
                if cloud {
                    assert_eq!(result.unwrap().0, MISSING_STATUS);
                } else {
                    assert!(result.is_err());
                }
            } else {
                let receipt = result.unwrap();
                assert_eq!(
                    receipt.0,
                    if scenario == "forbidden" {
                        FORBIDDEN_STATUS
                    } else if retries && scenario == "success" {
                        SUCCESS_STATUS
                    } else {
                        UNAUTHORIZED_STATUS
                    }
                );
                if retries {
                    assert!(
                        receipt.1 >= FIRST_RESPONSE_DELAY_MILLISECONDS,
                        "retry discarded first exchange time"
                    );
                }
            }
            assert_eq!(
                source.exchanges.get(),
                if !cloud || asynchronous {
                    0
                } else if retries
                    || scenario == "refresh-failure"
                    || (post
                        && !matches!(scenario, "post-403" | "post-guard" | "post-token-invalid"))
                {
                    2
                } else {
                    1
                }
            );
            if asynchronous && cloud {
                let authentication = async_provider
                    .authenticate(&endpoint, &async_cases::Clock, &async_cases::UnavailableUtc)
                    .await;
                assert_eq!(
                    authentication.is_ok(),
                    matches!(
                        scenario,
                        "post-403"
                            | "post-guard"
                            | "post-token-invalid"
                            | "artifact-short"
                            | "artifact-401-short"
                            | "event-short"
                            | "event-401-short"
                    ),
                    "only a complete 401 before stream delivery must invalidate the cached Cloud token"
                );
            }
            assert_eq!(
                guard_calls.get(),
                usize::from(
                    post && scenario != "post-token-invalid"
                        && !(scenario == "post-token401" && !cloud)
                )
            );
            assert!(
                timeout(Duration::from_millis(10), listener.accept()).await.is_err(),
                "unexpected additional retry"
            );
        }
    }
}
