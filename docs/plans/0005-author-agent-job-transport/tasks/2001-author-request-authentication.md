---
id: author-request-authentication
title: "Author Request Authentication"
workstream: "0020"
kind: task
depends_on:
  - author-hypertext-transfer-protocol-policy
gated: false
touches:
  - crates/slingshot-agent-connection/src/request_authentication.rs
  - crates/slingshot-agent-connection/src/lib.rs
  - crates/slingshot-agent-connection/tests/fixtures/request-authentication/**
  - crates/slingshot-agent-connection/tests/request_authentication.rs
status: planned
merged_as: ""
---
# Author Request Authentication

Apply Plan 0002 Basic or Cloud Service credentials to author requests through one redacting connection boundary.

**Steps:**

1. Commit Basic success/failure, Bearer success/failure, read-only forced-refresh, second-unauthorized, post-byte submission unauthorized, provider-error including `AccessTokenLifetimeTooShort`, distinct selected-snapshot `VerifiedIdentityManagementTrustPolicyIdentity` and `VerifiedAuthorTrustPolicyIdentity`, live root-source edits without reload, hostile additional-author-certificate-authority interception of Identity Management Services, and redaction fixtures before implementation.
2. Consume the Plan 0002 author credential provider without reopening configuration, credential, certificate, or platform-trust sources. Require its Identity Management Services exchange to retain only the immutable provider-policy-verified platform server-authentication roots and the author transport to retain only the distinct immutable effective platform-plus-selected-additional-author-certificate-authority roots from the same selected snapshot; never merge, cross-use, DER-only widen, or dynamically reload either policy.
3. Apply one Basic or Bearer Authorization header to every author request and prevent caller-supplied duplicate credential headers.
4. On one unauthorized Cloud read-only request, invoke Plan 0002's generation-aware invalidation/forced-refresh operation with the rejected token generation, join any concurrent replacement rather than invalidating it, obtain the resulting fresh token, and retry once; do not repeat Basic authentication or a second unauthorized Cloud response. For a post-byte job submission, refresh Cloud credentials through that same operation but route through same-identifier lookup-first reconciliation before any repeat; Basic unauthorized leaves ambiguous submission in RecoveryRequired until credentials recover.
5. Ensure secret-bearing wrappers, request errors, tracing fields, and fake-author recordings expose only credential kind and redacted presence.

**Tests:**

- Basic and Bearer fake-author modes accept the correct provider result.
- Basic unauthorized performs one read-only request; concurrent Cloud unauthorized responses invalidate only the rejected generation, coalesce one refresh, cannot evict a newer generation, and perform at most two read-only requests per caller.
- An unauthorized post-byte submission is never repeated directly: it retains SubmissionUnknown and uses refreshed Cloud credentials for lookup-first recovery, while Basic failure remains recoverable without a new identity.
- Provider failure, including a conservative usable-lease refusal, sends no author request; the Plan 0002 fake records one identity-management exchange and no recursive refresh for the too-short case.
- Debug, Display, error, trace, fixture, and captured-request scans contain none of the supplied username, password, token, client secret, or private-key fragments.
- Authentication is applied only to the configured author origin through the shared direct-only client; Identity Management Services and author use their distinct selected-snapshot root policies, neither dynamically reloads its root sources, an additional-author-authority-only Identity Management Services canary receives no request, and redirects and ambient proxies remain disabled.

- **Done when:** cargo test -p slingshot-agent-connection --test request_authentication passes both provider kinds, retry cardinality, provider-failure, origin, and whole-output redaction cases.
