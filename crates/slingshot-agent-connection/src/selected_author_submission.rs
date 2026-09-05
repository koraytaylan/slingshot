//! One submission exchange through the immutable selected-author transport.
//!
//! The caller must persist the derived submission and obtain its durable send
//! permission before entering this method. This driver never repeats a job
//! POST: failures after a possible write return uncertainty for lookup-first
//! reconciliation. Provider-authenticated CSRF reads may refresh once.

use http::{HeaderMap, HeaderName, HeaderValue, Method};
use slingshot_agent_protocol::identity::WireOperationIdentity;
use slingshot_agent_protocol::wire_contract::ExpectedProvenance;
use slingshot_domain::agent_identity::AgentEventStoreGeneration;
use slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract;
use slingshot_domain::command::schema::canonical_contract_digest;
use slingshot_domain::operation_executor::ExecutionIdentity;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

use crate::authentication::environment_provider::RequestAuthentication;
use crate::author_cross_site_request_forgery_protection::CrossSiteRequestForgeryToken;
use crate::command_submission::{
    Checkpoint, Exchange, Submission, SubmissionOutcome, UnknownCause, parse_acknowledgement,
};
use crate::selected_author_http::FiniteHttpFailure;
use crate::selected_author_transport::SelectedAuthorTransport;

/// A refusal established before this driver writes any job POST bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SubmissionSendRefusal {
    /// Local and remote identities do not name the selected operation.
    #[error("the submission does not belong to the selected execution")]
    Identity,
    /// The retained submission no longer matches this build's contract/digest.
    #[error("the submission derivation does not match this build")]
    Derivation,
    /// Request bytes, derived headers, or the held CSRF token are invalid.
    #[error("the submission request cannot be constructed")]
    Request,
}

impl SelectedAuthorTransport {
    /// Fetches a fresh, nonpersistent token and consumes it for this one POST.
    /// A token GET refusal never causes a POST or an automatic retry.
    pub async fn send_submission_with_fresh_token(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        authentication: &RequestAuthentication,
        now_unix_milliseconds: u64,
    ) -> Result<SubmissionOutcome, SubmissionSendRefusal> {
        self.send_submission_with_fresh_token_guarded(
            identity,
            submission,
            authentication,
            now_unix_milliseconds,
            || Ok(()),
        )
        .await
    }

    /// Runs a durable caller's final local preflight after token acquisition,
    /// before opening the POST connection. This is not an execution lease.
    pub async fn send_submission_with_fresh_token_guarded(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        authentication: &RequestAuthentication,
        now_unix_milliseconds: u64,
        before_post: impl FnOnce() -> Result<(), SubmissionSendRefusal>,
    ) -> Result<SubmissionOutcome, SubmissionSendRefusal> {
        self.send_submission_with_fresh_token_over(
            identity, submission, authentication, now_unix_milliseconds, before_post, Some(false),
        ).await
    }

    /// Acquires a fresh token and sends once over HTTP/2, without fallback.
    /// The durable owner's final guard runs after token acquisition and before
    /// opening the POST connection, exactly as on the HTTP/1.1 path.
    pub async fn send_submission_with_fresh_token_http2_guarded(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        authentication: &RequestAuthentication,
        now_unix_milliseconds: u64,
        before_post: impl FnOnce() -> Result<(), SubmissionSendRefusal>,
    ) -> Result<SubmissionOutcome, SubmissionSendRefusal> {
        self.send_submission_with_fresh_token_over(
            identity, submission, authentication, now_unix_milliseconds, before_post, Some(true),
        ).await
    }

    /// Acquires a token and sends at most once using each connection's ALPN
    /// selection. Both exchanges remain on the immutable selected origin;
    /// negotiation never grants a retry or bypasses the durable final guard.
    pub async fn send_submission_with_fresh_token_negotiated_guarded(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        authentication: &RequestAuthentication,
        now_unix_milliseconds: u64,
        before_post: impl FnOnce() -> Result<(), SubmissionSendRefusal>,
    ) -> Result<SubmissionOutcome, SubmissionSendRefusal> {
        self.send_submission_with_fresh_token_over(
            identity, submission, authentication, now_unix_milliseconds, before_post, None,
        ).await
    }

