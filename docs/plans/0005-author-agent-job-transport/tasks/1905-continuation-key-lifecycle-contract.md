---
id: continuation-key-lifecycle-contract
title: "Continuation Key Lifecycle Contract"
workstream: "0019"
kind: task
depends_on:
  - agent-identity-and-wire-schema-contract
gated: false
touches:
  - crates/slingshot-agent-protocol/src/continuation_token.rs
  - crates/slingshot-development/tests/fixtures/workspace-module-map/module-ownership.txt
  - crates/slingshot-agent-protocol/src/continuation_key_authority.rs
  - crates/slingshot-agent-protocol/src/lib.rs
  - crates/slingshot-agent-protocol/tests/continuation_key_lifecycle_contract.rs
  - crates/slingshot-agent-protocol/tests/fixtures/continuation-key-lifecycle/**
  - schemas/agent-protocol/continuation/**
status: done
merged_as: ""
---
# Continuation Key Lifecycle Contract

Continuation validation must survive the deployment topology that serves the token, while Rust remains a language-neutral contract/simulator boundary rather than a Java implementation.

**Steps:**

1. Consume Plan 0003's exact token framing, payload/binding, limits, HMAC input, and failure precedence without an alias; define the independently calculated exact current-plus-previous 431-byte maximum key-ring and 768-byte authority envelope with per-member canonical-byte charge vectors.
2. Define the three deployment profiles and compatible-readiness rules. Require the identical authenticated cluster-capable durable linearizable authority interface for AEM 6.5 single-node, AEM 6.5 cluster, and Cloud Service; current node count or deployment observation cannot relax any provider guarantee.
3. Pin authority read/CAS/fence/lease, one-record initialization, orphan/corruption refusal, least-authority encryption/ACL boundary, node replacement, rolling-version compatibility, restart, and no-silent-regeneration semantics.
4. Pin 32-byte CSPRNG keys, checked monotonic nonzero identifiers, exhaustion, constant-time 32-byte tag comparison, exclusive issuance/rotation, early/equality boundaries, and exact 960,000-ms previous-key retention.
5. Generate continuation and authority schemas/vectors while explicitly stating that external Java/AEM execution remains outside this repository.

**Tests:**

- Fresh, competing initialization, CAS conflict, timeout/ambiguity, stale fence, authority loss, orphan/corrupt/unsafe state, restart, node replacement, and rolling-version vectors fail or converge exactly.
- Every profile with an absent/unverifiable provider, relaxed durability/linearizability guarantee, or failed authority operation refuses before token/job activity; single-node AEM 6.5 uses the same replacement/scale-out-safe authority proof as the other profiles.
- Current/previous tokens survive all ready-node/restart paths; early rotation refuses, equality retires, exhaustion never wraps, and every invalid tag path uses constant-time comparison before payload interpretation.

- **Done when:** focused protocol tests prove exact 431-byte token-key state and universal deployment-profile authority semantics with multi-instance/replacement/scale-out vectors and no Java-execution claim, and all workspace gates succeed.
