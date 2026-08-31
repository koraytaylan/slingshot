---
id: daemon-configuration-startup
title: "Daemon Configuration Startup"
workstream: "0016"
kind: task
depends_on:
  - target-runtime-namespace
  - persistent-installation-identifier
  - sqlite-schema-and-migrations
gated: false
touches:
  - crates/slingshot-daemon/Cargo.toml
  - crates/slingshot-daemon/src/startup.rs
  - crates/slingshot-storage/src/operation_repository.rs
  - crates/slingshot-storage/src/sqlite_statement_inventory.rs
  - crates/slingshot-command-line/src/daemon_process.rs
  - crates/slingshot-command-line/tests/daemon_process.rs
  - crates/slingshot-daemon/tests/configuration_startup.rs
  - "crates/slingshot-daemon/tests/fixtures/configuration-startup/**"
status: done
merged_as: "5d48a718e850eddf89f4cd7aa8b19a8ae12eee8f"
---
# Daemon Configuration Startup

The long-lived daemon must establish its own trusted target from the Slingshot configuration root rather than inheriting endpoint or credential material interpreted by a short-lived client.

**Steps:**

1. Author fixtures first for valid typed Plan 0002 snapshots; exact `S1 -> listed source/digest verification -> discovered/transitive source-set equality -> S2` committed-generation success and every missing/mixed/mismatched/exhausted cut; equal target/revision under genuine same-principal rotation; independently changed opaque target, selected revision, and both; profile-authentication-contract-only drift; canonical-equivalent route-typed snapshots and independently changed `VerifiedIdentityManagementTrustPolicyIdentity` or `VerifiedAuthorTrustPolicyIdentity`, live source edits, and an additional-author-CA that cannot authenticate an Identity Management Services interception; Plan 0002's named Basic and Cloud organization/client/`integration.id`-backed technical-account typed-vector outputs; explicit selection; defaults; Linux/macOS effective-user account-database and Windows current-token profile roots with ambient-variable traps; missing/relative/empty/non-Unicode/ambiguous root failures; invalid credential references; installation-record loss/corruption/mismatch; empty-root creation; terminal old-target/revision history; nonterminal old-target/revision rows in every state; unavailable SQLite source/build/configuration/VFS/closed-SQL/no-disk-transient proof; and secret-bearing failures. Never duplicate or derive Plan 0002's principal/revision/source-generation preimage fields.
2. Make the daemon entry accept profile and environment names plus a typed test-only configuration-root source. Production has no root override: Plan 0002 samples the account identity once, resolves its operating-system-authoritative home/profile directory, appends literal `.config/slingshot`, ignores ambient path variables, and opens it by absolute directory-handle traversal.
3. Ask Plan 0002 to construct one `SelectedEnvironmentSnapshot` only after its exact committed-generation proof succeeds; consume the resulting immutable authentication/root material and opaque typed identities, and accept no endpoint, deployment, authentication, credential, metascope, trust, source-generation, or root-store value from process arguments. A generation proof mismatch returns no snapshot and therefore cannot reach operation admission.
4. Derive the runtime namespace only from profile/environment names. Consume complete `AuthorTargetIdentity`, its SHA-256 output `AuthorTargetIdentityDigest` without hashing its lowercase rendering, and `SelectedEnvironmentRevision` directly from the snapshot; do not derive or inspect their profile-contract, principal, metascope, platform-IMS-root, or effective-author-root members.
5. Under the global installation-state lock, create identity only for demonstrably empty state; for a new target persist `initializing`, create/synchronize its database with the identifier, then persist `registered`; otherwise atomically compare global record, ledger state, and database snapshot.
6. Before endpoint bind, readiness, executor access, or network access, audit every author-target partition and stored selected revision and require Task 1503's exact SQLite source/build, pre-initialization no-spill configuration, memory temp store, restrictive VFS, closed SQL inventory, and physical-object whitelist evidence. Refuse unchanged if any invariant is unavailable or any nonterminal row differs from the selected identity/revision, including a same-source canonical-metascope, either selected root identity, or principal change; allow terminal old-target/revision history and select only the current principal-and-revision context for execution.
7. Retain the snapshot's platform-only root bytes and `VerifiedIdentityManagementTrustPolicyIdentity` for Identity Management Services plus its distinct effective platform-plus-selected-additional-author root bytes and `VerifiedAuthorTrustPolicyIdentity` for author, immutably for daemon lifetime. Never merge, cross-use, or DER-only widen them. Profile, selection, credential, certificate, additional-author-CA, and operating-system provider-policy changes become observable only after explicit stop and new-owner startup and never retarget or rebuild a live owner.
8. Route failures through bounded redacted diagnostics; refusal leaves no endpoint or readiness record and never mutates operation state.

**Tests:**

- Deterministic policy fixtures cover the exact sampled-account `~/.config/slingshot` contract for every supported row and ignore all ambient home/config variables, while at most the current matching native row emits an explicitly untrusted observation and tests use only the typed temporary-root source. Unavailable/invalid account-profile results read no configuration and publish no readiness; Plan 0009 owns authenticated aggregate evidence.
- Valid Plan 0002 snapshots supply an opaque-principal target identity, its exact once-hashed target digest, security-context revision, and name-only namespace. Equal typed target/revision under genuine same-principal rotation remains compatible; a supplied revision-only change preserves the target partition but refuses old nonterminal work, while a supplied target change selects another partition without a second namespace. Contract-only and Plan 0002 Basic/Cloud tuple vectors prove the expected opaque changes without downstream preimage reconstruction.
- Missing, ambiguous, duplicate, invalid, argument-injection, unverifiable SQLite invariant, prohibited SQL, and ambient-temp/VFS-open cases fail before endpoint bind or readiness publication and leave persistent bytes unchanged.
- Missing, corrupt, or mismatched installation state beside existing state leaves every byte unchanged and publishes no readiness.
- Exact staged target initialization resumes; unregistered database, registered missing database, or impossible staged combinations refuse unchanged.
- An incomplete or mismatched committed generation publishes no snapshot, endpoint, readiness, or operation row. Changing any snapshot source, operating-system provider policy, or selected additional author certificate authority does not retarget/rebuild a live daemon; explicit stop/start captures one complete current generation and the two distinct immutable root snapshots deterministically. Same-principal secret rotation may supply equal target/revision, whereas restart-visible drift in either root identity supplies only a changed revision.
- Terminal old-target/revision history remains queryable; any old-target/revision nonterminal row, including work admitted under another authentication principal, canonical metascope set, `VerifiedIdentityManagementTrustPolicyIdentity`, or `VerifiedAuthorTrustPolicyIdentity` at the same author address, refuses startup unchanged and is never failed, reconciled, or executed against the selected security context.
- No password, private key, client secret, signed assertion, or access token appears in startup output or metadata.

- **Done when:** `cargo test -p slingshot-daemon --test configuration_startup` proves operating-system-account configuration-root resolution, exact complete-generation admission, immutable stable-read principal/scope/two-root snapshots, direct no-second-hash target partitioning, genuine same-principal credential-rotation compatibility, profile-contract/scope/root-policy/principal pre-network recovery refusal, live-root immutability plus restart-visible revision-only drift, additional-author-CA rejection for Identity Management Services, atomic installation comparison, pre-readiness cross-partition/revision audit, byte-preserving refusal, terminal-history coexistence, explicit-stop/start reload, and secret/raw-principal/source-digest-free failures, and all workspace gates succeed.