    /// Authenticates token acquisition and sends the retained job at most once.
    /// A validated POST 401 may refresh the provider for subsequent lookup,
    /// but its outcome remains unknown even if refresh fails. The final durable
    /// guard runs after all pre-POST authentication work.
    pub async fn send_submission_authenticated_guarded(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        provider: &crate::authentication::environment_provider::EnvironmentAuthenticationProvider,
        source: &dyn crate::authentication::access_token_cache::AccessTokenSource,
        reading: u64,
        now_unix_milliseconds: u64,
        before_post: impl FnOnce() -> Result<(), SubmissionSendRefusal>,
    ) -> Result<SubmissionOutcome, SubmissionSendRefusal> {
        self.require_submission(identity, submission)?;
        let started = tokio::time::Instant::now();
        let receipt = self.authenticated_finite_get(
            provider, source, reading, &["libs", "granite", "csrf", "token.json"],
            &[], &HeaderMap::new(),
        ).await.map_err(|_| SubmissionSendRefusal::Request)?;
        let token = self.decode_fresh_token(&receipt)?;
        let elapsed = u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000))
            .unwrap_or(u64::MAX);
        let (authentication, lease) = provider.authenticate(
            &self.endpoint(&["bin", "slingshot-agent", "jobs"]),
            reading.saturating_add(elapsed), source,
        ).map_err(|_| SubmissionSendRefusal::Request)?;
        before_post()?;
        self.require_submission(identity, submission)?;
        let elapsed = u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000))
            .unwrap_or(u64::MAX);
        self.exchange_submission(
            submission, &authentication, &token,
            now_unix_milliseconds.saturating_add(elapsed), None,
            || async { if let Some(lease) = lease {
                // Refresh cannot turn post-byte uncertainty into pre-send refusal.
                let _ = provider.refresh_after_unauthorized(lease, source);
            } },
        ).await
    }

    /// Awaits selected runtime authentication and sends the retained POST once.
    /// Refresh after a complete 401 only prepares subsequent lookup; it cannot
    /// change an uncertain submission into a pre-send refusal or repeat it.
    pub async fn send_submission_authenticated_async_guarded<Clock, Utc>(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        provider: &crate::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider,
        clock: &Clock,
        utc: &Utc,
        now_unix_milliseconds: u64,
        before_post: impl FnOnce() -> Result<(), SubmissionSendRefusal>,
    ) -> Result<SubmissionOutcome, SubmissionSendRefusal>
    where
        Clock: crate::authentication::identity_management_exchange::MonotonicClock + Sync,
        Utc: crate::authentication::token_assertion::CoordinatedUniversalTimeClock + Sync,
    {
        self.require_submission(identity, submission)?;
        let started = tokio::time::Instant::now();
        let receipt = self.authenticated_finite_get_async(
            provider, clock, utc, &["libs", "granite", "csrf", "token.json"],
            &[], &HeaderMap::new(),
        ).await.map_err(|_| SubmissionSendRefusal::Request)?;
        let token = self.decode_fresh_token(&receipt)?;
        let (authentication, lease) = provider.authenticate(
            &self.endpoint(&["bin", "slingshot-agent", "jobs"]), clock, utc,
        ).await.map_err(|_| SubmissionSendRefusal::Request)?;
        before_post()?;
        self.require_submission(identity, submission)?;
        let elapsed = u64::try_from(started.elapsed().as_nanos().div_ceil(1_000_000))
            .unwrap_or(u64::MAX);
        self.exchange_submission(
            submission, &authentication, &token,
            now_unix_milliseconds.saturating_add(elapsed), None,
            || async { if let Some(lease) = lease {
                let _ = provider.refresh_after_unauthorized(lease, clock, utc).await;
            } },
        ).await
    }

    async fn send_submission_with_fresh_token_over(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        authentication: &RequestAuthentication,
        now_unix_milliseconds: u64,
        before_post: impl FnOnce() -> Result<(), SubmissionSendRefusal>,
        http2: Option<bool>,
    ) -> Result<SubmissionOutcome, SubmissionSendRefusal> {
        self.require_submission(identity, submission)?;
        let receipt = if http2.is_none() {
            self.finite_negotiated_query(
                Method::GET, &["libs", "granite", "csrf", "token.json"],
                &[], authentication, &HeaderMap::new(), b"",
            ).await
        } else if http2 == Some(true) {
            self.finite_http2_query(
                Method::GET, &["libs", "granite", "csrf", "token.json"],
                &[], authentication, &HeaderMap::new(), b"",
            ).await
        } else {
            self.finite_http1(
                Method::GET,
                &["libs", "granite", "csrf", "token.json"],
                authentication,
                &HeaderMap::new(),
                b"",
            )
            .await
        }.map_err(|_| SubmissionSendRefusal::Request)?;
        let token = self.decode_fresh_token(&receipt)?;
        before_post()?;
        self.require_submission(identity, submission)?;
        self.exchange_submission(
            submission,
            authentication,
            &token,
            now_unix_milliseconds.saturating_add(receipt.elapsed_milliseconds),
            http2,
            || async {},
        )
        .await
    }

    fn decode_fresh_token(
        &self,
        receipt: &crate::selected_author_http::FiniteHttpReceipt,
    ) -> Result<CrossSiteRequestForgeryToken, SubmissionSendRefusal> {
        if receipt.response.status != 200
            || !json_media_type(receipt.response.content_type.as_deref().unwrap_or(""))
        {
            return Err(SubmissionSendRefusal::Request);
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct TokenDocument {
            token: String,
        }
        let document: TokenDocument = serde_json::from_slice(&receipt.response.body)
            .map_err(|_| SubmissionSendRefusal::Request)?;
        let bound =
            AuthorAgentTransportContract::embedded().limit("maximum_author_response_header_bytes");
        if document.token.is_empty()
            || document.token.len() as u64 > bound
            || HeaderValue::from_str(&document.token).is_err()
        {
            return Err(SubmissionSendRefusal::Request);
        }
        Ok(CrossSiteRequestForgeryToken {
            origin: self.origin(),
            value: document.token,
            // This private local value is never cached or returned; its only
            // lifetime is the immediately following POST construction.
            expires_at_unix_milliseconds: u64::MAX,
        })
    }

    /// Checks the selected identity and installed derivation without opening a
    /// socket. Durable coordinators call this before admitting a pending send.
    pub fn require_submission(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
    ) -> Result<(), SubmissionSendRefusal> {
        self.require_execution(identity).map_err(|_| SubmissionSendRefusal::Identity)?;
        require_submission_derivation(identity, submission)?;
        require_valid_arguments(submission)
    }

    /// Sends the already persisted submission once. No authentication refresh,
    /// redirect, or automatic retry can issue a second POST here.
    pub async fn send_submission(
        &self,
        identity: &ExecutionIdentity,
        submission: &Submission,
        authentication: &RequestAuthentication,
        token: &CrossSiteRequestForgeryToken,
        now_unix_milliseconds: u64,
    ) -> Result<SubmissionOutcome, SubmissionSendRefusal> {
        self.require_submission(identity, submission)?;
        self.exchange_submission(submission, authentication, token, now_unix_milliseconds, Some(false), || async {}).await
    }

    async fn exchange_submission<AfterUnauthorized: std::future::Future<Output = ()>>(
        &self,
        submission: &Submission,
        authentication: &RequestAuthentication,
        token: &CrossSiteRequestForgeryToken,
        now_unix_milliseconds: u64,
        http2: Option<bool>,
        after_unauthorized: impl FnOnce() -> AfterUnauthorized,
    ) -> Result<SubmissionOutcome, SubmissionSendRefusal> {
        let body = submission.wire_body().map_err(|_| SubmissionSendRefusal::Request)?;
        let origin = self.origin();
        let mut fields = HeaderMap::new();
        for (name, value) in submission
            .request_headers(Some(token), &origin, now_unix_milliseconds, &[])
            .map_err(|_| SubmissionSendRefusal::Request)?
        {
            fields.append(
                HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| SubmissionSendRefusal::Request)?,
                HeaderValue::from_str(&value).map_err(|_| SubmissionSendRefusal::Request)?,
            );
        }
        let exchange = if http2.is_none() {
            self.finite_negotiated_query(
                Method::POST, &["bin", "slingshot-agent", "jobs"],
                &[], authentication, &fields, &body,
            ).await
        } else if http2 == Some(true) {
            self.finite_http2_query(
                Method::POST, &["bin", "slingshot-agent", "jobs"],
                &[], authentication, &fields, &body,
            ).await
        } else {
            self.finite_http1(
                Method::POST,
                &["bin", "slingshot-agent", "jobs"],
                authentication,
                &fields,
                &body,
            )
            .await
        };
        let receipt = match exchange {
            Ok(receipt) => receipt,
            Err(FiniteHttpFailure::Request) => return Err(SubmissionSendRefusal::Request),
            Err(failure) => {
                return Ok(Submission::transport_failure(match failure {
                    FiniteHttpFailure::Connect => Checkpoint::TransportConnect,
                    FiniteHttpFailure::Write => Checkpoint::RequestHead,
                    FiniteHttpFailure::Head => Checkpoint::ResponseHead,
                    FiniteHttpFailure::Body | FiniteHttpFailure::EventHeartbeat => Checkpoint::ResponseBody,
                    FiniteHttpFailure::Request => {
                        unreachable!("handled before phase classification")
                    }
                }));
            }
        };
        let response = receipt.response;
        if response.status == 401 {
            after_unauthorized().await;
            return Ok(SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::LookupRequired });
        }
        let acknowledgement = if json_media_type(response.content_type.as_deref().unwrap_or("")) {
            match parse_acknowledgement(&response.body) {
                Ok(acknowledgement) => acknowledgement,
                Err(_) => {
                    return Ok(SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Body });
                }
            }
        } else {
            return Ok(SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Media });
        };
        let retry_after_milliseconds = match response.retry_after.as_deref() {
            None => None,
            Some(value) if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => {
                Some(value.parse::<u64>().unwrap_or(u64::MAX).saturating_mul(1000))
            }
            Some(_) => {
                return Ok(SubmissionOutcome::SubmissionUnknown { cause: UnknownCause::Body });
            }
        };
        Ok(submission.interpret(&Exchange {
            acknowledgement: Some(acknowledgement),
            body_bytes: response.body.len() as u64,
            elapsed_milliseconds: receipt.elapsed_milliseconds,
            framing_ambiguous: false,
            head: response.head,
            media_type: "application/json".to_owned(),
            retry_after_milliseconds,
            status: response.status,
            trailer_section_present: false,
            trailing_bytes: false,
            unknown_fields: false,
        }))
    }
}

