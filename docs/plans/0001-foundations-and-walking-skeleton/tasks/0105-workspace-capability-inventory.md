---
id: workspace-capability-inventory
title: "Workspace Capability Inventory"
workstream: "0001"
kind: task
depends_on:
  - workspace-module-map
gated: false
touches:
  - Cargo.toml
  - Cargo.lock
  - policy/workspace-capabilities.toml
  - crates/*/Cargo.toml
  - crates/slingshot-development/tests/workspace_capability_inventory.rs
  - "crates/slingshot-development/tests/fixtures/workspace-capability-inventory/**"
status: done
merged_as: "a7061633e8dad8db834070c7f6eb2ec44be24060"
---
# Workspace Capability Inventory

The initial dependency graph is one reviewable candidate contract: every structural module family and explicitly named planned capability consumer maps to an exact capability and dependency kind before native probes confirm that the selected packages expose the required interfaces.

**Steps:**

1. Author an independent structural-family/planned-consumer-to-capability fixture, including explicit standard-library and empty build-dependency entries, without duplicating feature-owned leaf inventories.
2. Define `policy/workspace-capabilities.toml` with one exact candidate registry package/version, feature set, default-feature choice, target condition, owning crate, and normal/build/development dependency kind for every non-standard capability.
3. Cover typed errors/serialization/TOML/JavaScript Object Notation, URLs/identifiers/digests/Base64/secret buffers, asynchronous runtime/cancellation/bytes, and a Hypertext Transfer Protocol client capable of HTTP/1.1-or-HTTP/2-only negotiation, disabled redirect/proxy/decompression/protocol migration, separately controlled name-resolution/connection and Transport Layer Security phases, incremental pre-collection header/count/aggregate/compression bounds, every informational/final head, actual trailer-section presence including empty, and unambiguous framing errors. Cover certificates/signed assertions, client construction from explicitly supplied immutable root stores with no ambient/default-root merge, separately supplied identity-management and author root stores, and supported-platform server-authentication root enumeration that safely exposes each provider record's DER plus effective trust-purpose, distrust/deny, application/policy/name restriction, and evaluability decision needed to reject lossy conversion. Also cover bundled SQLite, command-line/diagnostics/schemas, platform directories/locks/endpoints/detachment/stable child supervision, descriptor-relative no-follow file traversal and identity, Linux descriptor-bound POSIX access-control lists, macOS descriptor-bound extended access-control lists, Windows process-token Security Identifiers/security descriptors/discretionary access-control lists/reparse/128-bit file identity plus named-pipe creation with the external `PIPE_REJECT_REMOTE_CLIENTS` flag, deterministic tar/gzip/zip archive codecs and checksum primitives assigned to the later `slingshot-development` Rust packager, Rust/shell/workflow/Structured Query Language parsing, fake-author service, temporary files, property tests, and process assertions. Every native-filesystem/process/trust candidate must expose a safe public Rust API compatible with inherited `unsafe_code = "forbid"`. The archive rows select and probe dependency APIs only; Plan 0009 owns packager behavior and release evidence.
4. Centralize every candidate registry declaration under workspace dependencies, let members select only workspace entries, update the committed lockfile, and keep each dependency kind/target exactly equal to the inventory.
5. Reject unused inventory entries, module consumers without a capability, duplicated version/feature policy, a package minimum Rust version above 1.98.0, and any normal/build support-crate edge into product.

**Tests:**

- Every structural family and explicitly named planned capability consumer maps to exactly one or more required capability rows or an explicit standard-library row; no inventory row lacks a consumer, and no row claims authority over a feature plan's exact leaf list.
- Cargo metadata and lockfile match every candidate package, exact version, feature/default-feature, target condition, and dependency kind in the inventory.
- The three target-conditioned dependency sets name exactly `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, and `x86_64-pc-windows-msvc`; the later supported-platform task must match this closed set rather than add a family fallback.
- Each Plan 0002 transport, route-separated provider-trust, and configuration-filesystem capability has one exact consumer row and safe-API candidate. A client that hides informational/trailer sections, bounds only after collection, implicitly merges default roots, cannot keep an additional author root out of identity-management trust, exposes only a certificate list without trust decisions, or lacks descriptor-bound access-control-list, reparse, Security-Identifier, stable file-identity, stable supervised-child, or Windows `PIPE_REJECT_REMOTE_CLIENTS` creation capability fails inventory coverage rather than deferring an unplanned manifest change.
- Fixtures reject missing/extra dependencies, misplaced build/development dependencies, feature drift, duplicated policy, unsupported minimum Rust version, and forbidden local-package direction.

- **Done when:** `cargo test -p slingshot-development --test workspace_capability_inventory && cargo metadata --locked --format-version 1 --no-deps` proves every structural family and planned capability consumer has one exact centralized candidate dependency contract, without a duplicate feature-leaf inventory, and authorizes only `workspace-capability-probes` to correct that contract before it becomes the retained baseline.
