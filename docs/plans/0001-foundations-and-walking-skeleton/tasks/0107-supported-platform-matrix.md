---
id: supported-platform-matrix
title: "Supported Platform Matrix"
workstream: "0001"
kind: task
depends_on:
  - workspace-capability-inventory
gated: false
touches:
  - support/platforms.toml
  - crates/slingshot-development/src/supported_platform_matrix.rs
  - crates/slingshot-development/tests/supported_platform_matrix.rs
  - "crates/slingshot-development/tests/fixtures/supported-platform-matrix/**"
status: planned
merged_as: ""
---
# Supported Platform Matrix

One exact abstract row per supported target binds required capabilities and artifact layout without inventing a native machine, linker, software-development kit, provider, or aggregate evidence set. Plan 0001 evaluates every row through deterministic policy fakes and may observe only the row matching the current environment; Plan 0009 owner-gates the concrete native mappings.

**Steps:**

1. Commit accepted and rejected exact-row fixtures for triples, abstract host/architecture requirements, capability identifiers, release executable/suffix/archive/smoke layout, duplicates, placeholders, concrete runner/build-image/linker/software-development-kit values, and family-only fallbacks.
2. Create `support/platforms.toml` with exactly `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, and `x86_64-pc-windows-msvc`. Each row names only its exact target triple, required native operating-system/architecture, and closed capability identifiers, including provider-record server-authentication trust decisions that distinguish unconditional permission from distrust, purpose/external restriction, unevaluable state, and conflicting same-DER records. Unix rows require owner-only runtime directories, Unix domain sockets, distinct advisory owner/election locks, peer-current-user policy, atomic same-directory readiness, session-independent detachment, and stable supervised-child cleanup. The Windows row requires current-user Security-Identifier/access-control-list enforcement, named pipes created with `PIPE_REJECT_REMOTE_CLIENTS`, distinct owner/election locks, atomic readiness, detached creation, and stable supervised-child cleanup.
3. Pin artifact layout only: executable stem `slingshot`; empty suffix plus deterministic Rust-packager `tar.gz` profile for Linux/macOS; `.exe` plus deterministic Rust-packager `zip` profile for Windows; native smoke mode `direct`; canonical flat membership `slingshot[.exe]`, `LICENSE`, and `SHA256SUMS`; and normalized ordering, path spelling, modes, ownership fields, and timestamps. This does not claim the packager, license, machine, or archive exists in Plan 0001.
4. Define deterministic build-policy requirements as capability identifiers—exact repository Rust toolchain/target, incremental disabled, closed environment, source/build-root remapping, source-date derived from the source object, native linker/system-root or software-development-kit observation, and no ambient archive program—without supplying concrete linker, image, system-root, environment, or runner values. Deterministic policy fakes validate every row. Plan 0009's owner gate alone maps these requirements to exact observed native inputs and proves two builds/archive bytes.
5. Parse and validate the abstract manifest through one development API consumed unchanged by local quality and Plan 0009. Current-native observation accepts only the row matching the running target/host/architecture and emits an untrusted report; a concrete provider selector, observed image, linker digest, software-development-kit digest, cross-compile result, family label, or aggregate success field is invalid in this Plan 0001 manifest.

**Tests:**

- The manifest contains the three exact abstract rows once, and every row has complete provider-trust-decision, runtime, filesystem, build-policy, and artifact-layout capability identifiers with no concrete provider/build-environment value, placeholder, or family fallback.
- Linux/macOS expose `slingshot`, empty suffix, `tar.gz`, and `direct`; Windows exposes `slingshot`, `.exe`, `zip`, and `direct` exactly.
- The Windows capability set requires `PIPE_REJECT_REMOTE_CLIENTS`; removing it or substituting access-control-list-only remote protection fails the row fixture.
- A native invocation refuses a nonmatching target/host/architecture, and deterministic fakes reject missing source remap, source-date, closed-environment, linker/system-root observation, archive profile, or stable supervision capability. The result is policy validation, not multi-host evidence or executable/archive reproducibility.
- Concrete provider selector/image, mutable machine label, linker/system-root digest, self-authored aggregate report, or copied per-row success is rejected as unprovisioned Plan 0009-owned data.
- The target set equals every target-conditioned dependency row and a cross-compile-only result cannot mark a row supported.

- **Done when:** `cargo test -p slingshot-development --test supported_platform_matrix` proves all three abstract capability/artifact-layout rows and deterministic policy fakes, while current-native observation accepts at most its one exact row and every concrete provider/build-image/linker/system-root, cross-compile, copied, or aggregate proof claim is refused for Plan 0009 to supply later.
