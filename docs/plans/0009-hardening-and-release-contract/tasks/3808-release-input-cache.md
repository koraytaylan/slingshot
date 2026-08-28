---
id: release-input-cache
title: "Release Input Cache"
workstream: "0038"
kind: task
depends_on:
  - pinned-coverage-fuzzing-tool
  - daemon-process-chaos
  - model-context-protocol-fuzzing
  - minimum-rust-and-dependency-gates
  - owner-confirmed-github-automation
  - owner-confirmed-native-evidence-trust
  - owner-supplied-release-metadata
gated: false
touches:
  - support/release-input-cache.toml
  - schemas/release/locked-source-cache.schema.json
  - scripts/prepare_locked_source_cache
  - scripts/verify_locked_source_cache
  - .github/workflows/release.yml
  - crates/slingshot-development/src/release_input_cache.rs
  - crates/slingshot-development/src/lib.rs
  - crates/slingshot-development/src/main.rs
  - crates/slingshot-development/tests/release_input_cache.rs
  - "crates/slingshot-development/tests/fixtures/release-input-cache/**"
status: planned
merged_as: ""
---
# Release Input Cache

Cold native artifact builds and isolated aggregate acceptance need a verified input producer before either consumer can run. This task owns that network-enabled preparation boundary and the complete row-role cache set; neither consumer may repair or supplement it.

**Steps:**

1. Commit `support/release-input-cache.toml`, the cache schema, and fixtures for every owner-mapped native row plus the coordinator. The role table declares complete inputs and requires one same-release-run protected-environment RustSec owner-review record binding exact source commit/tree, exact RustSec origin/full-commit/tree, authority/reviewer-policy digest, and provider workflow/run/attempt identity. It contains no author timestamp or reusable freshness flag. Give the complete canonical bytes and SHA-256 sidecars of `slingshot.daemon-runtime-contract/1` and `slingshot.author-agent-transport-contract/1` separate required roles in every member, with exact/missing/extra/mutated/role-swapped fixtures. Exactly the selected FSM native row and the coordinator include one independently addressed `plan_0008_cargo_home_seed` projection and its record; every other row rejects it.
2. Implement the network-enabled preparation command from a new empty root. It requires explicit verified Slingshot/FSM/RustSec trees, coverage bundle, and `--rustsec-owner-review-record <same-run-record>`; verifies the protected authority, exact source/pin/run binding before admission; then fetches only immutable declared inputs. A prior-run/copied/mismatched/self-timestamped record, ambient cache, installed tool, mutable reference, or existing output fails.
3. Produce a bounded canonical cache set with one independently addressed member for each native row and one coordinator member. Every native member contains the exact target's frozen Cargo source closure—including the canonical command-limits manifest, exact `schemas/command-canonical-json-1.json` and independently authored vectors, all exact command argument/result schemas, separately tagged limits/canonical-contract/role-schema digests, independent conformance/parity fixtures, and the two canonical runtime/transport manifests, sidecars, typed-parser provenance, and distinct format/digest records—pinned Rust/Cargo components, repository tools, and target-specific inputs required by `build-release-artifacts`; the selected FSM row additionally contains the pinned FSM closure. The coordinator member contains that same product/fuzz/command-contract/runtime/transport closure, the already verified coverage-fuzzing bundle and its source/binary manifest, pinned FSM and repository-tool inputs, coordinator standard library, and digest-pinned OCI image layout. From the one exact pinned-FSM lock/source closure, project an ordinary-file/directory-only Cargo-home tree independently into the selected FSM row and coordinator. Consume the landed `slingshot.finite-state-machine-compatibility/1` manifest through Plan 0008's verifier as the sole authority for file/directory/component/path/depth/per-file/aggregate limits and deterministic first-fault order; do not copy its values into a Plan 0009 limit source. Record the complete compatibility-manifest digest, canonical seed-tree digest, file and directory counts, aggregate file bytes, FSM source/commit/tree/lock digest, and cache-entry identities in each member's existing manifest. Host-provided linker/system-root or software-development-kit bytes are not mislabeled as cached: their exact observed identities remain validated native-row inputs.
4. Emit one schema-valid canonical manifest binding the owner-review record digest and provider run identity, exact Slingshot/FSM/RustSec trees, every lock/pin, role/row, cache entry, toolchain, coverage provenance, OCI digest, and bounds without an author-provided freshness time. No member relies on another or an ambient cache.
5. Implement the bounded offline verifier and `scripts/verify_locked_source_cache`. Before any consumer starts, hash each complete runtime/transport manifest, validate its canonical parser and sidecar, compare repository/embedded/cache/report digests under distinct roles, and reject independent byte/order/newline/sidecar/role mutations without fallback. It also rejects missing, extra, duplicate, noncanonical, path-escaping, linked, sparse, device, special, over-bound, writable, mutable, wrong-role, wrong-lock/source/checksum/toolchain/tool/coverage-bundle/image, host-cache-derived content, or any command-limits/canonical-JSON-contract/schema/conformance digest disagreement or role substitution without fetching, repairing, updating, installing, or consulting an ambient cache. For each permitted Plan 0008 projection, invoke the shared seed verifier against the exact landed compatibility manifest; independently compare manifest/tree/count/aggregate/source/lock fields, verify exact and next-unit boundaries for every Plan 0008 dimension plus same-path precedence, and reject a projection on an unselected row. The verifier never repairs, supplements, normalizes, or derives an alternate Cargo home. Independent fixtures mutate only the canonical contract while limits and schemas stay fixed, only limits while canonical contract and schemas stay fixed, and each role schema while all other roles stay fixed.
6. Add a network-enabled release-workflow preparation job for each exact native row and the exact coordinator role. Each job starts with empty Cargo/Rust/cache roots, prepares and verifies its member, and transfers only the addressed canonical cache plus manifest. Downstream tasks receive cache paths and digests explicitly. Both `release-artifact-contract` and `release-acceptance-matrix` depend directly on this task to serialize their shared workflow/command modules; the artifact task consumes the matching native member, while acceptance is additionally ordered after authenticated artifacts and consumes only the coordinator member.

