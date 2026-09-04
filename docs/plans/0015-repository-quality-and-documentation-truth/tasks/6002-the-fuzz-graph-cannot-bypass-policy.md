---
id: the-fuzz-graph-cannot-bypass-policy
title: "The Fuzz Graph Cannot Bypass Policy"
workstream: "0060"
kind: task
depends_on: []
gated: false
touches:
  - scripts/quality
  - deny.toml
  - crates/slingshot-development/tests/toolchain_and_dependency_policy.rs
  - crates/slingshot-development/tests/fixtures/dependency-policy/graphs.jsonl
status: complete
merged_as: "pending"
---
# The Fuzz Graph Cannot Bypass Policy

The fuzz workspace is deliberately excluded from the product workspace and resolves through its own lockfile. The quality gate and checksum assertions inspect only the product graph, so a dependency used by executable fuzz code can evade every dependency-policy result.

**Steps:**

1. Run the same pinned, offline source/license/advisory/yanked policy independently against the root and `fuzz/Cargo.toml`, using one authenticated advisory snapshot and no fetch or lock update.
2. Parse both manifests and lockfiles for locked resolution, permitted registries/sources, checksums, source replacement, and the pinned fuzz toolchain inputs; report which graph and package refused.
3. Add fixtures whose only violating package is reachable from the fuzz lock for each policy class, and require the gate to fail while the product graph remains unchanged.
4. Prove cold and warm runs leave both manifests and lockfiles byte-identical and cannot silently fall back to the root graph.

- **Done when:** a package reachable only from the fuzz lock cannot bypass any dependency rule applied to product dependencies, both graphs are checked offline, and neither lockfile changes during the gate.
