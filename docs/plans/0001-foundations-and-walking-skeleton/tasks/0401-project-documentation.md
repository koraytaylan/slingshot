---
id: project-documentation
title: "Project Documentation"
workstream: "0004"
kind: task
depends_on:
  - quality-automation
  - walking-skeleton-process-proof
gated: false
touches:
  - README.md
  - CONTRIBUTING.md
  - ARCHITECTURE.md
  - crates/slingshot-development/tests/product_documentation.rs
status: done
merged_as: "1c372afe92a5486b7f41ef2a9221280c8ec4d539"
---
# Project Documentation

The root documents describe the repository after the walking skeleton lands and keep all later aspirations in their plan bundles.

**Steps:**

1. Write a documentation contract test for required headings, links, exact quality commands, crate names, start/ping/non-public cleanup semantics, the canonical foundation-contract manifest, the exact landed capability inventory, and absence of configured task-marker/placeholder tokens or planning-only headings; keep semantic accuracy, completeness, historical framing, and comment quality in the explicit review checklist.
2. Write `README.md` with purpose, version, explicit start and existing-only ping invocations, crate map, exact manifest-derived abstract target triples/capabilities/artifact layout, current-environment-only untrusted native observation boundary, unpublished/no-release-metadata boundary, and the fact that no Adobe Experience Manager operation or aggregate release-platform proof exists.
3. Write `CONTRIBUTING.md` with fixtures-first claim testing, workspace-wide unsafe-code forbiddance, full-word declared names, named constants, the 1,000-line and complexity-10 ceilings including migrations, machine-checkable documentation forms, the mandatory semantic contract/comment review checklist, present-state documentation rules, dependency direction, task-footprint discipline, immutable least-privilege workflow policy, and the exact argument-free local quality gate.
4. Write root `ARCHITECTURE.md` with the landed dependency graph and both binaries, durable-job/process-support ownership, abstract platform rows versus Plan 0009 owner-authenticated native mappings, `support/foundation-contract.toml` as the sole Plan 0001 limit source, Windows remote-pipe rejection, nonce-bound cooperative stop, stable-child supervised test cleanup without PID signalling, daemon lifecycle/request/ownership, and source-policy mechanism.

**Tests:**

- Every referenced file, heading, command, package, and local link exists at this commit.
- Every documented declared target equals one abstract manifest row exactly; docs distinguish that declaration from the zero-or-one current-native untrusted observation and from Plan 0009's later authenticated all-row release evidence.
- Every documented release executable/suffix, archive layout, and smoke value equals its abstract row, and docs never claim a concrete runner/build environment, aggregate native proof, or release readiness while legal/repository metadata and Plan 0009 evidence are absent.
- The README start example creates one owner in a temporary root; following ping matches its documented shape, while ping against a fresh root returns not running and creates nothing.
- The product-documentation test rejects claims outside the exact landed capability inventory and the source-policy checker rejects configured task markers/placeholders/planning headings; a checked review record covers semantic present-state accuracy and historical framing without claiming that syntax analysis inferred them.
- `cargo doc --workspace --no-deps` succeeds with warnings denied and links from root architecture to public crate documentation resolve.

- **Done when:** `cargo test -p slingshot-development --test product_documentation`, documented temporary-root start/ping/not-running examples, and `scripts/quality` succeed with root documents describing explicit start-or-join, existing-only ping, nonce/supervision cleanup, exact foundation limits, exact-snapshot-only RustSec input, and truthful abstract/current-native/release-evidence boundaries.
