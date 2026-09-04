---
id: unreviewed-dependency-duplication-refuses
title: "Unreviewed Dependency Duplication Refuses"
workstream: "0060"
kind: task
depends_on: ["the-fuzz-graph-cannot-bypass-policy"]
gated: false
touches:
  - scripts/quality
  - deny.toml
  - support/dependency-duplicate-exceptions.toml
  - crates/slingshot-development/tests/toolchain_and_dependency_policy.rs
  - crates/slingshot-development/tests/fixtures/dependency-policy/graphs.jsonl
status: complete
merged_as: "pending"
---
# Unreviewed Dependency Duplication Refuses

`cargo deny` currently reports multiple versions as warnings, and the gate accepts them without an asserted exception inventory. A new duplicate can expand compiled code and review surface without changing the result.

**Steps:**

1. Make an unreviewed duplicate-version set fatal in both product and fuzz graphs.
2. Keep exact exceptions in structured policy with package and versions, dependency paths, reason, owner, and an explicit review or expiry condition; require the observed resolved set to equal it.
3. Refuse a stale exception after versions converge and a new dependency path or version not named by its exception, with diagnostics naming the graph and shortest introducing paths.
4. Add baseline, approved, converged, changed-path, and newly introduced duplicate fixtures and prove warning exit semantics cannot make the repository gate pass.

- **Done when:** every resolved duplicate version in both lockfiles exactly matches a reviewed live exception and any new, changed, unexplained, or obsolete exception makes the quality gate fail.
