---
id: check-selected-configuration
title: "Check Selected Configuration"
workstream: "0025"
kind: task
depends_on:
  - resolve-command-targets
gated: false
touches:
  - crates/slingshot-command-line/src/configuration_check.rs
  - crates/slingshot-command-line/tests/configuration_check.rs
  - "crates/slingshot-command-line/tests/fixtures/configuration-check/**"
status: planned
merged_as: ""
---
# Check Selected Configuration

Validate one selected environment and every referenced local authentication input without creating a daemon boundary or remote connection.

**Steps:**

1. Commit Basic and Cloud fixtures for explicit/default selection, safe credential and additional certificate authority files, Cloud credential JSON and private-key references, target revision, symlinks, nonregular files, unsafe modes, malformed values, sentinel secrets, every Plan 0002 `ConfigurationDiagnostic` source-class/stage/manifest-vocabulary-structural-location/code family, occurrence aggregation, and the exact below/at/above inclusive 32-item truncation-marker rule.
2. Implement the configuration-check service through Plan 0002's selector and safe file reader, validating the complete credential/deployment matrix and computing `AuthorTargetIdentity` plus `SelectedEnvironmentRevision`.
3. Return a bounded nonsecret report whose failure entries are exactly Plan 0002's closed `ConfigurationDiagnostic { source_class, stage, structural_location, code, occurrences }` values. Consume its occurrence aggregation and inclusive 32-item truncation-marker rule without reconstructing source provenance, preserving a discovery order, or adding a path/name/digest/reference/value/remediation field. Prove the service never constructs a daemon connector, invokes a process starter, or reaches a network boundary.
4. Scan every success and failure output for the fixture credential, private-key, and certificate sentinels.

**Tests:**

- `configuration_check` accepts valid Basic and Cloud selections and rejects each unsafe or malformed referenced file with only the exact closed upstream diagnostic shape; below/at/above 32 cases preserve the inclusive marker and occurrence counts without leaking a source-order oracle.
- Boundary recorders prove configuration and referenced files are read while daemon, process, and network call counts remain zero.
- Reports expose no source/profile/environment/credential/certificate path, reference or name, no digest or source order, no secret/value, no publisher route, and no renderer-authored suggestion; structural locations use only the bounded manifest vocabulary.

- **Done when:** `cargo test -p slingshot-command-line --test configuration_check` proves configuration check validates the selected profile, credential and certificate files, Cloud private key, and target revision with zero daemon/process/network calls and renders only the exact bounded nonsecret Plan 0002 diagnostic vocabulary.
