---
id: author-cross-site-request-forgery-protection
title: "Author Cross-Site Request Forgery Protection"
workstream: "0020"
kind: task
depends_on:
  - author-request-authentication
gated: false
touches:
  - crates/slingshot-agent-connection/src/author_cross_site_request_forgery_protection.rs
  - crates/slingshot-agent-connection/src/lib.rs
  - crates/slingshot-development/tests/fixtures/workspace-module-map/module-ownership.txt
  - crates/slingshot-agent-connection/tests/fixtures/author-cross-site-request-forgery-protection/**
  - crates/slingshot-agent-connection/tests/author_cross_site_request_forgery_protection.rs
status: done
merged_as: ""
---
# Author Cross-Site Request Forgery Protection

Every authenticated job POST needs one fresh Adobe Experience Manager token and one current-author referrer without making either header caller-controlled.

**Steps:**

1. Commit Basic and Cloud fixtures for exact context-prefix route construction; valid, empty, malformed, unknown-field, over-bound, expired, and restarted token responses; 401 refresh, 403, 404 ingress absence, throttling, deadlines, media/coding rejection, redirect/proxy refusal; exact Referer derivation; duplicate/header-injection rejection; and complete redaction before implementation.
2. Construct only the fixed context-prefix-preserving GET `/libs/granite/csrf/token.json` route from the immutable selected snapshot's typed author target; never decode route material from opaque AuthorTargetIdentity. Use the authenticated author connection with ambient proxy, redirects, and automatic decompression disabled; apply the shared connect/header/body/idle/status/media/coding bounds.
3. Accept only one bounded identity-encoded `application/json` response whose closed body is exactly `{"token":"..."}` with a nonempty bounded value. Classify token-route 403 as authentication refusal and bare 404 as AuthorIngressRouteUnavailable; because the job POST has not occurred, either proves no submission effect for that attempt.
4. Implement nonserializable AuthorCrossSiteRequestForgeryToken with redacted Debug and Display and no persistence, clone, trace, error, or fixture exposure. Fetch a new value immediately before every POST attempt and consume it into exactly one external `CSRF-Token` header; restart never restores a prior token.
5. Derive exactly one standard `Referer` value from normalized selected author origin—scheme, host, nondefault effective port, trailing slash, and no user information/context path/query/fragment. Accept no user, profile-header, environment-header, publisher, or arbitrary configured referrer input.
6. Reject caller-supplied or duplicate Authorization, CSRF-Token, Referer, and Idempotency-Key before network access. Treat any unvalidated POST 403 as SubmissionUnknown and use lookup by the same explicit generation and opaque AgentOperationIdentifier first; only a validated identity/generation-bearing agent rejection can prove nonexecution.
7. Report a bounded deployment diagnosis when capability or token preflight shows that author ingress does not expose the required fixed routes; never probe alternate paths, follow redirects, weaken filtering, or contact publisher.

**Tests:**

- Both credential modes fetch a fresh token immediately before every initial or same-identifier retry POST, including after expiry and daemon restart.
- Context prefixes are preserved exactly, while origin replacement, path fallback, ambient proxy, every redirect, and every publisher-shaped address receive no request.
- Only one exact JSON/media/coding response within every named bound constructs the ephemeral token; malformed, compressed, duplicate-header, delayed, unauthorized, forbidden, and missing-route responses send no following POST.
- Captured requests contain exact external `CSRF-Token` plus the selected author origin in `Referer`; duplicate or caller-injected security/idempotency headers fail before transmission.
- Unvalidated 403 after POST bytes retains SubmissionUnknown and reconciles through lookup before any fresh-token same-identifier attempt; a validated nonexecution rejection remains distinguishable.
- Whole-output, Debug, Display, trace, storage, restart, fixture, and fake-author scans contain none of the issued token or authentication values.

- **Done when:** `cargo test -p slingshot-agent-connection --test author_cross_site_request_forgery_protection` passes both deployment eras, exact token/referrer/header contracts, context-prefix/ingress, route policy, expiry/restart, ambiguity, redaction, and author-only cases.
