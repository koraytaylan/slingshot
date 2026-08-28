---
id: prove-profile-authentication-boundaries
title: "Prove Profile Authentication Boundaries"
workstream: "0008"
kind: task
depends_on:
  - cache-cloud-access-tokens
gated: false
touches:
  - crates/slingshot-agent-connection/tests/profile_authentication_boundaries.rs
  - crates/slingshot-configuration/tests/profile_authentication_boundaries.rs
  - crates/slingshot-development/src/profile_authentication_harness.rs
  - crates/slingshot-development/tests/profile_authentication_boundaries.rs
  - docs/CONFIGURATION.md
status: planned
merged_as: ""
---
# Prove Profile Authentication Boundaries

Exercise profile loading through authenticated author requests while a publisher trap and secret scanners prove the two negative security claims.

**Steps:**

1. Commit an end-to-end transcript containing exact account-root policy and ambient traps; deterministic all-row filesystem/trust policies plus at most one untrusted native row; required configuration-snapshot publication and every old/new/mixed-generation cut; Basic/Cloud exact identity preimages; genuine rotation versus metascope/route-specific-root/principal drift; root/context/cleartext warning; access-control/file races; JSON depth; secret-parser traps; exact compact JSON Web Signature and decoded HTTP informational/final/trailer boundaries; clocks/deadlines/redirect/lease/refresh; direct platform-only identity-management and author-extended trust clients; hostile-additional-CA identity-management interception; author traffic; and forbidden publisher/proxy traffic.
2. Build the composed harness in the outermost development crate with platform-rooted fake identity-management and selected-additional-rooted author listeners plus hostile-additional-CA identity-management, redirect-location, proxy, and publisher traps. Execute it from `crates/slingshot-development/tests/profile_authentication_boundaries.rs`; the inward configuration and agent-connection integration tests prove only their focused contracts and never depend on development.
3. Document exact TOML, prefix append, TLS default, loopback exception, explicit Adobe Experience Manager 6.5 non-loopback cleartext opt-in and configuration-check warning, Basic bytes, verified snapshots, target/revision, identity exchange/clocks, direct proxy/trust, and author-only rule after proof.

**Tests:**

- The development test runs the complete transcript with shuffled files/concurrent requests, proves token refresh, exact two-attempt per-file and complete-generation retry/refusal including aggregate bytes, immutable live snapshots, explicit-restart platform/additional-root drift, and independently calculated contract/principal/target/identity-management-trust/author-trust/revision/JWS/informational-final-trailer values.
- The same-named configuration and agent-connection tests retain focused verified-loader and provider/transport boundary coverage without importing `slingshot-development`.
- Proxy and publisher accept counts remain zero and a scan of standard streams, errors, error sources, debug values, local/agent wire captures, and traces finds none of the fixture secrets including malicious key/type/parser bytes or their stable digests and none of the raw Basic/Cloud principal tuple.
- The redirect transcript records exactly one secret-bearing exchange request at the validated original identity-management endpoint, zero accepts at its `Location`, and no token/result derived from the three-hundred response.
- The hostile-CA transcript proves the same selected additional CA authenticates author while its certificate for `ims-na1.adobelogin.com` yields no authenticated identity-management request bytes and no token.

- **Done when:** `cargo test -p slingshot-configuration --test profile_authentication_boundaries && cargo test -p slingshot-agent-connection --test profile_authentication_boundaries && cargo test -p slingshot-development --test profile_authentication_boundaries` proves deterministic policies, one committed bounded source generation, exact contract/principal/target/two-route-trust/revision/JWS/head construction, genuine rotation versus security drift, no additional-author-CA identity-management trust widening, safe direct author-only authentication/refresh, and complete redaction; Plan 0009 remains the sole authenticated aggregate native gate.
