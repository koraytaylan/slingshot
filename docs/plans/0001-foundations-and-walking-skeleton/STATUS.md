# Plan 0001 — Foundations and Walking Skeleton — ✅ Complete

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** ✅ Complete.
- **Goal:** establish the complete independently checked Rust workspace/module/dependency/platform contract and prove that concurrent explicit start clients converge on one target-scoped daemon while existing-only ping never spawns.
- **Root cause:** the empty repository has no crate boundaries, enforceable engineering rules, local protocol, daemon ownership model, or executable proof on which later AEM and workflow behavior can safely build.
- **Approach:** scaffold the unpublished shell; define module/dependency ownership; pin abstract target capability/artifact rows and one canonical foundation-limit manifest; authenticate RustSec only as an exact snapshot; run all-row policy fakes plus at most the current matching native row as untrusted observation; and prove retained ping, nonce-bound stop, Windows remote-pipe policy, stable-child cleanup, and explicit start convergence before present-state documentation. Plan 0009 owns concrete authenticated all-row evidence.
- **Progress:** 16/16 tasks done; 0 blocked; 0 dropped.
- **Integration:** `complete`; run `develop`; base `main` @ `11c2e531d8a07a885321e7a09b1dccc623a733cf`; validation base `6afb29648bc1ef9a858fb2a983ebd7c26a685262`; mode `sequential`; final integration `01975a11732aa9d822349218631392dd1deb4215`.
- **Exceptions:** eight plan defects were found while implementing and were each fixed in their own change: an assertion that was true only for the commit that wrote it, a module map that could not grow, a task footprint with no place to declare a dependency between two workspace crates, a supported-target row demanding behavior no safe interface reaches, a runtime feature a deadline could not be proved without, and a dependency selection the advisory and license gates refused.
- **Outcome:** A pinned unpublished workspace with exact abstract platform/artifact contracts, one canonical wire/security/process limit source, exact-snapshot-only RustSec input, deterministic all-row policy fixtures, and at most one untrusted current-native proof that 20 explicit starts converge, ping never spawns, nonce-bound stop cannot hit a replacement, and supervised cleanup never signals by PID; Plan 0009 must authenticate every native release row.

_Last updated: 2026-08-29, against `develop` @ `542a9dc`._
