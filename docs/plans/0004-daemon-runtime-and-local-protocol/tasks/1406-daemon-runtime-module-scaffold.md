---
id: daemon-runtime-module-scaffold
title: "Daemon Runtime Module Scaffold"
workstream: "0014"
kind: chore
depends_on: []
gated: false
touches:
  - crates/slingshot-domain/src/lib.rs
  - crates/slingshot-domain/src/daemon_runtime_contract.rs
  - crates/slingshot-domain/src/operation.rs
  - crates/slingshot-domain/src/command_fingerprint.rs
  - crates/slingshot-domain/src/installation.rs
  - crates/slingshot-domain/src/persistent_capacity.rs
  - crates/slingshot-domain/src/operation_executor.rs
  - crates/slingshot-local-protocol/src/lib.rs
  - crates/slingshot-local-protocol/src/control.rs
  - crates/slingshot-local-protocol/src/message.rs
  - crates/slingshot-storage/src/lib.rs
  - crates/slingshot-storage/src/database.rs
  - crates/slingshot-storage/src/sqlite_statement_inventory.rs
  - crates/slingshot-storage/src/sqlite_vfs.rs
  - crates/slingshot-storage/src/operation/listing.rs
  - crates/slingshot-storage/src/operation/mod.rs
  - crates/slingshot-storage/src/operation_repository.rs
  - crates/slingshot-storage/src/artifact_store.rs
  - crates/slingshot-storage/src/installation_state.rs
  - crates/slingshot-storage/src/persistent_capacity.rs
  - crates/slingshot-storage/src/maintenance.rs
  - crates/slingshot-daemon/src/lib.rs
  - crates/slingshot-daemon/src/startup.rs
  - crates/slingshot-daemon/src/diagnostics.rs
  - crates/slingshot-daemon/src/unavailable_operation_executor.rs
  - crates/slingshot-daemon/src/operation_scheduler.rs
  - crates/slingshot-daemon/src/operation_submission.rs
  - crates/slingshot-daemon/src/operation_queries.rs
  - crates/slingshot-daemon/src/operation_wait.rs
  - crates/slingshot-daemon/src/request_dispatch.rs
  - crates/slingshot-daemon/src/shutdown.rs
  - crates/slingshot-daemon/src/artifact_transfer.rs
  - crates/slingshot-daemon/src/operation_maintenance.rs
  - crates/slingshot-daemon/src/operation_recovery.rs
  - crates/slingshot-command-line/src/lib.rs
  - crates/slingshot-command-line/src/daemon_process.rs
  - crates/slingshot-test-support/src/lib.rs
  - crates/slingshot-test-support/src/fake_operation_executor.rs
  - crates/slingshot-test-support/src/operation_fault_injection.rs
  - crates/slingshot-test-support/src/process_barrier.rs
  - crates/slingshot-development/src/lib.rs
  - crates/slingshot-development/src/slingshot_test_daemon.rs
  - crates/slingshot-development/src/test_daemon_faults.rs
  - crates/slingshot-development/tests/daemon_runtime_module_scaffold.rs
  - "crates/slingshot-development/tests/fixtures/daemon-runtime-module-scaffold/**"
  - crates/slingshot-development/tests/fixtures/workspace-module-map/module-ownership.txt
status: done
merged_as: "8d86a5f0792c4f16f2ba80175f9d39f7f661db96"
---
# Daemon Runtime Module Scaffold

Adopt the Plan 0001 crate roots and register every genuinely new Plan 0004 library leaf once so each later feature task begins from a compiling declared module.

**Steps:**

1. Commit an independently ordered fixture that classifies the complete Plan 0004 production, support, and development source inventory as an adopted Plan 0001 root, an adopted Plan 0001 behavioral or process-entry leaf, or a new Plan 0004 library leaf. Map every new library leaf to its owning crate, parent declaration, and descendant feature task before changing a module declaration.
2. Adopt the existing `lib.rs` roots for domain, local protocol, storage, daemon, command line, test support, and development. Preserve their Plan 0001 declarations and behavior while adding exactly one declaration for each new library leaf in this task's footprint. Do not edit or recreate the Plan 0001 behavioral leaves `framing.rs`, `runtime_namespace.rs`, `ownership.rs`, `local_server.rs`, and `daemon_connection.rs`, or the existing `slingshot-development` process entry in `main.rs`.
3. Create only the genuinely new library leaves as compiling documentation-only structural modules, including `slingshot_test_daemon.rs` under the development library root. Each new module document states its present architectural ownership and contains no feature implementation, placeholder behavior, public type, constant, endpoint, planning marker, or prospective claim. Create no `src/bin` entry and no Cargo target; the workspace retains only the Plan 0001 `slingshot` and `slingshot-development` binaries.
4. Add a structural test that compares the independent fixture, parent declarations, physical source files, this task's exact library-source footprint, and every descendant Plan 0004 task's source footprint in both directions. Require a bijection among the fixture's new-library entries, new parent declarations, new library source files, scaffold-owned new-library footprint entries, and descendant-owned library leaves. Require every adopted behavioral or process-entry leaf to be classified exactly once but excluded from the scaffold-created leaf set. Reject a missing, additional, duplicate, wildcard-only, unreachable, or misowned leaf; a parent edit owned by a feature task solely for registration; an unregistered descendant source; any graph root other than this scaffold; overlapping feature source footprints without an ancestor relationship; or any workspace binary target beyond the exact two inherited from Plan 0001.
5. Assert that Plan 0001 owns the crate manifests, roots, existing behavioral leaves, and dependency directions. Assert that `daemon-runtime-contract` depends directly on this scaffold and every other Plan 0004 task reaches it transitively. Run workspace compilation, documentation warnings, source policy, and semantic documentation review over the scaffold.

**Tests:**

- Every new Plan 0004 library leaf exists, is reachable through exactly one declaration from its owning crate root, and byte-matches its independently ordered ownership fixture entry.
- The fixture, declaration inventory, source-file inventory, scaffold source footprint, and union of descendant new-library footprints contain the same complete new-library set and owning crate.
- Every Plan 0001 behavioral or process-entry leaf used by Plan 0004 remains registered through its existing parent or target, is classified as adopted rather than scaffold-created, and has at least one descendant owner. The new test-daemon code is a declared development-library leaf rather than a Cargo target.
- Parent modules contain no Plan 0004 behavioral implementation, and newly created leaf modules contain only accurate present-state structural documentation until their owning descendant task implements them.
- The fixture rejects an undeclared source file, a declaration without a file, a source in the wrong crate, a second parent for one leaf, a wildcard in place of an exact leaf, an unregistered descendant task, another graph root, an unordered overlapping source footprint, or a third workspace binary.
- No scaffold-created source contains placeholder behavior, planning language, undocumented exported items, an external dependency use, a feature-specific constant, or a change to a Plan 0001 manifest or dependency direction.

- **Done when:** `cargo test -p slingshot-development --test daemon_runtime_module_scaffold && cargo check --locked --workspace --all-targets && RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` proves the exact new-library declaration/source/fixture/footprint bijection, the classified adoption of Plan 0001 roots and behavioral/process-entry leaves, exactly the inherited two workspace binaries, and a single dependency root for all Plan 0004 work.