/// Validates retained argument bytes without repairing or rewriting them.
fn require_valid_arguments(submission: &Submission) -> Result<(), SubmissionSendRefusal> {
    use slingshot_domain::command::canonical_json::{
        ArrayOrderInventory, require_array_order, require_canonical_bytes,
    };
    use slingshot_domain::command::schema::{SchemaRole, command_schema};
    let refused = || SubmissionSendRefusal::Derivation;
    let wire_name = &submission.provenance.command_contract.command_wire_name;
    let mut value = require_canonical_bytes(submission.canonical_arguments.as_bytes())
        .map_err(|_| refused())?;
    let inventory: serde_json::Value =
        serde_json::from_str(include_str!("../../../schemas/command-canonical-json-1.json"))
            .map_err(|_| refused())?;
    let pointers = serde_json::from_value(inventory["arrays"][wire_name]["arguments"].clone())
        .map_err(|_| refused())?;
    let inventory = ArrayOrderInventory::new(pointers).map_err(|_| refused())?;
    require_array_order(&value, &inventory).map_err(|_| refused())?;
    let schema = command_schema(wire_name, SchemaRole::Arguments);
    let validator = jsonschema::draft202012::new(&schema).map_err(|_| refused())?;
    if !validator.is_valid(&value) {
        return Err(refused());
    }
    value
        .as_object_mut()
        .ok_or_else(refused)?
        .insert("command".to_owned(), wire_name.clone().into());
    let command: slingshot_domain::command::catalog::Command =
        serde_json::from_value(value).map_err(|_| refused())?;
    command.require_usable().map_err(|_| refused())
}

