---
id: terminal-operation-maintenance
title: "Terminal Operation Maintenance"
workstream: "0017"
kind: task
depends_on:
  - list-operations
  - operation-status-and-result
  - checksum-verified-artifact-store
  - persistent-capacity-accounting
gated: false
touches:
  - "crates/slingshot-daemon/tests/fixtures/operation-maintenance/**"
  - crates/slingshot-daemon/src/operation_maintenance.rs
  - crates/slingshot-daemon/tests/operation_maintenance.rs
  - crates/slingshot-storage/src/maintenance.rs
  - crates/slingshot-storage/src/sqlite_statement_inventory.rs
status: done
merged_as: "7072f6bf8ea972e2c989f5dbe7f9d4e88bf2fb67"
---
# Terminal Operation Maintenance

Persistent history needs explicit bounded retention without ever pruning recoverable work or leaving a successful operation that names missing bytes.

**Steps:**

1. Author no-op, terminal selection, nonterminal exclusion, complete child-row effects, multiple recovery-resume receipts, shared/referenced/unreferenced content, inline and over-inline preview/application results, exact-repeat and different-preview supersession, stale/tampered preview, metadata lookup before/after ownership transfer, read-start/transfer ordering, interrupted result association/blob delete, old target, applied-receipt replay/restart, prior-receipt/result retirement, receipt/association-ledger exhaustion, and every manifest/envelope bound fixture first.
2. Define a versioned canonical `TerminalMaintenanceManifest`: required `before` applies to terminal-settlement time and completed prior-receipt commit time respectively; requested `limit` bounds whole terminal operations; a named independent maximum bounds deterministic oldest-first completed-receipt retirements; and the manifest contains selected target digest, deterministic operation identifiers/revisions, exact operation/progress/recovery/recovery-resume-receipt/result/artifact-association/other child-row logical keys and revisions, exact artifact digest/length/reference action, exact completed prior-receipt retirements, and before/projected/released operation-row/receipt-row/association-row/artifact-byte capacity facts.
3. Consume the exact operation/effect/receipt/maintenance-result-association/retirement/manifest bounds from `DaemonRuntimeContract`. Select stable whole-operation groups without splitting their effects, require one maximum-shape operation to fit the committed manifest bound, digest the complete canonical manifest, and make candidate selection mutation-free. If the canonical preview result fits the inline-machine bound return it inline; otherwise reserve/install it and atomically create the derived `0x00` target-qualified association before returning its descriptor. Exact preview repeats reuse it; a different successful preview atomically replaces the target's one current-preview association and releases only an unshared/unowned prior blob, so the superseded digest is no longer readable or applicable. Capacity refusal returns no preview or association.
4. On apply, require the same selected target and preview digest. Look up a durable target-qualified application receipt before re-reading deleted candidates; return a completed receipt and its exact inline/result association identities as replay, or resume only a database-applied receipt's listed unreferenced-blob cleanup. For a fresh digest, require the current matching preview association when the reviewed preview was external, every manifest fact/projection to remain current, and room for the new receipt and any result association after exact completed-receipt retirements. Transfer that preview association from current-preview to the same application receipt at the checked next association revision; exact apply replay never advances it again.
5. In one full-synchronization transaction, delete only manifest-listed terminal operation children and artifact-slot associations, release exact row capacity, retire only listed completed prior application receipts, and insert the bounded target/digest-keyed `DatabaseApplied` receipt with conservative still-accounted blob facts. Never select/remove a nonterminal row or expose automatic pruning.
6. Never delete a blob with a remaining slot or maintenance-result reference. For each listed unreferenced blob, delete physically before a following full-synchronization transaction releases artifact capacity; after the final result, canonicalize the applied result and atomically mark the receipt `Completed` with exact actual effects plus either inline bytes or a derived `0x01` association. Interruption/failure leaves the database-applied receipt and conservative accounting for exact idempotent retry or a later preview, never a dangling reference, duplicate result, or over-credit.
7. Persist the reviewed manifest digest, canonical result digest/length, `MaintenanceResultIdentifier`, association revision, and inline-versus-associated disposition in the owning preview/application facts. Retiring a completed receipt atomically retires its preview/application result associations before releasing unshared blob references; reopen reconstructs every owner/reference and fails closed on mismatch. Order supersession, transfer, and retirement atomically against task 1708's metadata and read-start snapshots: a lifecycle transaction before read start changes or removes the returned association authority, while one after read start cannot change the already authenticated same-handle stream snapshot. Expose only bounded local preview/apply services and typed responses with author-target digest in every effect/receipt; task 1708 owns metadata/read and Plan 0006 owns the public command-line grammar/rendering.

**Tests:**

- No preview or apply selects a nonterminal row under any criteria or target partition, and every manifest contains the complete effects for each selected operation.
- Apply accepts only an unchanged exact fresh preview and transactionally removes the listed progress, recovery, resume-receipt, result, and slot facts while committing its receipt.
- Shared or still-referenced content remains; interrupted deletion leaves only an unreferenced recoverable blob and never a missing referenced blob.
- Applied maintenance releases exactly the previewed row/receipt capacity and only successfully deleted blob capacity, while preview, stale facts, shared content, and interrupted deletion cannot over-credit the namespace.
- Exact apply retry survives reopen after each row/blob/receipt/result-association phase, never repeats row release or advances a transferred association revision twice, resumes only listed unreferenced cleanup from `DatabaseApplied`, and returns the same completed target-qualified inline or associated summary after candidates are absent; wrong-target/tampered/stale/superseded digests refuse unchanged, and concurrent exact applies converge.
- Only completed prior application receipts are retired, under the same cutoff and independent named count bound, when a later manifest lists them; their owned result associations retire atomically, and ledger/257-association-boundary preview refuses before mutation unless its exact projection creates room for the new receipt/results.
- Maximum complete preview/applied documents pass their exact manifest-owned bounds, and one byte/one effect above each bound refuses without partial selection or mutation; inline results remain inline and over-inline results persist exactly under operation-free target-qualified associations without information loss.

- **Done when:** `cargo test -p slingshot-daemon --test operation_maintenance` proves complete bounded canonical preview effects/capacity, preview supersession, checked one-time association ownership/revision transfer, metadata/read-start lifecycle ordering, phased target-qualified durable apply replay/resumption with exact result identity/accounting, explicit completed-receipt/result retirement, restart reconstruction, absolute nonterminal preservation, and referential artifact safety at every interruption fixture, and all workspace gates succeed.
