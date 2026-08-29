---
id: workspace-capability-probes
title: "Workspace Capability Probes"
workstream: "0001"
kind: task
depends_on:
  - supported-platform-matrix
  - workspace-capability-inventory
gated: false
touches:
  - Cargo.toml
  - Cargo.lock
  - policy/workspace-capabilities.toml
  - "crates/*/Cargo.toml"
  - "crates/*/tests/workspace-capabilities/**"
  - crates/slingshot-development/tests/workspace_capability_probes.rs
  - "crates/slingshot-development/tests/fixtures/workspace-capability-probes/**"
status: done
merged_as: "329c37209e48d127d4dce3511e8175bf83af9298"
---
# Workspace Capability Probes

Compile probes independently prove and then freeze the exact dependency selection that exposes every required interface and target behavior with the pinned compiler and features.

**Steps:**

1. Author a probe-coverage fixture mapping every inventory row to an owning crate, dependency kind, required public API, feature assertion, and applicable exact platform rows.
2. Add a minimal probe under the owning crate and dependency kind for every non-standard candidate capability; exercise the required public behavior rather than merely importing the package. The client probe negotiates only HTTP/1.1/HTTP/2, disables ambient behaviors, exposes each 100/103/final head and absent/empty/nonempty trailer distinction, enforces decoded/compression bounds before collection, rejects ambiguous framing, and permits separate injected name-resolution/connection and Transport Layer Security deadlines. Native trust probes enumerate provider records and distinguish unconditional server-authentication permission from distrust/deny, another purpose, external application/policy/name constraints, unevaluable settings, and same-DER conflicting decisions without reducing those cases to an undifferentiated certificate list. They also build explicit immutable platform-only identity-management and platform-plus-additional author stores with no ambient/default-root merge and prove that a root added only to author cannot authenticate identity management. Native configuration-filesystem probes open a descendant relative to a directory descriptor/handle without following links and obtain identity/access-control evidence from that same object: Linux proves masked named/default POSIX access-control-list inspection, macOS proves extended access-control-list inspection, and Windows proves sampled process-token user/LocalSystem/BUILTIN\Administrators Security Identifiers, security-descriptor/discretionary-access-control-list flags and entries, reparse evidence, link count, volume serial number, and 128-bit file identifier. Each probe uses only safe public Rust APIs under inherited `unsafe_code = "forbid"`. The `slingshot-development` archive probes write and read the selected deterministic tar/gzip and zip profiles, verify checksum primitives over fixed bytes, reject trailing/corrupt input, and make no release-artifact claim.
3. Compile with Rust 1.98.0 and each candidate's exact default-feature/feature set, target-gating Unix and Windows capabilities without fake cross-platform imports. If a candidate fails its declared interface, target, or minimum-compiler proof, replace it with one exact reviewed candidate and update the inventory, centralized/member manifests, and lockfile together before rerunning all affected probes.
4. Run target-independent probes plus target-conditioned deterministic policy fakes on the current environment. Run real target-specific behavior only when exactly one abstract platform row matches that current native target/host/architecture; a cross-check may compile another target but cannot satisfy native behavior. Emit at most one `untrusted_current_native_observation`. Plan 0009 later maps the same commands to owner-confirmed native jobs for every row and authenticates each result.
5. Freeze the dependency selection after every inventory row has compile/API coverage, every platform policy fake passes, and any current matching native row passes its real probe. Reject an uncovered inventory row, probe without a consumer, wrong dependency kind, undeclared correction, feature/default-feature drift, unsupported compiler, mismatched current row, copied remote report, or aggregate-success claim. Any later intentional dependency change must update the inventory, manifests, lockfile, probes, and dependency-policy evidence in the task that owns that change.

**Tests:**

- Every retained inventory row has exactly the required probe coverage and every probe exercises the named public API.
- Disabling any required feature, enabling an undeclared default feature, moving a probe to another dependency kind, or changing a target predicate fails its fixture.
- Every exact row passes deterministic API/policy fixtures; only the row matching the current environment compiles and runs its native probes with the pinned toolchain, and no Plan 0001 invocation combines copied hosts into an all-row result.
- The current matching row executes its provider-trust-decision, explicit route-separated root-store, configuration-file handle/access-control, and stable-supervision probes against real native fixtures; on Windows it also proves every named-pipe constructor carries `PIPE_REJECT_REMOTE_CLIENTS`. A certificate-only trust list that cannot prove effective server-authentication eligibility, a client builder that merges ambient roots or accepts the author-only root on identity management, a path-reopen substitute, path-only access-control query, missing Windows 128-bit file identity/remote-client flag, PID-only termination, or repository-owned unsafe wrapper fails the probe.
- A failed candidate cannot be waived: fixtures require one atomic candidate replacement across inventory, manifests, lockfile, and affected probes while forbidding changes to the module map or supported-platform rows.
- Report fixtures permit zero or one current-native observation, label it untrusted, and reject a cross-compile, nonmatching, duplicated, authority-labelled, or all-row evidence set.

- **Done when:** `cargo test -p slingshot-development --test workspace_capability_probes` proves every retained dependency through compile/API and deterministic row-policy coverage, runs real behavior only for the row matching the current native environment, emits at most one untrusted report, and keeps manifests/lockfile synchronized without an unsupported, copied, or aggregate native claim.
