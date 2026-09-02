---
id: parse-cloud-service-credentials
title: "Parse Cloud Service Credentials"
workstream: "0007"
kind: task
depends_on:
  - enforce-credential-file-safety
  - redact-secret-values
  - selected-environment-revision
gated: false
touches:
  - crates/slingshot-agent-connection/src/authentication/cloud_service_credentials.rs
  - crates/slingshot-agent-connection/tests/cloud_service_credentials.rs
  - crates/slingshot-test-support/fixtures/cloud-credentials/**
status: done
merged_as: "b8169a103bb27e0fb2cac58595e0dca00dd3fa06"
---
# Parse Cloud Service Credentials

Decode already verified, bounded bytes for the environment-scoped JSON downloaded from the Adobe Experience Manager Developer Console and reject deprecated Adobe Developer Console Service Account documents as a different product contract.

**Steps:**

1. Commit redacted valid/invalid shape, wrapper, field, product, secret-parser, certificate/key, authority, metascope, generation, same-principal rotation, and one-at-a-time organization/client/technical-account identity fixtures based on the Adobe-documented shape. Include independent root-inclusive depth vectors and distinct values proving `technicalAccount.clientId` is client/audience identity while `integration.id` is the sole technical-account/JWT-sub identity.
2. Implement pure bounded duplicate-aware parsing that consumes only the selected `service_credentials` `SensitiveConfigurationDocument` from a completed `VerifiedConfigurationGeneration`. Charge JSON depth exactly as the manifest recurrence. Recognize the exact closed objects, validate `org` with `MAXIMUM_ORGANIZATION_IDENTIFIER_BYTES`, `technicalAccount.clientId` with `MAXIMUM_TECHNICAL_ACCOUNT_CLIENT_IDENTIFIER_BYTES`, and `integration.id` exactly once with `MAXIMUM_TECHNICAL_ACCOUNT_IDENTIFIER_BYTES`; no integration-identifier type/limit exists. Promptly move the extracted private key and client secret into `SecretValue`, dispose source/parser buffers when typed construction completes, pass organization, client, and technical-account identifier values to `AuthenticationPrincipalIdentity`, return canonical metascopes, validate public-certificate/private-key correspondence, accept no path, and preserve the bare authority for policy checking.
3. Normalize every parser failure into exact `service_credentials` source class, manifest stage/structural location, stable code, and checked occurrence count. Never retain or render a source reference, principal name, unknown key bytes, source excerpt, parser-library expected/actual value, or credential content; map the deprecated-product code to static human guidance citing the two official Adobe links without adding source-derived fields.

**Tests:**

- `cloud_service_credentials` accepts the complete documented success wrapper, validates every closed field, certificate/key correspondence, the bare-authority `imsEndpoint` representation, and exact product distinction; no rejected document reaches a request recorder.
- Root-inclusive depth below/at eight succeeds when otherwise valid, the would-be ninth level fails before unknown-member/shape interpretation, arrays and objects charge identically, member names add no level, and every expected result is independent of the parser implementation.
- Same-tuple private-key, client-secret, and public-certificate rotations yield the same principal identity; changing organization, client identifier, or the technical-account identifier from `integration.id` yields a different independently calculated identity.
- Reordered equal metascopes produce one canonical set and revision input; a changed set preserves principal identity but changes the revision produced by snapshot construction.
- Snapshot scans prove private key, client secret, malicious unknown-key bytes, wrong-type scalar, parser excerpt, and every fixture sentinel never appear in diagnostics, error sources, debugging, or tracing.
- Raw principal tuple values never appear in local/agent wire fixtures or diagnostics; only the opaque digest may cross those boundaries.
- Unknown fields report only the exact closed public tuple; different secret-bearing unknown keys and source references produce the same normalized diagnostic shape.
- A boundary fake proves parsing receives the selected role-tagged sensitive document only after S2, cannot reopen or substitute the credential path, and does not semantically parse an unselected credential document.

- **Done when:** `cargo test -p slingshot-agent-connection --test cloud_service_credentials` accepts only the committed Adobe Experience Manager Developer Console shape, derives the opaque organization/client/technical-account principal identity plus canonical metascopes under exact bounds/depth, and proves malicious keys/types/parser errors cannot disclose source or raw principal bytes.
