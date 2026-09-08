---
id: readiness-follows-complete-durable-startup
title: "Readiness Follows Complete Durable Startup"
workstream: "0075"
kind: task
depends_on: ["the-product-has-one-author-port"]
gated: false
touches:
  - crates/slingshot-command-line/src/daemon_entry.rs
  - crates/slingshot-daemon/src/startup.rs
  - crates/slingshot-daemon/src/service.rs
  - crates/slingshot-development/tests/operation_executor_composition.rs
  - crates/slingshot-agent-connection/src/authentication/runtime_snapshot.rs
status: completed
merged_as: "47d67ea"
---
# Readiness Follows Complete Durable Startup

The product currently publishes readiness after only endpoint bind and a ping/stop service. It never establishes installation/database state, audits the target, installs an executor, or recovers unfinished work.

**Steps:**

1. Introduce one product runtime builder that consumes the already resolved immutable profile/target/trust snapshot and owns all storage, repository, artifact, transport, executor, recovery, maintenance, cancellation, and diagnostic lifetimes.
2. Run hardened installation/database establishment and foreign-state audit under ownership before constructing execution. Refuse wrong target/revision/contract or unfinished foreign state without publishing readiness.
3. Recover durable local/outbox/artifact/maintenance work and install the concrete author adapter before binding the operation plane.
4. Publish readiness only after every required component is usable and advertise exactly the installed protocol versions; unwind endpoint/readiness/leases in reverse order on every startup failure.

- **Done when:** a ready compiled daemon has passed complete durable startup and installed execution, every injected startup/audit/recovery failure leaves no readiness record, and restart reconstructs the same target-bound runtime without direct test composition.

## Execution notes

### Audit includes the retained runtime contract

Startup previously audited only target and selected-environment revision,
despite carrying a selected daemon runtime-contract digest. The reviewed
storage statement and audit records now retain all three identity dimensions.
Unfinished work under another runtime contract refuses startup/executor
installation; terminal history remains queryable and does not block startup.
Distinct contracts under the same target/revision remain distinct partitions,
and the refusal reports partition count rather than incorrectly calling it an
operation count.

Tests cover all four nonterminal lifecycle states, both terminal states,
multiple contracts under one selection, and unchanged operation records after
refusal. Seven startup-audit tests and 15 executor tests passed.

The workspace-wide test compilation found a stale local-session test using the
removed split result-disposition writer. It now uses atomic successful
settlement with a complete inline result and checks the committed disposition
and bytes. Both local-session tests pass with their existing expected output;
the unsafe split API was not restored. `cargo check --workspace --tests
--offline` passed after this review fix, and whitespace checks passed.

This is initial task 7502 progress, not completed runtime composition. The
ownership-bound installation/database builder, recovery/maintenance setup,
executor installation, readiness publication/unwind and compiled restart
integration remain outstanding. Task 7501's finalized commit is unchanged.

### Ownership-bound builder input stage

`RuntimeBuilder` now consumes the resolved authentication snapshot and acquired
namespace ownership. Snapshots retain their resolved profile/environment names
so the constructor can reject a lock for another selection before initializing
provider state. Target and revision derive from the snapshot; the daemon runtime
contract derives from this build. The builder owns the normal async provider,
direct transport, database settings and state root, with the namespace lock
outliving its other owned fields. It exposes neither mutable ownership nor a
readiness/service conversion while durable startup is unfinished.

The constructor uses no endpoint override or injected token source. Tests prove
matching ownership stays exclusive, profile/environment mismatches unwind the
lock, normal drop releases it, debug output is redacted, and this initial stage
creates no durable state, network request or readiness record. The builder
test, seven startup tests and 15 executor tests passed; workspace-wide test
compilation passed. All eight outer concrete Basic/Cloud boundary tests passed
after the snapshot change; whitespace checks passed.

This stage is not yet called by the legacy daemon entry point. Durable
installation/database establishment, recovery and readiness transitions must
be added before that integration can claim complete startup. InstallationState
currently locks individual reads/replacements; the next establishment stage
must preserve one cross-process read/modify/register transaction rather than
compose separately locked calls and risk losing another target registration.

### One installation-ledger lock spans the startup decision

