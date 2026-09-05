# Task 7501 adapter completion review

This review closes **The Product Has One Author Port**, not Plan 0020 or the
compiled-product goal. It records the concrete adapter evidence after the
implementation/review loops in the task's chronological execution notes.

## Task requirements

| Requirement | Implementation and verification |
| --- | --- |
| One selected-author product adapter | `slingshot-daemon/src/author_agent_operation_executor.rs` has the sole production `AuthorPorts` implementation, `ProductAuthorPorts`. It owns `SelectedAuthorTransport`; the sole production `AuthorAgentProtocol` implementation is `RetainedAuthorProtocol`. Submit, settle and artifact completion pass through the selected transport and retained invocation identity. Test doubles implement the traits only outside this production composition. |
| Immutable endpoint, target, revision and trust | `environment_provider.rs`, `transport_policy.rs` and `selected_author_transport.rs` freeze the selected connection. Invocation construction and durable submission/lookup/event/reset/generation/terminal entry points validate authentication ownership before returning evidence or spending recovery budget. `environment_provider` integration tests and the concrete daemon admission/recovery matrices exercise target and revision-only drift, including retained-state paths that do not open a socket. |
| Bounded connection and wire phases | `connection_phase.rs`, selected-author HTTP/1 and HTTP/2 codecs, and IMS connector/client codecs enforce the contract's connection, TLS, write, head, body-idle and total bounds. The full connection suite covers exact deadline boundaries, partial input, cancellation-owned sockets and no protocol fallback. Plan 0005's normative DNS/TCP connection budget is shared; this review does not invent separate budgets from the shorter conformance prose. |
| Refuse alternate destinations and ambiguous evidence | `concrete_author_boundaries` reserves publisher/proxy/redirect/hostile-IMS traps while real Basic and Cloud requests execute. Both authentication modes run with hostile proxy environment variables in isolated children. Correctly named TLS peers prove that an author-only CA cannot authorize IMS. Connection tests cover hostname/root/version/ALPN failure, compression, duplicate/framing ambiguity, oversized heads/bodies, truncation and surplus bytes. |
| Preserve submission and recovery distinctions | `durable_author_submission.rs` retains the first-send obligation before POST and does not recreate it for an existing child. `selected_admission_orders_preflight_persistence_post_and_restart_recovery` checks preflight, persistence, stale guards, lost acknowledgement and reopened-database behavior. Invalid post-send evidence remains unknown, never permission for a second POST. Request-start retention and exact/equality expiry are covered by connection and durable tests. |
| Concrete physical-job acknowledgement conformance | `environment_provider::selected_submission_sends_bound_bytes_once_and_validates_the_answer` consumes all eight shared `command-submission/job-sets.json` fixtures over real CSRF and POST exchanges with fixed and owned async authentication. Sorted/distinct sets are accepted; empty, duplicate, unsorted and empty-element sets remain unknown without another request. The surrounding HTTP/2/TLS and identity/media tests remain in place. |
| Events, terminal evidence, reconnect and cancellation | `selected_live_events_commit_only_the_believed_prefix` covers owned/direct attachments and explicit/negotiated/provider modes; gaps, stale/exact replay, changed observations, cursor conflicts, terminal correlation, cancellation, captured-owner races and completed-event replay have durable state assertions. `subscription_reset_stages_two_authenticated_snapshots_before_atomic_installation` covers staged snapshots, generation loss, physical lookup, scheduled recovery and cancellation without partial installation. |
| Artifact truth and restart | The concrete selected-admission suite covers package and loaded-document completion, HTTP/2 lookup, async provider publication, capacity refusal, retirement, failure lookup and remote staging. Reopened repositories and verified artifact reads preserve atomic publication, terminal success, retained uncertainty and paused recovery without automatic resubmission. |
| No credential/private-principal diagnostics | Bound credentials, providers, transport and retained protocol render redacted. Outer transcripts scan Basic secrets, Cloud secrets/tokens and raw principal components. Concurrent Basic/Cloud requests and secret-only rotation prove independent provider/cache ownership, original-provider immutability and refusal of foreign leases without inventing a new semantic revision for a secret change. |

Paths above are relative to `crates/` unless otherwise stated.

## Existing agent conformance families

The seven tests in `slingshot-daemon/tests/author_agent_conformance.rs` remain
accurately described as protocol simulations. They are not used alone as proof
of a concrete transport. Their contract/provenance, credentials, routes,
submission identity, event folds, snapshot convergence and diagnostic-boundary
families are exercised by the concrete suites mapped above. In particular:

- All seven shared provenance-drift vectors plus generation/readiness refusal
  run through concrete durable admission with fixed and async authentication.
- The physical-job set vectors now traverse the actual submission codec rather
  than only the decoded interpreter.
- Exact event replay and same-sequence changed-state conflict are distinct
  shared vectors; the live-event matrix checks their durable counterparts.
- The outer Basic/Cloud tests observe actual authenticated author/IMS traffic
  and forbidden-destination listeners, not just a fake request recording.

## Verification record and review fixes

The complete agent-connection suite passed after the physical-job matrix was
added and stale heartbeat events were corrected to the current wire schema.
That run included every integration test and doc-test target, including the
full terminal-failure policy matrix. The decoder was not weakened to accept
the stale fixtures.

All 12 concrete daemon selected-admission tests passed together after eager
recovery-policy binding was added. The live-event matrix passed again after
the final terminal-credential budget regression was added. The seven agent
conformance tests and 15 executor tests passed. All eight outer concrete
boundary tests, six network-chaos tests and four profile-boundary tests passed
after credential-rotation review. Exact commands and intermediate failures are
retained in the task execution notes; a failed initial run is not counted as
successful verification.

Review fixes included strict wire bounds, owned async credential exchange and
cache cancellation, no-post-retry policy, durable guards, early authentication
binding, concrete drift/job-set coverage, and honest replay/heartbeat fixtures.
No known adapter follow-up from these review loops is left open.

## What this completion does not claim

Task 7502 owns the runtime builder, complete durable startup and readiness.
Task 7503 owns the versioned operation service, 7504 scheduler claiming, 7505
MCP composition, and 7506 product diagnostics. Task 7601 explicitly depends on
those tasks and owns compiled-binary-only catalog and crash/restart proof plus
the resulting product-documentation rewrite. Those deliverables are not
complete and remain required by the overall goal.

The loopback peers simulate the external author protocol; no test here claims
Java, AEM, Sling or JCR execution. The IMS test-support feature substitutes only
loopback TCP dialing while retaining fixed TLS identity and codecs. It is not
a runtime endpoint setting or proof of production DNS resolution. Normal
provider construction exposes no such override. These stated harness limits
do not replace the later compiled-process acceptance gate.
