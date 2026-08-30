---
id: daemon-runtime-contract
title: "Daemon Runtime Contract"
workstream: "0014"
kind: task
depends_on:
  - daemon-runtime-module-scaffold
gated: false
touches:
  - policy/daemon-runtime-contract-1.json
  - policy/daemon-runtime-contract-1.sha256
  - crates/slingshot-domain/src/daemon_runtime_contract.rs
  - crates/slingshot-domain/tests/daemon_runtime_contract.rs
  - "crates/slingshot-test-support/fixtures/daemon-runtime-contract/**"
status: done
merged_as: ""
---
# Daemon Runtime Contract

Commit the one exact machine-readable authority for every Plan-0004-owned wire, storage, scheduling, result, diagnostic, physical-database, and maintenance value before another Plan 0004 task consumes one.

**Steps:**

1. Commit canonical `policy/daemon-runtime-contract-1.json` and its exact SHA-256 sidecar with the closed format, complete values, formulas, canonical byte rules, and version-change rule from `ARCHITECTURE.md`; commit independently calculated below/exact/above and equation vectors before the typed parser, including the 32-byte WAL file header, 24-byte frame header, maximum transaction-frame growth, exact high-water, aggregate physical maximum, replacement maximum, equal filesystem reserve, the exact 257 maintenance-result-association bound `1 + 2 * 128`, and every maintenance-result identifier preimage/rendering vector.
2. Implement the read-only `DaemonRuntimeContract` parser and `DaemonRuntimeContractDigest`, embed the exact repository bytes, and reject a missing, additional, duplicate, reordered, differently encoded, differently valued, overflowing, locally defaulted, or formula-inconsistent member. Define the operation-free `MaintenanceResultIdentifier` and its exact domain-separated, target/kind/reviewed-manifest/content-digest derivation and lowercase-hex parser here so every later envelope/schema/store consumer uses one type and cannot smuggle an operation identifier or slot into it.
3. Add an inventory check that scans Plan 0004 production sources, schemas, tests, fixtures, scripts, and documentation for a copied public limit/default or a second manifest. Consumers may name typed accessors but cannot define an independently valued alias.
4. Prove that the Plan 0001 `FoundationContract`, Plan 0003 command limits, and this manifest have disjoint ownership, while the individual-artifact and physical-storage checked formulas consume their declared upstream inputs without copying them. Independently evaluate each SQLite equation from its primitive operands rather than calling or reproducing the production formula helper.
5. Expose the digest for `Hello`, every operation envelope, durable operation/recovery/maintenance provenance, and later Plan 0005/0006/0007 consumers; a digest change under operation protocol version `1` is rejected rather than negotiated as compatible.

**Tests:**

- Repository, embedded, parsed, regenerated, and sidecar bytes agree exactly, and a one-bit mutation changes the digest.
- Every exact value and checked formula in the architecture has one manifest member; an independent oracle obtains WAL bytes as `32 + frames * (24 + page_bytes)`, transaction WAL growth as `transaction_pages * (24 + page_bytes)`, then recomputes high-water, aggregate, replacement, and reserve, and independently derives 257 target associations from one current preview plus two associations per 128 retained receipts. The first next-over input, arithmetic overflow, unknown key, wrong order, alternate number spelling, or local fallback fails.
- Maintenance-result vectors cover both kind octets; one-bit target, reviewed-manifest, and content-digest changes; field reorder; missing/extra bytes; uppercase/nonhex rendering; and an oracle proving the exact 97 post-separator octets contain no operation identifier, command name, or artifact slot.
- Independent fixtures consume the manifest API or parse the committed bytes and contain no copied expected numeric value.
- A stale or missing digest remains compatible only with retained control inspection/stop and cannot enter a versioned operation or durable record.

- **Done when:** `cargo test -p slingshot-domain --test daemon_runtime_contract` proves exact bytes/digest, complete value/formula inventory, operation-free target-qualified maintenance-result identity vectors, independent boundary vectors, no ad-hoc public Plan 0004 values, and retained-control-only behavior under digest mismatch, and all workspace gates succeed.