InstallationState now exposes a must-use scoped transaction guard with read,
occupancy and atomic-replacement methods. The same operating-system lock spans
the decision, staged intent and registered record; legacy single-operation
methods delegate to that guard without nested locking. Each replacement is
durable immediately, so guard drop releases ownership without pretending to
roll back a published staging record. Debug output is redacted.

Review found and fixed a related occupancy bug: the previous temporary lock
was dropped after `is_err()` before the directory scan. The scan now stays
inside the held guard. Tests inspect lock contention through a separate file
handle, stage/register under uninterrupted ownership, register eight targets
concurrently without losing entries, and drop/reopen staged intent before
finishing registration. Existing corrupt, linked, oversized and atomic record
tests remain unchanged and pass.

All 13 installation-state tests and seven startup tests passed; workspace-wide
test compilation and whitespace checks passed. The builder's durable stage
will consume this primitive next. Database establishment, recovery and actual
daemon readiness integration remain unfinished; task 7502 stays active.

### Database identity and audit-before-recovery reopen

The database now exposes typed installation-identity reads and a one-time
insertion using the existing reviewed statement inventory. Missing identity is
distinct from unreadable/malformed identity; repeated insertion, including an
identical identity, refuses instead of replacing retained installation state.
Restart tests cover both startup and live connections.

`reopen_bound` requires an existing database, checks its ledger identity and all
three unfinished-work partition dimensions through a read-only connection,
then runs the existing hardened open, migration and recovery sequence. This
closes the ordering gap in the new path: foreign work is rejected before
artifact-reservation recovery or migration. Tests refuse missing, unidentified
and foreign-installation databases, reject each foreign partition dimension
with unchanged database bytes and rows, and allow terminal foreign history.

The new path is a prerequisite for the builder's durable stage, not a claim
that the legacy entry point already uses it. Fresh database/ledger staging,
builder integration, recovery composition and readiness gates remain required.

### Builder establishes the locked installation and audited database

`RuntimeBuilder::establish_durable` now consumes the owned input stage and
returns a `DurableRuntime` retaining the database, stable installation identity,
validated target paths and original selection/namespace lock. Database drop
precedes ownership drop. The transition holds one installation transaction
across empty-root classification, random identity creation, staged intent,
database identity establishment and registration. Existing databases use the
read-only audited reopen; no endpoint or readiness conversion exists yet.

Registered missing databases, unknown existing databases, unreadable ledgers,
foreign identities and missing ledgers beside occupied state refuse. Staged
targets resume only with an absent database or matching audited database. A
partially created database with no identity is not silently adopted; its
retained staging remains available for diagnosis. Established targets retain
their identity on restart and do not rewrite the ledger unnecessarily.

Review found the namespace path helper named `installation.json` while the
actual hardened ledger used `installation-state.json`. The helper now shares
the storage constant, and the builder test checks exact path equality.

Tests exercise eight restart/staging/refusal cases, unchanged ledger/database
bytes on refusal, occupied-root retained evidence, ownership retention and
release, and absent readiness throughout. All three builder tests, eight
namespace tests, seven startup-audit tests and 15 executor tests passed.
Workspace-wide test compilation and whitespace checks passed. The existing
socket-based builder test required a permitted loopback rerun after the
sandbox refused its bind; that rerun passed.

Remaining task gates are concrete runtime repository/artifact/maintenance and
cancellation/diagnostic ownership, recovery/executor installation, actual CLI
entry integration, readiness/unwind and compiled restart/failure evidence.
This durable stage is not by itself a ready daemon; task 7502 remains active.

### Runtime owns repository, artifact, diagnostic and cancellation resources

DurableRuntime now owns the operation repository, retained author-job repository
and subscription ledger. Additional connections use `open_live`, avoiding a
second startup recovery pass, and all are checked against the same audited
database object. Capacity accounts borrow that database and always use the
embedded policy; startup verifies that their usage queries are readable.

The runtime also owns the artifact store and diagnostic sink. Before opening
the artifact store it validates the content directory as private and non-linked
using the same owner-only directory helper as the target paths. The content
directory name is shared with storage rather than duplicated. Diagnostic sink
initialization and health reads must succeed. Target registration follows
successful resource initialization, not merely database creation.

Runtime drop cancels its root cancellation scope before dropping resources;
callers receive only child scopes, so cancelling one child cannot cancel the
runtime or its sibling. Repository and filesystem resources drop before the
owned namespace. There are no spawned workers at this stage.

