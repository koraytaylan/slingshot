---
id: redact-secret-values
title: "Redact Secret Values"
workstream: "0006"
kind: task
depends_on:
  - profile-authentication-module-scaffold
gated: false
touches:
  - crates/slingshot-configuration/tests/secret_diagnostics.rs
  - crates/slingshot-domain/src/secret_value.rs
  - crates/slingshot-domain/tests/secret_values.rs
status: done
merged_as: ""
---
# Redact Secret Values

Provide the long-lived extracted-secret type and the temporary sensitive-source rules before profile or credential construction can expose sensitive bytes as an ordinary string or vector.

**Steps:**

1. Commit high- and low-entropy sentinel-secret fixtures and snapshots for display, debugging, error chaining, tracing, and attempted digest/comparison exposure before adding the wrapper.
2. Implement `SecretValue` as the only long-lived typed representation of an extracted password, private key, client secret, signed assertion, or token, with fixed redaction, narrowly named byte access, and zeroizing drop/replacement for its mutable owned buffer. Define the temporary `SensitiveConfigurationDocument` contract used by the filesystem task: redacted formatting, no serialization/equality/order/hash/clone/general byte-returning API, narrowly named digest/inspection/parse lending operations, and zeroizing disposal of its mutable owned buffer. Expose no secret fingerprint or stable comparison helper, and document that dependency-owned immutable parser buffers, operating-system copies, allocator internals, and externally owned bytes are outside the zeroization claim.
3. Add compile-fail coverage for serialization and secret comparison plus scans over every rendered diagnostic and tracing snapshot; errors retain only manifest-vocabulary structural locations and stable nonsecret codes.

**Tests:**

- `secret_values` pins display, debug, narrowly named access, absence of equality/digest/serialization/general-byte interfaces, temporary-source disposal, and owned-buffer drop/replacement behavior without claiming observation of memory a wrapper does not own.
- `secret_diagnostics` proves no sentinel secret or stable digest of any low-entropy sentinel appears in errors or tracing output.

- **Done when:** `cargo test -p slingshot-domain --test secret_values && cargo test -p slingshot-configuration --test secret_diagnostics` proves secrets have no observable value, digest, stable comparison oracle, or overbroad zeroization claim.