/// Independent proof of the argument gates beyond JSON shape.
#[cfg(test)]
mod argument_tests {
    use super::*;
    use crate::command_submission::ExpectedArtifactManifest;

    fn submission(wire: &str, arguments: &str) -> Submission {
        Submission::build(
            &ExpectedProvenance {
                canonical_json_contract_digest: canonical_contract_digest(),
                command_contract: SelectedCommandContractIdentity::installed(wire).unwrap(),
                transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
            },
            WireOperationIdentity::of(
                &"1".repeat(64),
                &"2".repeat(64),
                "operation-one",
                AgentEventStoreGeneration::of(7),
            ),
            "subscription-one",
            arguments,
            ExpectedArtifactManifest::empty(),
        )
        .unwrap()
    }

    #[test]
    fn argument_preflight_enforces_set_order_without_rewriting_retained_bytes() {
        let valid = submission(
            "download_content_package",
            r#"{"package_name":"example","roots":["/content/a","/content/b"]}"#,
        );
        assert!(require_valid_arguments(&valid).is_ok());
        for arguments in [
            r#"{"package_name":"example","roots":["/content/b","/content/a"]}"#,
            r#"{"package_name":"example","roots":["/content/a","/content/a"]}"#,
        ] {
            let held = submission("download_content_package", arguments);
            assert!(require_valid_arguments(&held).is_err());
            assert_eq!(held.canonical_arguments, arguments);
        }
    }

