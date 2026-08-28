---
id: define-profile-documents
title: "Define Profile And Authentication Contract"
workstream: "0005"
kind: task
depends_on:
  - redact-secret-values
gated: false
touches:
  - policy/profile-authentication-contract-1.json
  - crates/slingshot-domain/src/profile_authentication_contract.rs
  - crates/slingshot-domain/src/configuration_snapshot.rs
  - crates/slingshot-domain/src/profile.rs
  - crates/slingshot-domain/tests/profile_authentication_contract.rs
  - crates/slingshot-domain/tests/profile_documents.rs
  - "crates/slingshot-test-support/fixtures/profile-authentication-contract/**"
  - "crates/slingshot-test-support/fixtures/configuration-snapshots/**"
  - crates/slingshot-test-support/fixtures/profiles/basic-profile.toml
  - crates/slingshot-test-support/fixtures/profiles/cloud-profile.toml
  - crates/slingshot-test-support/fixtures/profiles/invalid-authentication-pair.toml
status: planned
merged_as: ""
---
# Define Profile And Authentication Contract

Own the one canonical Plan 0002 manifest and define the closed domain shape represented by the two TOML examples in `SCOPE.md`, including author and publisher metadata and the deployment-specific authentication matrix.

**Steps:**

1. Commit canonical `policy/profile-authentication-contract-1.json` with exact format, profile/selection/configuration-snapshot/generic-source/aggregate-generation limits, `MAXIMUM_CONFIGURATION_STABLE_READ_ATTEMPTS = 2`, `MAXIMUM_CONFIGURATION_GENERATION_ATTEMPTS = 2`, platform/identity-management/author-trust limits and route-specific identity domains, diagnostic source-class/stage inventories and 32-item inclusive truncation rule, exact assertion UTC/size and form-body maxima, informational/trailer/request-header policies, failure-code registry including `configuration_file_changed_during_read`, `configuration_snapshot_inconsistent`, and `identity_management_response_trailer_rejected`, precedence arrays, identity-preimage field inventories, JSON Web Signature encoding, decoded-response-section charging, and the service-credential JSON-depth recurrence from `ARCHITECTURE.md`; independently authored exact/next-over vectors parse it without Rust constants. Add a byte-for-byte regeneration/inventory test that rejects missing, additional, renamed, reordered, differently valued, or ad-hoc public values.
2. Commit valid/invalid TOML 1.0.0 profile, optional selection, and required sorted configuration-snapshot inventories; portable-reference and lowercase-digest grammar vectors; exact/over per-source and 16,777,216-byte aggregate generation, directory, name, string, URL, path-segment, service-credential, platform-only identity-management trust, author platform-plus-selected-additional trust, 12,776-byte assertion, 25,868-byte form body, decoded HTTP head/trailer section, sampled UTC, and token limit vectors; diagnostic candidate sets of 31, 32, and 33 distinct public tuples; scalar/empty-container/alternating-object-array/unknown-member JSON documents proving root-inclusive depth below/at/above eight; every manifest literal/failure/preference tie; and independent target/revision/JWS vectors before Rust types. Fixtures read expected values from the typed manifest API or an independent parser and contain no copied numeric limit.
3. Implement the typed read-only manifest API plus fully named profile, environment, deployment, normalized `TierBaseAddress`, `AllowInsecureAuthorTransport`, Basic username, authentication, and optional certificate reference types under the exact closed TOML/member/grammar contract with author-only connection selection.
4. Canonicalize allowed origin and prefix under the exact Domain Name System/IP/port/percent/UTF-8 rules, giving root one representation and non-root prefixes one leading/no trailing slash; append individually encoded endpoint segments without URL-reference join/replacement semantics.
5. Store Basic password bytes in `SecretValue` from the first profile representation, and define canonical input as exact username bytes, one colon separator, and exact password bytes accessed only through its closure; do not normalize either input.
6. Add unit tests for manifest agreement, construction, stable ordering, base-address normalization/endpoint append, exact Basic bytes/Base64 vectors, invalid authentication pairs, and absence of publisher connection conversion.

**Tests:**

- `profile_authentication_contract` parses and regenerates the closed manifest, proves all public Rust values, per-file/two-attempt source-generation and both route-specific trust bounds, inclusive diagnostic truncation, exact JSON-depth, UTC/assertion/form maxima, informational/final/trailer charging, identity/JWS inventories, and fixture consumption, and rejects a one-byte/value/order/inventory mutation.
- `profile_documents` parses the exact closed TOML structures and rejects every crossed pair, unknown/duplicate/alias member, unsupported format, unsafe/ambiguous base prefix, empty/colon/control-bearing username, and malformed certificate reference at exact/next-over manifest bounds.
- Context-path vectors prove appending an endpoint cannot drop, replace, double, or escape the normalized prefix.
- Non-loopback cleartext author requires exact opt-in only for Basic Adobe Experience Manager 6.5; Cloud, TLS, loopback, and publisher-only cases cannot carry a meaningless opt-in.
- Adobe Experience Manager 6.5 publisher cleartext remains metadata and has no connection-target conversion.
- Basic fixtures pin empty, ASCII, Unicode, colon-bearing-password, and boundary-length username/password bytes independently from the encoder implementation.
- Compile-time and snapshot checks prove the profile has no plain string/vector password field and no serialized/debug password representation.
- Domain tests prove author selection is available and publisher metadata has no connection-target conversion.

- **Done when:** `cargo test -p slingshot-domain --test profile_authentication_contract && cargo test -p slingshot-domain --test profile_documents` proves one canonical manifest supplies every Plan 0002 public value and fixture, and both profile shapes obey the closed grammar, authentication/transport matrix including exact non-loopback cleartext opt-in, canonical root/context-path addresses, safe endpoint append, and byte-exact Basic serialization.
