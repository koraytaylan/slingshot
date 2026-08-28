---
id: secure-daemon-diagnostics
title: "Secure Daemon Diagnostics"
workstream: "0016"
kind: task
depends_on:
  - persistent-capacity-accounting
  - target-runtime-namespace
gated: false
touches:
  - crates/slingshot-daemon/src/diagnostics.rs
  - crates/slingshot-daemon/tests/diagnostics.rs
  - "crates/slingshot-daemon/tests/fixtures/diagnostics/**"
status: planned
merged_as: ""
---
# Secure Daemon Diagnostics

A detached daemon needs bounded operator evidence without leaking secrets into command output or growing persistent state without limit.

**Steps:**

1. Author redaction, exact-bound, rotation, restart, symlink, ownership, permission, interrupted-rotation, standard-output, and standard-error fixtures before configuring the sink.
2. Define named record, file-byte, retained-file-count, derived total-diagnostic-storage, and diagnostic-field bounds and reject or truncate through typed policies before formatting; keep that total outside operation/artifact capacity and its filesystem safety reserve.
3. Create and rotate files below the secure persistent state root with current-user access, verified handles, same-directory atomic replacement, synchronization, and no link following.
4. Redact credentials, authorization values, command secret properties, assertion material, tokens, internal request bodies, and filesystem source paths before any record reaches the sink.
5. Expose only bounded sink health through daemon status and keep detached diagnostics out of ordinary command and Model Context Protocol standard streams.

**Tests:**

- Exact-bound records and files are retained, over-bound values follow the declared bounded policy, and rotation never exceeds the named file count.
- Active plus rotated diagnostics never exceed the derived total after write, rotation, interruption, or restart and cannot consume the operation/artifact safety reserve.
- Every fixture secret and internal source path is absent from active files, rotated files, errors, status, and captured process streams.
- Symlink, wrong-owner, broad-permission, and interrupted-rotation fixtures fail closed without overwriting another object.
- Restart continues the same bounded rotation sequence and status reports sink health without revealing a path.

- **Done when:** `cargo test -p slingshot-daemon --test diagnostics` proves bounded rotation, verified current-user storage, complete fixture-secret redaction, stream isolation, and deterministic recovery at every rotation boundary, and all workspace gates succeed.
