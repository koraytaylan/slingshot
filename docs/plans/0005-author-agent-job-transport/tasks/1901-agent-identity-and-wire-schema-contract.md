---
id: agent-identity-and-wire-schema-contract
title: "Agent Identity and Wire Schema Contract"
workstream: "0019"
kind: task
depends_on:
  - author-agent-transport-contract
gated: false
touches:
  - crates/slingshot-domain/src/lib.rs
  - crates/slingshot-domain/src/agent_identity.rs
  - crates/slingshot-domain/src/selected_command_contract_identity.rs
  - crates/slingshot-domain/tests/agent_identity.rs
  - crates/slingshot-agent-protocol/Cargo.toml
  - crates/slingshot-development/tests/fixtures/workspace-capability-inventory/consumer-capabilities.toml
  - crates/slingshot-development/tests/fixtures/workspace-module-map/module-ownership.txt
  - policy/workspace-capabilities.toml
  - crates/slingshot-agent-protocol/src/identity.rs
  - crates/slingshot-agent-protocol/src/wire_contract.rs
  - crates/slingshot-agent-protocol/src/lib.rs
  - crates/slingshot-agent-protocol/tests/identity_and_wire_schema_contract.rs
  - crates/slingshot-agent-protocol/tests/fixtures/identity-and-wire-schema/**
  - schemas/agent-protocol/identity/**
  - schemas/agent-protocol/common/**
status: done
merged_as: ""
---
# Agent Identity and Wire Schema Contract

Remote identity and common wire provenance must be complete before continuation-key or logical-execution state is defined.

**Steps:**

1. Adopt the dependency-ordered domain crate root and declare `agent_identity` and `selected_command_contract_identity` exactly once. Author language-neutral vectors for DaemonSubscriptionIdentifier and AgentOperationIdentifier derivations, generation, opaque target and SelectedEnvironmentRevision partitioning, exact parse rejection, and cross-generation disjointness. Consume Plan 0002's expected profile-authentication-contract drift, genuine-rotation stability, independent `VerifiedIdentityManagementTrustPolicyIdentity` and `VerifiedAuthorTrustPolicyIdentity` revision drift, and Basic/Cloud organization/client/`integration.id`-backed technical-account typed outputs without reconstructing their preimages; use its `AuthorTargetIdentityDigest` hash output directly and include a sentinel that would differ under an erroneous hash of the lowercase rendering.
2. Define the unchanged five-field SelectedCommandContractIdentity from the installed Plan 0003 registry and separately authenticate `CommandCanonicalJsonContractDigest` as the SHA-256 of exact `schemas/command-canonical-json-1.json`. Require both selected argument/result schema roots' `x-slingshot-canonical-json-contract-sha256` annotation to equal it before accepting their role digests. Define the exact SubmittedCommandDigest preimage with canonical-contract digest between limits and role-schema digests, plus AuthorAgentTransportContractDigest, exact ExpectedArtifactManifest, and complete defaulted command arguments.
3. Require artifact/annotation/role-digest authentication, then submission/result raw bytes under `slingshot.command-canonical-json/1`, then ordinary Draft 2020-12 decoded shape, then typed conversion. Reject contract-only, annotation-only, transport, limits, argument-schema, result-schema, version, argument, or artifact-manifest drift before reservation or effect.
4. Define closed common `slingshot.agent/1` identity/provenance/envelope/error schemas. Every operation-bearing document carries the exact transport and separate canonical-contract digests plus required unchanged-five-field/schema correlation; no schema claims to prove raw-byte canonicality.
5. Generate only this task's common and identity schemas and prove byte-identical regeneration from independently authored fixtures.

**Tests:**

- Exact vectors prove stable same-target/revision identity, profile-contract/principal target disjointness, direct no-second-hash target use, independent `VerifiedIdentityManagementTrustPolicyIdentity` or `VerifiedAuthorTrustPolicyIdentity` selected-revision drift refusal, cross-generation disjointness, and checked agent generation exhaustion.
- Noncanonical-but-schema-equivalent bytes fail at the raw-byte gate; schema/typed conversion is never reached. Contract-only drift fixes both role schemas/five fields and fails before raw-byte validation; annotation-only drift changes the corresponding role digest and fails independently.
- Every common schema accepts exact bytes and rejects a missing/surplus/mismatched transport digest, canonical-contract digest, or command-contract identity without mutation.

- **Done when:** focused domain/protocol tests prove exact unchanged-five-field identity, separate artifact/dual-annotation provenance, raw-canonical-before-Draft-before-typed ordering, independent digest substitution refusal, and generated schema parity, and all workspace gates succeed.
