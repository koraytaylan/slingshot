---
id: validate-profile-selection
title: "Validate Profile Selection"
workstream: "0005"
kind: task
depends_on:
  - load-profiles-deterministically
gated: false
touches:
  - crates/slingshot-configuration/src/profile_selection.rs
  - crates/slingshot-configuration/tests/profile_selection.rs
status: planned
merged_as: ""
---
# Validate Profile Selection

Resolve explicit or configured-default names into one immutable selected environment and its daemon namespace without silently choosing a first entry.

**Steps:**

1. Write table fixtures covering explicit selection, complete defaults, partial defaults, absent names, multiple candidates, and each valid insecure-author warning state before implementing selection.
2. Resolve selection only from the post-S2 unique typed collection. A successful internal result retains the canonical profile/selection source provenance required by revision construction, a canonical `(profile, environment)` namespace key, and the deterministic optional `InsecureAuthorTransportWarning` projected from the selected environment. A failure emits only the closed `ConfigurationDiagnostic` source class/stage/manifest structural location/code/occurrence tuple, with no requested name, candidate suggestion, source reference, or ordering oracle.
3. Test that no ordering change can alter the selected value or public diagnostic and that differently named missing profile/environment inputs normalize to the same applicable diagnostic tuple.

**Tests:**

- `profile_selection` covers every explicit/default combination and exact namespace key.
- Permutation cases prove candidate ordering does not affect selection or diagnostics; public failures contain no request/candidate/source name while successful internal provenance remains available only to the identity builder.
- Non-loopback cleartext Adobe Experience Manager 6.5 selection exposes exactly one stable warning for configuration-check and connection consumers, while TLS and loopback selection expose none.

- **Done when:** `cargo test -p slingshot-configuration --test profile_selection` proves selection is explicit, stable, never first-found, and carries the exact insecure-author warning only for a valid opted-in non-loopback cleartext selection.