The builder restart matrix now has ten cases, including blocked artifact
content and diagnostic paths. Tests verify shared database identity, capacity
binding, diagnostic readability, child isolation/drop cancellation, unchanged
identity on restart, no readiness on failure and released namespace ownership.
The matrix and workspace-wide test compilation passed; whitespace checks passed.

Recovery dispatch, maintenance/publication reconciliation, concrete executor
installation and compiled readiness integration are still outstanding. These
resource handles do not by themselves satisfy those later startup gates.

### Startup resumes approved maintenance and completes filesystem deletion

The runtime now revisits its selected target's unfinished maintenance receipts
before returning durable startup state. A bounded reviewed query discovers
only already-applied receipts; recovery never previews or approves new pruning.
Referenced content remains pending and is reported in the runtime's recovery
receipts. A recovery error refuses the startup transition without readiness.

Review exposed an unused cleanup function that marked receipts complete after
deleting database content rows, without deleting filesystem content. That
receipt-acknowledgement path was removed. The replacement holds an immediate
database transaction across reference revalidation, filesystem unlink and
directory synchronization, then removes accounting/journal rows and completes
the receipt only when no candidates remain. Already-unlinked content is an
idempotent crash retry. Invalid digests, non-private/non-regular objects,
symlinks and hard links refuse; failed deletion preserves retryable intent.
The older accounting-only release primitive remains explicitly documented as
not sufficient to complete maintenance and is not used by runtime recovery.

Storage tests cover real-file deletion, interrupted unlink, publication-held
references, directory-blocked deletion, symbolic/hard links, retries and target
isolation. The builder restart matrix now checks an approved pending receipt
is completed through startup. Both storage recovery tests, the builder matrix,
seven existing maintenance tests and workspace-wide test compilation passed;
whitespace checks passed.

Unfinished local/outbox/publication recovery, concrete executor installation and
compiled readiness integration remain required. No completion claim is made
for task 7502 or plan 20.

### Reconstruct retained local input and unique author child

The durable runtime now retains nonterminal operation inputs in enqueue order,
including exact admitted command bytes and persisted lifecycle/recovery facts.
Each input is paired with its unique retained author child, when present,
through a target/local-identifier query and one repository read transaction.
This preserves submission evidence rather than interpreting a restart as
permission to POST again. Reconstructed records are inputs, not execution
leases; dispatch still must use the concrete protocol's retained-input checks.

Startup rejects inconsistent local summaries, installation identities, selected
revisions and runtime contracts, and child revision drift. Review also found
that the read-only pre-recovery audit checked the database's singleton identity
but not installation identifiers on unfinished operation rows. That audit now
rejects unfinished foreign-installation rows before mutable startup; terminal
history remains allowed.

Tests prove byte-identical queued input and unchanged lifecycle/revision across
the builder restart, exact retained child reconstruction and target isolation,
and unchanged database bytes for each of four foreign identity dimensions.
The builder matrix, targeted repository restart test, expanded database audit
test and workspace-wide test compilation passed; whitespace checks passed.

These inputs are not yet dispatched. Retained submission/contract validation
at execution, publication reconciliation, concrete executor wiring and compiled
readiness/restart evidence remain mandatory task gates.

### Runtime invokes the concrete executor over owned resources

DurableRuntime now constructs each retained protocol invocation from its owned
repositories, artifact store, capacity account, async authentication provider
and process-lifetime monotonic clock. UTC assertion time uses the system's whole
Unix seconds separately. ProductAuthorPorts can borrow the runtime's existing
selected transport, so invocation composition does not rebuild trust or add
another connector. The typed command comes from the protocol's independently
validated retained bytes, not a second caller-supplied command.

The runtime invocation checks selected execution identity and installation
ownership, then runs AuthorAgentOperationExecutor. Root shutdown is selected
before execution polling and cancels local work without claiming remote
nonexecution. The caller still owns scheduling authority and durable outcome
folding; this method does not grant a lease or re-admit an operation.