**Tests:**

- A fresh same-release-run owner approval plus empty-home preparation produces every exact native member and coordinator; a cold network-denied frozen/offline probe resolves solely from each member.
- Missing, copied, prior-run, wrong-source/pin/environment-policy, self-timestamped, or reusable RustSec review evidence fails before acquisition; every member binds the one accepted record digest.
- Missing, extra, duplicate, reordered, source-substituted, checksum-mismatched, mutable, ambient-cache-derived, wrong-row/role, over-bound, special, escaping, or writable content fails before a consumer starts.
- The selected native row alone contains the exact FSM closure. The coordinator alone contains the OCI layout and verified coverage-fuzzing bundle; its manifest binds the cargo-fuzz source/tree/lock/toolchain/two-build executable digest and an empty-PATH offline version probe.
- The selected FSM row and coordinator each contain exactly one byte-identical, independently addressed Plan 0008 Cargo-home seed projection bound to the exact compatibility-manifest and pinned FSM source/tree/lock digests. Empty/exact/next file, directory, component, relative-path, depth, per-file, and aggregate boundaries plus links/reparse points, special files, destination aliases, path escapes, mutation, missing/extra content, wrong role, and deterministic same-path precedence produce the Plan 0008 verifier's exact outcome without proportional over-bound copy or ambient lookup.
- Toolchain, linker/system-root or software-development-kit, source-lock, platform-row, repository-tool, RustSec, FSM, coverage-tool, and OCI-image identities have distinct fields; no host SDK is silently copied into or inferred from a Cargo cache.
- Limits-manifest, canonical-JSON-contract, canonical-vector, argument-schema, result-schema, and conformance/parity identities have distinct roles; independent one-bit drift or role-swapped bytes fail offline verification before either release consumer can run a command.
- Daemon-runtime and author-agent-transport manifest bytes, sidecars, formats, typed values, and digests have distinct authenticated roles; a one-byte, ordering, newline, sidecar, embedded-value, report, or cross-role mismatch fails before build or acceptance.
- Repeating preparation for the same exact revision and declared remote bytes yields identical digest-bearing cache content and manifests, while a changed lock, pin, row, tool, source checksum, or image changes or invalidates the corresponding member.

- **Done when:** `cargo test -p slingshot-development --test release_input_cache`, one protected same-run `scripts/prepare_locked_source_cache --finite-state-machine-source <verified-path> --rustsec-advisory-database <verified-path> --rustsec-owner-review-record <verified-same-run-record> --coverage-fuzzing-tool-bundle <verified-bundle> --output-directory <new-cache-set>` preparation, and network-denied verification authenticate both runtime/transport manifests and digests, prove exact owner-reviewed RustSec binding and complete offline members, and prove byte-identical selected-row/coordinator Plan-0008-limit-conforming seed projections with verified manifest/tree/source/lock records before either release consumer becomes ready.
