---
id: daemon-runtime-documentation
title: "Daemon Runtime Documentation"
workstream: "0018"
kind: task
depends_on:
  - local-operation-golden-session
gated: false
touches:
  - README.md
  - ARCHITECTURE.md
  - docs/DAEMON.md
  - crates/slingshot-development/tests/product_documentation.rs
status: done
merged_as: "749e5fa38b5f0e1ac8b5c6c4930147988cc38f56"
---
# Daemon Runtime Documentation

The product documentation must describe the product daemon that process/recovery suites prove without presenting the internal development test-daemon subcommand or fake executor as a product capability.

**Steps:**

1. Write documentation assertions first for `support/foundation-contract.toml` as the sole inherited value source and `policy/daemon-runtime-contract-1.json` plus digest as the sole Plan-0004 value source, retained `daemon.ping` and current-nonce `daemon.stop` versus operation versions/digests, stale-nonce replacement safety, process identifiers as diagnostics only, current-user endpoints and mandatory Windows `PIPE_REJECT_REMOTE_CLIENTS`, cooperative or retained-stable-handle test cleanup, complete opaque target/revision matching with no downstream preimage fields, sampled-account `~/.config/slingshot` resolution, same-principal rotation versus changed-principal pre-network recovery refusal, slow-client read/write deadlines and wait exemption, exact WAL-header/frame arithmetic and database/WAL/SHM/reader/checkpoint/backpressure behavior, pinned memory-only SQLite transient policy, restrictive VFS/closed SQL inventory and exact whitelisted objects, conditional terminal failure payloads, operation states/recovery/durable resume receipts, name-only process namespace versus target-partitioned durable keys, installation identity, secure diagnostics, bounded list/complete maintenance manifests/application receipts, operation-free target-qualified maintenance-result identity/association/retention/metadata/read, the exact lookup-to-start ownership-transition rule, same-handle prefix-authenticated result resumption, restart refusal, and explicit local-only scope.
2. Add the current daemon usage and automatic-start behavior to `README.md` without documenting commands that are not executable.
3. Update `ARCHITECTURE.md` with crate boundaries, one owner per name pair, fail-closed installation/target audits, target-partitioned lifecycle, stable control/versioned operations, unavailable product executor, and artifact/result data flow.
4. Create `docs/DAEMON.md` with control hello/`daemon.ping`/status/current-nonce `daemon.stop`, stale-instance refusal, diagnostic-only process identifiers, Windows remote-client rejection, identity/revision mismatch guidance, manifest-derived slow-client deadline behavior, unavailable execute, list/status/wait/result, durable recovery-resume replay lifetime, operation artifact, complete maintenance preview and durable apply-receipt/result contracts including target-and-identifier-only operation-free metadata, digest-bound read, the sole apply ownership transition, supersession/retirement, secure state/diagnostics, and explicit stop/start configuration snapshots.
5. State factually that the product executor is unavailable and makes no remote connection; keep internal development test-daemon mechanics out of product capability prose.

**Tests:**

- Documented command and response examples parse through the production local protocol types.
- Every operation state and stable error named by the daemon appears exactly as implemented.
- Documentation states one daemon per profile/environment, operating-system-account configuration-root resolution, complete opaque target partitioning with no downstream principal fields, same-principal secret-rotation compatibility, changed-principal nonterminal startup refusal, installation fail-closed behavior, target-partitioned history, conditional terminal failure evidence, checksum verification, explicit bounded terminal maintenance, operation-free result metadata/read associations and receipt/result retirement, and recoverable nonterminal stop behavior.
- Product prose passes the temporal/documentation policy while `docs/plans` remains outside that policy scan.

- **Done when:** `cargo test -p slingshot-development --test product_documentation` validates `README.md`, `ARCHITECTURE.md`, and `docs/DAEMON.md` against stable control, conditional terminal evidence, durable resume/maintenance replay and operation-free result metadata/reads, target/revision and persistence contracts, unavailable product execution, bounded manifests/artifacts, and present-state prose rules, and all workspace test, clippy, formatting, and rustdoc gates succeed.
