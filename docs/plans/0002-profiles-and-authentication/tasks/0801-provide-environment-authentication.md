---
id: provide-environment-authentication
title: "Provide Environment Authentication"
workstream: "0008"
kind: task
depends_on:
  - enforce-credential-file-safety
  - exchange-assertions-for-access-tokens
  - redact-secret-values
  - validate-profile-selection
  - selected-environment-revision
  - additional-certificate-authority-and-proxy-policy
gated: false
touches:
  - crates/slingshot-agent-connection/src/authentication/environment_provider.rs
  - crates/slingshot-agent-connection/tests/environment_provider.rs
status: planned
merged_as: ""
---
# Provide Environment Authentication

Build one immutable startup snapshot and expose a provider that derives request authentication for the complete selected author base address only.

**Steps:**

1. Write cases for Basic/Cloud/authors/cleartext warning, per-file retry, complete/mixed source generations, same-principal rotation, reordered/changed metascopes, equivalent/changed platform-only identity-management roots, equivalent/changed author platform-plus-selected-additional roots, hostile-additional-CA identity-management interception, Basic username and each Cloud organization/client/technical-account change, restart/root-store drift, mismatched target, publisher, and unrelated origin.
2. Assemble `SelectedEnvironmentSnapshot` once only from `VerifiedConfigurationGeneration` after S2. Parse only the selected role-tagged credential and certificate sensitive documents, derive Basic/Cloud principal and canonical metascopes from validated types, snapshot verified server-authentication platform roots once, derive the distinct author platform-plus-selected-additional root set, construct `VerifiedIdentityManagementTrustPolicyIdentity` and `VerifiedAuthorTrustPolicyIdentity`, finalize the exact contract-bound target/revision preimages with both raw identities, retain noninterchangeable route-typed root material used by the respective clients, dispose every source/parser buffer, and expose no reload, private generation digest, source reference diagnostic, or raw-principal rendering.
3. Implement closed provider dispatch and complete author-base equality plus safe endpoint append. Refuse non-loopback cleartext unless the immutable snapshot has typed opt-in and matching warning; expose warning status to configuration/connection diagnostics without secrets.
4. Test both successful variants and prove source changes do not alter a live provider, explicit reconstruction loads the new bytes, and every nonauthor target is rejected before any network client receives it.

**Tests:**

- `environment_provider` pins exact Basic input/Base64 and bearer header material at the final request boundary.
- Cloud provider cases prove every parse begins with one accepted shared stable-read buffer and no path is reopened after validation.
- Publisher, wrong-prefix, and unrelated target cases assert zero network calls; ambient proxy variables never receive a connection.
- Non-loopback cleartext without opt-in makes zero network calls; the valid opt-in sends exact Basic bytes and exposes one warning, while Cloud rejects the field and remains TLS-only.
- A newly committed same-path password/private-key/client-secret/public-certificate rotation preserving the organization/client/technical-account tuple, metascopes, and both route-specific root sets preserves target/revision and leaves the live snapshot unchanged. Reordered equivalent sets preserve revision; a metascope or either route's verified-root change preserves target but changes revision. An additional-only change leaves the identity-management identity unchanged and changes the author identity/revision. A Basic username or Cloud tuple change changes principal/target/revision; raw principals and private source digests appear nowhere observable.
- A selected additional CA can authenticate the author route but its valid-hostname `ims-na1.adobelogin.com` certificate fails identity-management TLS with zero HTTP request bytes and no token; neither live route reloads after construction.

- **Done when:** `cargo test -p slingshot-agent-connection --test environment_provider` proves one committed immutable principal/two-route-trust snapshot authorizes only safe author-prefix endpoints, preserves genuine rotation, changes revision across metascope or either route's verified-root drift, changes target for changed principal, prevents additional-author-CA identity-management interception, enforces transport rules, and never creates publisher/proxy traffic.
