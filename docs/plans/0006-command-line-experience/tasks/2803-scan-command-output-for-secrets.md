---
id: scan-command-output-for-secrets
title: "Scan Command Output For Secrets"
workstream: "0028"
kind: task
depends_on:
  - pin-command-golden-sessions
gated: false
touches:
  - crates/slingshot-command-line/tests/command_secret_scans.rs
  - crates/slingshot-test-support/fixtures/command-secret-scans/**
status: planned
merged_as: ""
---
# Scan Command Output For Secrets

Prove that command arguments, progress, results, diagnostics, traces, and golden diffs never expose profile passwords or Cloud credential material.

**Steps:**

1. Commit distinct sentinel values for every secret class and for configuration source/profile/environment/credential/certificate path, reference, name, digest, and ordering provenance that Plan 0002 forbids publicly; cover failures at parsing, configuration check, daemon start, authentication, submission, observation, result, artifact, maintenance-result, and interrupt boundaries.
2. Run the compiled process with capture of arguments, both streams, tracing, daemon transcript, generated files, and failure diffs.
3. Scan raw bytes and common encodings for every sentinel. For configuration check, additionally schema-validate that every public diagnostic has only `source_class`, `stage`, manifest-vocabulary `structural_location`, `code`, and `occurrences` and that the inclusive 32-item truncation marker is respected; do not treat a profile/path/name/digest/order oracle as a useful nonsecret fingerprint.

**Tests:**

- `command_secret_scans` covers every failure boundary and secret class.
- Positive controls prove the scanner fails when a harness helper deliberately emits each sentinel encoding.

- **Done when:** `cargo test -p slingshot-command-line --test command_secret_scans` finds no secret in any process-observable artifact.