A concrete runtime test preserves an exhausted persisted lookup pause exactly,
leaves the operation unchanged and creates no remote child, rejects a foreign
revision, and refuses local execution after shutdown. It exposed invalid
`query_paths` test input using `paths`; the fixture and earlier reconstruction
fixture now use the real `root_path` schema. The concrete runtime test, builder
restart matrix, 15 executor tests and workspace-wide test compilation passed.

The legacy CLI entry point still does not use this runtime. Scheduler ownership,
publication recovery, complete startup failure/unwind coverage and compiled
readiness/restart evidence remain required before task 7502 can close.

### Selected snapshot factory retains one verified configuration generation

Inspection of the compiled entry path found that profile loading discarded the
credential and certificate bytes after validating their generation. A runtime
factory could therefore only reopen files or accept separately supplied
material. LoadedProfiles now retains those opaque, secret-bearing documents
from the verified generation and lends them by exact reference and role.
Profile/selection source bytes are not redundantly retained, and unrelated
credentials are not parsed during selection.

`build_runtime_snapshot` consumes that generation, resolves the requested pair,
parses selected Basic/Cloud and optional author-certificate material, snapshots
platform trust once, and uses the existing ProfileSelection revision derivation.
It creates no network connection and accepts no separate target/revision,
credential override or filesystem reopen. Refusals contain no source bytes.

Tests cover exact Basic authorization bytes before/after password rotation,
stable target/revision, no token clock for Basic, unused invalid credentials,
selected Cloud credentials, selected additional certificates and malformed
selected material. Both factory tests, six loading tests, six selection tests
and workspace-wide test compilation passed; whitespace checks passed.

This closes a configuration prerequisite for the CLI integration. The compiled
entry still uses its legacy ownership-only path and must be changed together
with complete recovery, service lifetime and readiness gating.

### Server shutdown owns and joins its connection tasks

Review of the entry/service lifetime found that the accept loop detached each
connection task. Returning from shutdown could leave idle connections holding
service references and namespace ownership. The server now owns a JoinSet,
reaps finished tasks during acceptance, and aborts/joins remaining local
connections on stop or accept failure before returning.

Endpoint cleanup is now scoped to listener lifetime. A Unix listener records
its socket's device/inode and unlinks only that same socket on explicit cleanup
or drop. Dropping an old listener cannot remove a replacement. Bind refuses
regular files and symlinks at the endpoint path instead of deleting them as if
they were stale sockets. This also covers endpoint unwind after later startup
failures without requiring every caller's success path to call remove.

Real Unix-socket tests verify drop cleanup, replacement preservation, retained
file/link refusal, and shutdown with one idle connection and all 64 slots
occupied. The latter covers both accept-wait and permit-wait cancellation;
after shutdown no service reference survives and namespace acquisition succeeds
immediately. Seven server tests and workspace-wide test compilation passed.

The first socket test run correctly rejected its non-private temporary root;
the fixture now uses the product's owner-only runtime subdirectory. Socket
tests ran with the required local-socket permission. These lifetime fixes are
part of task 7502's unwind gate, not a claim of completed CLI runtime integration.

### Read-only audit covers unfinished outbox ownership

The outbox schema deliberately has no local-operation foreign key. Auditing
only nonterminal local rows could therefore miss an unfinished orphan or an
author child carrying a different selected revision. The bound reopen now
checks those relationships through a reviewed read-only join before migration,
artifact reservation reconciliation or maintenance recovery. An unfinished
orphan refuses; a child needed by nonterminal local work must match the selected
target and revision. Terminal orphan history is retained without adoption.

The check runs only for schema versions that contain the outbox; supported
version-one databases precede that table and migrate it normally. Four tests
within the new matrix cover matching work, unfinished orphan, child revision
drift and terminal orphan history. Refused cases preserve exact database bytes.
The matrix, three installation/database tests, builder restart matrix and
workspace-wide test compilation passed; whitespace checks passed.

This closes another pre-recovery audit gap. Complete publication recovery and
the compiled entry/service/readiness connection are still unfinished, and task
7502 remains active.

### Abandoned private staging is reconciled without losing publication holds

Startup now removes abandoned UUID-named artifact stages under namespace
ownership before producers can run. It does not remove addressed content,
release publication holds or assert result validity. A held publication whose
stage disappeared must still reverify bytes through normal result recovery.
Stage cleanup synchronizes the directory before reporting its removal count;
unsafe file types, links, non-private files and malformed stage names refuse.

