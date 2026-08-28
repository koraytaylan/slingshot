---
id: persistent-installation-identifier
title: "Persistent Installation Identifier"
workstream: "0015"
kind: task
depends_on:
  - daemon-runtime-contract
gated: false
touches:
  - crates/slingshot-domain/src/installation.rs
  - crates/slingshot-storage/src/installation_state.rs
  - crates/slingshot-storage/tests/installation_state.rs
  - "crates/slingshot-storage/tests/fixtures/installation-state/**"
status: planned
merged_as: ""
---
# Persistent Installation Identifier

Remote identity cannot remain stable across daemon restarts if a missing local record is silently replaced. This task defines one secure installation identity and a fail-closed registered-target ledger before operation databases copy it.

**Steps:**

1. Author empty-root, existing-record, same-target and distinct-target concurrent-first-start, registered-target, missing-record, corrupt-record, mismatched-record, symlink, ownership, permission, crash-before-publication, crash-after-publication, and replacement fixtures first.
2. Define `InstallationIdentifier` as a bounded nonsecret domain value and persist it with a versioned `initializing`/`registered` target ledger in one atomically replaced secure global user-state record.
3. Serialize creation and every registered-target update through one secure global installation-state lock that is independent of all per-target namespace locks.
4. Permit atomic identifier creation only while holding that global lock and after a verified scan proves the complete Slingshot persistent state root contains no prior global record, target registration, target database, artifact state, or diagnostic state.
5. Synchronize temporary bytes, atomic publication, and the containing directory; validate existing file identity, ownership, type, link status, and current-user permissions through one open handle.
6. Provide locked transitions from absent target to `initializing` to `registered`; resume staged initialization only against an absent or exactly matching database snapshot and refuse every unregistered-database, registered-with-missing-database, or identifier-mismatch combination unchanged.
7. Refuse missing, corrupt, or mismatched global identity whenever any target state exists, preserve every byte, and return a bounded recovery diagnostic without generating a replacement.

**Tests:**

- Same-target and different-target first daemons contend on the global lock, publish one identifier and one ledger, and both target registrations preserve that value.
- Missing, corrupt, and mismatched records beside existing target state refuse unchanged and create no replacement.
- The registered-target ledger survives reopen and cannot be rolled back or substituted through a link or permission race.
- Crashes before publication leave an empty demonstrably reusable root; crashes after publication leave one complete synchronized identity and ledger that every successor observes.
- Crashes at both target-registration transitions resume only the exact staged case; impossible ledger/database combinations fail unchanged.

- **Done when:** `cargo test -p slingshot-storage --test installation_state` proves single atomic creation, durable registered-target identity, and byte-preserving refusal for every loss, corruption, mismatch, permission, and interruption fixture, and all workspace gates succeed.