    #[test]
    fn argument_preflight_refuses_a_schema_valid_but_unusable_move() {
        use slingshot_domain::command::schema::{SchemaRole, command_schema};
        let arguments = r#"{"adjust_references":false,"destination_path":"/content/source/child","source_path":"/content/source"}"#;
        let mut value: serde_json::Value = serde_json::from_str(arguments).unwrap();
        let schema = command_schema("move_page", SchemaRole::Arguments);
        assert!(jsonschema::draft202012::new(&schema).unwrap().is_valid(&value));
        value["command"] = "move_page".into();
        assert!(
            serde_json::from_value::<slingshot_domain::command::catalog::Command>(value).is_ok()
        );
        assert!(require_valid_arguments(&submission("move_page", arguments)).is_err());
        let valid = arguments.replace("/content/source/child", "/content/elsewhere");
        assert!(require_valid_arguments(&submission("move_page", &valid)).is_ok());
    }
}

/// Requires exact installed contracts and the same derived remote identity.
pub fn require_submission_derivation(
    identity: &ExecutionIdentity,
    submission: &Submission,
) -> Result<(), SubmissionSendRefusal> {
    let expected = WireOperationIdentity::of(
        &identity.author_target_identity_digest,
        &identity.selected_environment_revision,
        &identity.operation_identifier,
        AgentEventStoreGeneration::of(submission.operation.agent_event_store_generation),
    );
    if submission.operation != expected {
        return Err(SubmissionSendRefusal::Identity);
    }
    let provenance = ExpectedProvenance {
        canonical_json_contract_digest: canonical_contract_digest(),
        transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
        command_contract: SelectedCommandContractIdentity::installed(
            &submission.provenance.command_contract.command_wire_name,
        )
        .map_err(|_| SubmissionSendRefusal::Derivation)?,
    };
    provenance
        .require_matching(&submission.provenance)
        .map_err(|_| SubmissionSendRefusal::Derivation)?;
    let rebuilt = Submission::build(
        &provenance,
        expected,
        &submission.daemon_subscription_identifier,
        &submission.canonical_arguments,
        submission.manifest,
    )
    .map_err(|_| SubmissionSendRefusal::Derivation)?;
    if rebuilt.submitted_command_digest != submission.submitted_command_digest {
        return Err(SubmissionSendRefusal::Derivation);
    }
    Ok(())
}

/// The protocol accepts only JSON with no parameter or one UTF-8 charset.
pub(crate) fn json_media_type(value: &str) -> bool {
    let mut parts = value.split(';');
    if !parts.next().unwrap_or("").trim().eq_ignore_ascii_case("application/json") {
        return false;
    }
    let Some(parameter) = parts.next() else { return true };
    if parts.next().is_some() {
        return false;
    }
    let Some((name, value)) = parameter.trim().split_once('=') else { return false };
    name.trim().eq_ignore_ascii_case("charset")
        && (value.trim().eq_ignore_ascii_case("utf-8")
            || value.trim().eq_ignore_ascii_case("\"utf-8\""))
}
