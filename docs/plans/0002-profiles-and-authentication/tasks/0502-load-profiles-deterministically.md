---
id: load-profiles-deterministically
title: "Load Profiles Deterministically"
workstream: "0005"
kind: task
depends_on:
  - define-profile-documents
  - enforce-credential-file-safety
gated: false
touches:
  - crates/slingshot-configuration/src/profile_loader.rs
  - crates/slingshot-configuration/tests/profile_loading.rs
  - crates/slingshot-test-support/fixtures/profile-directories/duplicate-name/**
  - crates/slingshot-test-support/fixtures/profile-directories/ordered/**
status: done
merged_as: "ae71b0991e0699cef41decd7301ddc5b5c246c24"
---
# Load Profiles Deterministically

Load exactly one profile from each ordinary TOML file below the profiles directory and produce stable results independent of directory enumeration order.

**Steps:**

1. Commit directory fixtures with reversed filenames, duplicate/unsupported/unknown/version cases, normalized/invalid prefixes, every insecure-author opt-in combination, invalid Basic usernames/certificate references, links/old-handle replacements, owner/permission violations, valid/changed/missing/surplus/mixed-generation snapshot inventories, verified selection, and distinct sentinels in malformed source excerpts, unknown keys, wrong-type values, private source digests, credential references, usernames, and passwords.
2. Implement bounded nonrecursive discovery plus `ConfigurationSourceInventoryInspector` for the generic S1/source/S2 coordinator. Directory discovery preclassifies profile references, while the generic authority's no-follow optional-presence result for exact `selection.toml` must agree with S1 and preclassifies that optional reference, so the coordinator enforces both smaller class limits before inspection. The inspector receives those role-tagged `SensitiveConfigurationDocument` values plus the generic-bounded role-unknown collection, strictly parses only profile/selection TOML, and returns individually typed documents, requested selection names, and the exact role-tagged transitive service-credential and additional-certificate-authority reference inventory. It deliberately defers cross-document duplicate-name and profile/environment-selection resolution until after the coordinator proves S2. Each canonical reference has exactly one role; same-role reuse deduplicates, while cross-role reuse or reuse of a fixed snapshot/selection/profile location fails generation consistency. The inspector has no JSON, PEM, trust, or key parser. The coordinator, not this inspector, verifies private digests, exact inventory/class limits, aggregate bytes, and S2 equality before producing `VerifiedConfigurationGeneration`; unselected credential/certificate documents remain opaque and require no selected-environment semantic validation.
3. After `VerifiedConfigurationGeneration` returns, validate cross-document profile-name uniqueness and order the retained typed profiles while preserving the requested explicit/default names for task `validate-profile-selection`. Immediately replace every TOML lexer, syntax, duplicate-member, type, unknown-field, per-document semantic, and cross-document duplicate-name error with a closed `ConfigurationDiagnostic` containing only manifest source class, stage, structural location, code, and checked occurrence count. Coalesce/sort without a source-reference key and enforce the 32-item inclusive marker rule; retain no source reference, profile/environment name, digest, parser source chain, excerpt, unknown-key bytes, expected/actual scalar, credential reference, username, or password. Add tests that permute filesystem enumeration and compare complete values and diagnostics byte for byte.

**Tests:**

- `profile_loading` proves both enumeration orders yield identical sorted profiles and environments.
- Rejection cases pin normalized duplicate/unsupported/unknown/version errors, unsafe prefixes, missing/meaningless/Cloud insecure opt-in, invalid username/certificate reference, every file-authority failure, diagnostic coalescing plus 31/32/33-item truncation, and indistinguishable failures from differently named source references without exposing any fixture sentinel or digest.
- Valid opted-in selection emits one stable `InsecureAuthorTransportWarning`; TLS/loopback selections emit none and warning ordering is deterministic.
- Every profile/selection fixture is parsed from one committed inventory after the verified handle's two equal bounded reads and three equal evidence samples; no accepted path is reopened for parsing, while credential/certificate bytes remain opaque until selected after S2.
- Loader integration tests inject the generic fake authority and prove old-or-new atomic replacement is truthful while target, credential, password, trust, or selection sources cannot cross generations; private comparison digests appear nowhere observable.

- **Done when:** `cargo test -p slingshot-configuration --test profile_loading` produces identical values, warnings, and closed sentinel-free diagnostics across directory permutations while proving its parser-independent inspector classifies one complete committed root-contained generation and never parses or reopens credential/certificate sources.
