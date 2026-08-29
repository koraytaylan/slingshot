---
id: additional-certificate-authority-and-proxy-policy
title: "Additional Certificate Authority And Proxy Policy"
workstream: "0006"
kind: task
depends_on:
  - enforce-credential-file-safety
  - selected-environment-revision
gated: false
touches:
  - crates/slingshot-configuration/src/additional_certificate_authority.rs
  - crates/slingshot-configuration/src/platform_trust.rs
  - crates/slingshot-agent-connection/src/transport_policy.rs
  - crates/slingshot-configuration/tests/additional_certificate_authority.rs
  - crates/slingshot-configuration/tests/platform_trust.rs
  - crates/slingshot-agent-connection/tests/transport_policy.rs
  - "crates/slingshot-test-support/fixtures/additional-certificate-authority/**"
status: done
merged_as: "9a5d1f94cd45fc771312d217d67dd63739d2cc93"
---
# Additional Certificate Authority And Proxy Policy

Allow one explicit environment-scoped author trust extension without permitting it, an unsafe file, or an ambient proxy to redirect credential-bearing identity-management traffic.

**Steps:**

1. Commit valid/bounded platform-root snapshots; provider trust-purpose permit/deny/distrust, application/policy restriction, unevaluable constraint, absent/present Extended Key Usage, and same-DER equal/conflicting record cases; platform enumeration failure/partial/count/entry/aggregate excess; valid single/multiple additional author certificates; reordered/PEM-respelled route-set equivalence; changed platform/additional DER; malformed/private-key/unsafe source; generation mismatch; a test additional CA that signs both the selected author and `ims-na1.adobelogin.com`; and ambient-proxy traps before implementation.
2. Snapshot platform server-authentication roots once through the supported-platform adapter. Retain exact DER only when every provider record for it is an unconditionally permitted certificate authority for Transport Layer Security server authentication, with no provider distrust/deny, externally represented purpose/application/policy/name constraint, or unevaluable setting, and with server authentication present when anchor Extended Key Usage exists. Reject conflicting duplicate records so conversion to an immutable verifier cannot broaden provider policy. Consume the optional selected `additional_certificate_authority` `SensitiveConfigurationDocument` only from the completed verified generation; parse its bounded nonempty PEM collection, require certificate-authority/key-signing/server-authentication eligibility, reject private keys, and type it as an author-only extension. Normalize failures to the exact closed `platform_trust` or `additional_certificate_authority` public diagnostic tuple and retain no reference/PEM/parser/subject/error-source bytes.
3. Construct a unique sorted platform-only root set and `VerifiedIdentityManagementTrustPolicyIdentity`, plus a distinct unique sorted platform-plus-selected-additional author root set and `VerifiedAuthorTrustPolicyIdentity`, through task `selected-environment-revision`. Give the two client builders noninterchangeable input types: identity management accepts only the platform set, while author accepts only the author union. Both derive once from the same completed selected startup snapshot; never re-open the platform store, replace platform roots, pass additional DER or the author union to identity management, or treat an additional file as client credentials.
4. Disable ambient proxy discovery explicitly for identity-management and Adobe Experience Manager client builders, including conventional proxy environment variables, and expose no publisher client builder.

**Tests:**

- Unconditionally server-authentication-trusted platform roots work for both routes without the optional file, while an additional-only fixture extends only its selected immutable author snapshot; both route identities match independently calculated values. Distrust, purpose exclusion, external constraints, unevaluable records, missing server-authentication Extended Key Usage, and conflicting duplicate decisions fail before client construction.
- Reordered/PEM-respelled/effectively duplicate roots yield the same applicable identity. A platform-set change changes both route identities on explicit restart; an additional-set-only change changes only the author identity. Neither can affect a live client.
- A test additional CA absent from the platform set authenticates the selected author successfully, but its certificate for `ims-na1.adobelogin.com` fails identity-management TLS with zero HTTP request bytes and no token; compile-time/API fixtures prove neither its DER, author union, nor author identity can be supplied to the identity-management builder.
- Every unsafe file, malformed/empty/oversized collection, and private-key document fails before client construction without exposing source, key, certificate-subject, or parser-excerpt bytes.
- Proxy environment variables pointing at a trap receive zero connections for identity-management and author requests.
- Publisher metadata cannot enter either transport builder.

- **Done when:** `cargo test -p slingshot-configuration --test additional_certificate_authority` and `cargo test -p slingshot-agent-connection --test transport_policy` prove one bounded provider-policy-verified platform snapshot supplies platform-only identity-management trust and a distinct author-only extended trust snapshot, with exact route identities, no additional-CA IMS interception, direct non-proxy transport, restart-visible route drift, and zero publisher dialing.