Tests simulate skipped stage destruction while retaining a real publication
hold. Cleanup removes exactly the partial file, is idempotent, preserves the
addressed file and byte charge, and leaves the same publication recoverable.
Additional cases refuse symbolic/hard links and unknown stage names without
removing their targets. Fixtures use the storage-defined `.partial` suffix.
The builder restart matrix now includes an abandoned stage and records one
reconciled stage while preserving installation identity and absent readiness.

Both storage staging tests, the expanded builder matrix and workspace-wide
test compilation passed; whitespace checks passed. This is staging recovery,
not proof that every pending publication has been bound to its retained result.
Publication-to-operation reconstruction and compiled entry/readiness integration
remain outstanding task gates.

### Bounded pending-publication inventory survives restart intact

Startup now reconstructs pending publication identities, artifact identities,
content digests, charged lengths and original timestamps without consuming any
hold. Shared content retains separate producer records, so ambiguity remains
visible to result recovery instead of being collapsed. Invalid identifiers,
negative timestamps, oversized content or an excessive producer count refuse.
A left join ensures missing blob accounting cannot hide a pending producer.

Review caught that a single query using the total producer bound could exceed
the statement inventory's 256-row listing bound. Reconstruction now uses
256-row keyset pages within one read transaction, preserving a single snapshot
while enforcing the total capacity-derived producer maximum. The runtime owns
the resulting startup inventory for later operation/result association.

Tests cross the page boundary with 257 records, preserve duplicate producers
and byte charges across reopen, refuse a smaller capacity policy without
mutation, and reject corrupt producer metadata without releasing any of the
258 retained holds. Existing stage/publication recovery and builder restart
tests pass; workspace-wide test compilation and whitespace checks pass.

This inventory grants neither settlement nor cleanup authority. Associating
each producer with its retained operation/result, and the compiled daemon
entry/readiness connection, remain unfinished; task 7502 stays active.

### Publications resolve to retained operation slots without settlement

Startup now derives the allowed artifact identities from installation, selected
target, retained operation identifier and command-declared slot, including the
structured-result slot. Each pending producer must resolve to one such owner
and fit that slot's byte bound. Unknown owners, wrong installation/target,
undeclared slots and oversized content refuse. Duplicate producer records stay
distinct rather than becoming an implicit choice of one producer.

Terminal owners remain available for this association, but do not enter the
nonterminal execution recovery list. A retained hold can therefore remain
diagnosable without reactivating an ended operation. Unrelated historical
command names absent from this build do not block another publication; a hold
whose own command cannot be resolved still refuses.

Tests cover exact structured-result ownership, both declared remote artifact
slots, wrong target/installation/local owner, unknown command, undeclared slot,
size refusal and unchanged ambiguous producer records. The builder restart
matrix now retains a real pending publication and verifies its reconstructed
owner/slot and original timestamp while the hold remains charged and the local
operation remains queued. Owner-binding tests, the builder matrix and
workspace-wide test compilation passed; whitespace checks passed.

Association proves neither file presence nor result validity. Normal retained
result recovery still verifies content and performs atomic settlement. The
compiled entry/service/readiness connection remains unfinished; task 7502 is
not complete.

### Service retains the established runtime across connection lifetimes

`DaemonService::from_runtime` now consumes the established runtime instead of
requiring callers to separate its namespace lock from SQLite and executor
resources. A mutex makes the movable, non-shareable SQLite connections safe to
retain behind the service's shared connection handle. Conversion derives the
published identity from the immutable selected context, but publishes nothing
and continues to advertise no operation versions while dispatch is control-only.
The service withdraws readiness before dropping its runtime resources.

The restart matrix moves the runtime into the service, drops the creating
handle, and releases the last connection handle on another thread. It verifies
exact selected identity, absent premature readiness, retained namespace
exclusivity, and cancellation only after the last handle drops. The matrix and
workspace-wide test compilation pass. All nine real ping/stop tests pass after
fixing an existing concurrent fixture collision: two tests used the same
process-and-suffix directory and could delete one another's live runtime.
Fixtures now allocate unique short temporary directories without pre-deletion.

This removes the service lifetime integration obstacle; the compiled entry
still needs to load its own verified selection, establish this runtime and use
the runtime-owning service. Task 7502 remains active, not blocked or complete.

### Compiled entry establishes the runtime before readiness

The product daemon entry now verifies its private namespace, acquires ownership,
loads account configuration through the existing hardened loader, builds one
selected authentication/trust snapshot, and establishes the durable runtime
using settings from the embedded runtime contract. Persistent state lives below
the platform's local Slingshot data directory, separately from the runtime root.
Only then does a runtime-owning service bind its listener and publish readiness.
Normal shutdown drains connections, withdraws readiness, removes the endpoint
and releases resources/ownership. Declaration order also keeps ownership alive
through endpoint cleanup when startup fails or its future is dropped.

An explicit `runtime-test-host` feature builds a separate process host that
supplies private test configuration/state trees to the same startup function.
The `slingshot` executable has no corresponding argument or environment hook.
Walking, explicit-start and golden lifecycle tests require this feature; the
existing quality gates already use `--all-targets --all-features`. No account
configuration or system trust file was changed to make tests pass.

The first compiled startup found a real Linux platform-trust failure: the
provider reused the additional-author-CA parser and its 32-authority limit for
the much larger system bundle. Platform bundles now use the platform count,
entry and aggregate bounds, with bounded file reads and certificate decoding.
They retain strict certificate/authority validation and reject private-key or
other blocks. Duplicate system entries collapse deterministically. System
locations are discovered without `SSL_CERT_FILE`/`SSL_CERT_DIR`; inherited
selectors cannot choose IMS trust. The shared PEM decoder charges block/DER
bounds before retaining over-limit results. A 33-certificate platform bundle,
over-limit input, malformed input, private keys and end entities are covered.

Compiled process tests prove stable selected/runtime identity and database
installation identity across restart, a fresh nonce, twenty-starter
convergence, independent namespaces, and inherited certificate-selector
isolation. Seven refusal cases cover configuration, installation, database,
diagnostics, staged artifacts, endpoint obstacles and unfinished foreign
revision. Every case retains database/ledger bytes, leaves no readiness or owner
lock, and removes only its endpoint; an existing non-socket obstacle survives.
An injected readiness-publication bound failure and cancelled startup also
unwind without an endpoint or readiness record.

### Review fixes and requirement audit

The full CLI suite exposed old lifecycle fixtures that started an unconfigured
daemon and a transcript normalizer that still expected numeric request IDs.
Those fixtures now use verified private configuration and normalize canonical
UUIDs; expected transcript bytes were not rewritten. A stale MCP schema-digest
fixture was reviewed independently: all changes were explained by the already
committed canonical-contract annotation change, plus create-page's corrected
title bound (65536 to 1024). Restoring those two historical values reproduced
every old digest; the fixture now pins the current projection. This does not
implement or claim task 7505's remaining MCP surface work.

- Step 1: `RuntimeBuilder` consumes the immutable selection and owns transport,
  authentication, storage/repositories, artifact/capacity, recovery, maintenance,
  cancellation, diagnostics and the last-released namespace lock. The service
  retains this same runtime, including across connection/thread lifetimes.
- Step 2: ledger-bound establishment and read-only foreign-state audit precede
  mutable reopen/recovery. Storage, builder and compiled refusal tests cover
  installation/partition mismatches and preserved retained state.
- Step 3: startup reconstructs local/outbox and owner-bound publication evidence,
  recovers approved cleanup/stages, and installs the selected concrete execution
  path before listener bind. Runtime executor tests retain lookup-first/paused
  evidence and cancellation semantics without replacing execution with a stub.
- Step 4: the compiled entry publishes only after all of the above and listener
  bind. Failure/publication/cancellation tests prove unwind. Readiness and ping
  truthfully advertise an empty operation-version set until task 7503 installs
  versioned dispatch; no future scheduler or MCP capability is advertised early.

The full command-line all-target/all-feature suite passed. Configuration tests
passed, with the native ACL capability case rerun successfully outside the
restricted filesystem. Runtime ownership/restart/executor tests, the seven
startup process tests, workspace all-target/all-feature compilation and
whitespace checks passed. This closes task 7502. Versioned dispatch (7503),
scheduler claims (7504), MCP composition (7505), diagnostics routing (7506) and
the broader end-to-end operation proof (7601) remain separate unfinished tasks.
