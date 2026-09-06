---
id: the-daemon-serves-versioned-operations
title: "The Daemon Serves Versioned Operations"
workstream: "0075"
kind: task
depends_on: ["readiness-follows-complete-durable-startup"]
gated: false
touches:
  - crates/slingshot-daemon/src/service.rs
  - crates/slingshot-daemon/src/request_dispatch.rs
  - crates/slingshot-daemon/tests/request_dispatch.rs
status: planned
merged_as: ""
---
# The Daemon Serves Versioned Operations

The product service dispatches only ping and stop and advertises no operation version, while every CLI operation first asks hello and then sends a versioned `OperationEnvelope`.

**Steps:**

1. Add hello and versioned `OperationEnvelope` dispatch to the product service, using exact command registry, limits, schema identity, and Plan 0019 repository methods.
2. Persist accepted submission before acknowledgement and expose bounded status, wait/progress, result, artifact, cancellation, recovery, and maintenance methods through the retained local protocol.
3. Advertise exactly the operation versions installed in this runtime and return typed unavailable, unsupported-version, validation, and domain refusals rather than method-not-found or an empty hello.
4. Drive every method across a real framed endpoint, including malformed/foreign envelope, exact/adjacent version, unavailable executor, backpressure, cancellation, and shutdown cases.

- **Done when:** hello truthfully advertises the installed operation protocol and every public operation/control method crosses the real framed service to one persisted result or typed refusal, with no ping/stop-only fallback.

## Implementation checkpoint

The selected-author runtime/configuration boundary and ownership-bound durable
startup are implemented in tasks 7501 and 7502. No external configuration choice
or new authority is needed to continue this task.

The service now answers hello from its retained runtime identity and advertises
the installed operation protocol version, with a typed unavailable response when
no identity is established. Compiled startup tests
check the greeting against the published target, revision, runtime contract,
namespace, and nonce, and reject nonempty or non-object greeting arguments.
The service now recognizes operation envelopes at the framed boundary and binds
their target, environment revision, and runtime contract before repository access.
Execute admission, status, listing, result, recovery resume, and maintenance
metadata requests cross that boundary and render typed operation responses; the
runtime's operation-free maintenance and operation-slot repositories remain
separate. Operation streaming, wait publication, cancellation, and terminal
maintenance coordinators still remain to be connected. Artifact and maintenance
read streams now emit ordered framed start/chunk/end responses through the local
server, with the existing verified-reader end gate. Wait requests now bind to the
runtime waiter registry and return an already-queued catch-up update through the
same endpoint; root-shutdown cancellation is wired, while peer-disconnect
cancellation ownership and executor scheduling still remain to be connected.
Preview manifests are
held by digest only after this service produces them; apply refuses an unknown
or expired digest and revalidates the retained manifest atomically. A runtime
integration test sends an operation envelope through a real framed connection
to the live service and receives a target-qualified missing-operation response,
while adjacent/foreign
bindings remain refused before repository access.

Strict operation-envelope decoding now checks foundation bounds and rejects
duplicate decoded object keys throughout command arguments before constructing
the typed request. `operation_dispatch::BoundRequest` validates the installed
version before interpreting its request vocabulary, then checks all three
retained identity bindings before allowing repository access. Its status handler
reads the persisted lifecycle and revision, with missing-operation and bounded
read-failure responses. Tests cover exact/adjacent versions, foreign bindings,
duplicate headers/nested keys, an uninstalled surface, and status after SQLite
reopen. They are now wired into service dispatch and framed endpoint tests; the
remaining review matrix covers wait publication, cancellation, and executor
scheduling, plus complete endpoint shutdown behavior.

Execute preparation now deserializes the exact installed typed command, checks
all five catalog/installed identity fields, normalizes canonical arguments, and
derives the fingerprint in the daemon. `PreparedAdmission::persist` returns
acceptance only after the repository admission transaction commits. Tests prove
reopen/replay at capacity, conflict without overwriting retained work, capacity
refusal without a new row, workflow/request correlation independence, and
unavailable/invalid-input refusal before admission. Scheduled persistence now
uses embedded pending limits and maps a full queue to
`SchedulerCapacityExhausted`. The storage transaction checks replay/conflict
first, then counts unfinished rows excluding the owner's live execution slots,
and checks global/per-caller capacity before insertion. The owning runtime must
hold its scheduling lock across this call. Tests cover competing connections
at the last slot, per-caller isolation, live slots, terminal history, and replay
at capacity. Runtime slot ownership and the remaining public wire mappings of
repository failures still need endpoint composition.

Listing now applies the wire's lifecycle filters in an inventoried SQLite
keyset query without reading command/result bodies. Cursors carry format,
target, normalized filter digest, ordering, last sequence/identifier, and an
integrity digest; decoding checks canonical spelling and every binding. New
admissions must be representable within the declared cursor bound. Tests cover
reopen plus concurrent newer arrivals, filter normalization and mismatch,
terminal filtering, cursor tampering, wrong target/order/version, and exact or
exceeded page/filter/cursor bounds. The wire now also carries optional caller,
workflow, and terminality filters without changing the encoding of requests
that omit them. SQL intersects every supplied filter; the cursor digest binds
the complete filter set. Tests prove target isolation, contradictory filters,
wire types, workflow bounds, invalid identities, and cursor invalidation on
each additional filter change. The existing CLI request constructor preserves
its unfiltered defaults. The listing handler is not yet connected to the framed
endpoint.

Recovery dispatch now validates category/identifier, invokes the durable resume
service, and projects applied/replayed receipts with the currently persisted
lifecycle. Its opaque receipt identifier derives from immutable target,
environment, and source bindings, so settlement/reopen does not change it.
Tests cover stale revision, wrong category, no outstanding recovery, missing
operation, bounded validation refusal, and replay reporting a later terminal
state. Repository failures still await the shared endpoint refusal mapping.

Review found that existing receipt fast paths could replay before checking the
requested environment revision. Both the recovery service and transactional
receipt methods now enforce that immutable binding before replay. Foreign
revision requests preserve the operation and receipt; same-environment replays
after progress/settlement/reopen still succeed. The 12 dispatch tests, seven
daemon recovery tests, 13 storage recovery tests, and ten minimal-protocol
checks pass.

Result dispatch now classifies one retained row observation and returns explicit
`ResultInline` or `ResultArtifact` success responses, rather than overloading a
transfer-start frame. Inline results must be canonical JSON within the embedded
inline bound. Structured-result metadata must occupy the reserved slot, match
the installation/target/operation-derived artifact identity, and carry canonical
digest, media type, and bounded length. Missing or malformed retained results
produce a bounded read failure without rewriting terminal history. Pending,
recovery, and terminal-failure responses preserve the domain's conditional
execution evidence; terminal kind/disposition mappings are exhaustive.

The CLI projects both explicit successful-result shapes and rejects a result
that names another operation or exceeds the inline bound. The 15 dispatch
tests, two CLI result-projection tests, and complete 52-test local-protocol suite
pass. Result reads remain component-level: endpoint composition, command-artifact
access metadata, streaming verification, and the complete framed review matrix
are still required before this task can be completed or a version advertised.

Artifact dispatch now resolves an opaque identifier through a bounded,
inventoried target-and-operation-qualified association lookup. Ambiguous slots,
foreign operation associations, invalid deterministic identities, and oversized
metadata refuse before opening bytes. A successful operation is required. The
stream retains one verified handle, emits bounded padded-base64 chunks with
absolute offsets, and emits `ArtifactEnd` only after final same-handle integrity
verification. Dropping it does not change operation state. Tests cover resumed
and empty-suffix reads, wrong digest, past-end offset, cross-operation access,
and truncation after the start frame without false success or terminal-row
mutation. All 16 dispatch tests, 32 artifact-store tests (including the new
ambiguity test), 15 migration/inventory tests, and ten minimal-protocol checks
pass. Real framed streaming, connection cancellation, reader admission, and
shutdown integration remain to be composed; this does not advertise protocol
support yet.

Review follow-ups before task completion:

The maintenance integration audit found that the four operation-free request
variants repeat their target inside the request body. `BoundRequest` now checks
that address against the already-validated outer/runtime target before allowing
any handler access. Tests cover all four variants, exact bindings, canonical
foreign targets, malformed/empty targets, and public-safe refusals naming only
the served target. The storage schema and SQL inventory include maintenance
result associations, but the corresponding runtime repository/ownership wiring
is not implemented yet. The preview audit also found the wire omitted the
explicit `before` criterion required by the maintenance architecture. The
request now requires `before_unix_milliseconds`, and the CLI requires `--before`
and carries its exact unsigned value without inventing a clock-derived cutoff.
Wire tests reject omitted, null, negative, fractional, and string cutoffs; CLI
tests cover missing/invalid/overflow input and exact zero/maximum propagation.
The canonical preview request fixture now includes the cutoff. Maintenance
association ownership and endpoint wiring remain implementation work, not
external blockers or completed functionality.

`maintenance_results::read` now uses the inventoried target/identifier lookup
with blob and receipt-owner joins in one SQLite observation. It validates
canonical digests, deterministic result identity, positive bounded length,
matching blob length, positive association revision, media type, closed kind,
and current-preview or matching reviewed-source receipt ownership. Missing
results remain distinct from invalid retained metadata. Pure validator tests
cover current/receipt-owned variants and corrupt identity/length/ownership;
a real empty repository test verifies the query and absent/invalid-address
distinction. These three tests pass. Successful populated-row/reopen proofs
are now added through `record_current_preview`: it accepts already verified,
durably accounted content, validates blob length/identity and accounting scope,
then atomically supersedes the current-preview association and checks capacity
inside the same immediate transaction. An exact current-preview repeat replays
even at capacity. Failed replacement rolls back the prior association; returned
superseded metadata leaves blob accounting conservative until reference-checked
cleanup. The populated SQLite test covers reopen/replay, replacement at the
association limit, rollback under refusal, target isolation, and missing or
mismatched blob metadata. All four maintenance-result unit tests pass. This
association API does not stage or publish files: the caller must retain verified
content and its durable publication hold through commit. Operation-free file
publication, superseded-blob cleanup, receipt retention, wire metadata mapping,
and streaming remain to implement.

Maintenance apply now validates any current preview association before removals
and refuses a superseded reviewed digest. After inserting the application
receipt, the same transaction transfers the matching preview to that receipt
and increments its association revision. A replay takes the existing receipt
path without changing a later current preview. The new populated SQLite test
proves superseded refusal preserves the preview, receipt ownership survives
reopen, and replay preserves both the applied association and a newer current
preview. All five maintenance-result unit tests pass. Application-result document
publication and completed-receipt retirement remain separate unfinished work.

`record_application_result` now associates already-accounted application-document
content with an existing target-qualified receipt in an immediate transaction.
It validates the operation-free application identity and blob metadata, checks
association capacity before insertion, and refuses a different result for a
receipt that already owns one. Exact replay is checked before capacity. The
populated receipt test now covers refused insertion at capacity, successful
association, replay at capacity and after reopen, wrong-target/missing receipt,
and conflicting content without replacement. The application and retained
preview have distinct identifiers even for identical content. File publication
and durable publication-hold consumption are still caller prerequisites and
are not yet integrated into the runtime.

Bound maintenance metadata dispatch now projects validated association fields
and distinguishes `MissingMaintenanceResult` from a missing operation. Invalid
identifiers and invalid retained metadata receive bounded refusals; the CLI
recognizes the operation-free missing-result response. A populated dispatcher
test creates preview/application associations, applies maintenance removing the
producing operation, reopens SQLite, and reads both retained descriptions with
the correct kind, receipt owner, association revision, and reviewed digest.
This is a metadata/ownership proof, not a file-publication or framed-streaming
proof. All 19 dispatch tests pass; endpoint composition remains outstanding.

Maintenance reads now open content by validated maintenance metadata without
manufacturing operation/artifact identifiers, using the shared verified-handle
reader. The bounded stream checks expected digest/offset before opening, hashes
resumed prefixes, emits absolute-offset base64 chunks, and emits its maintenance
end marker only after final verification. The populated dispatcher test now uses
real shared content and verifies resumed/empty-suffix transfers after operation
removal/reopen, digest and offset refusals, and truncation after start with no
success marker or association mutation. Shared reader verification checks initial
length and bounds its first pass to expected length plus one byte, preventing a
concurrent append from extending that scan indefinitely. Artifact regression
tests remain required alongside maintenance streaming tests. Operation-free
publication and real framed streaming/cancellation are still unfinished.

Cleanup review found that an applied receipt could remain `DatabaseApplied`
forever when a retained maintenance association shared one of its cleanup
blobs. Cleanup now distinguishes durable associations from transient publication
holds: durable sharing retires that receipt's cleanup item without deleting the
file or accounting, while publication-only holds keep retryable intent. The
dispatcher persistence/streaming test now completes cleanup before reopen and
still reads the retained bytes. Existing restart tests continue proving that
publication-only holds delay deletion and that release permits retry to finish.
All seven maintenance storage unit tests and the shared-content dispatcher test
pass. A separate durable cleanup mechanism for superseded current previews is
still required; this change does not claim to implement it.

Superseded-preview cleanup now has a durable path: replacing a preview with
different content records a `preview:<result-id>` cleanup owner in the same
transaction. This namespace is disjoint from digest application-receipt owners
and remains operation-free. A subsequent replacement is refused until the
previous obligation resolves, while exact replay remains available; same-content
replacement needs no cleanup record. Cleanup validates the bounded journal,
holds the write transaction across reference checks and filesystem removal,
and clears accounting/intent only after deletion or verified durable sharing.
Publication-only holds retain retryable intent. Runtime establishment resumes
this cleanup under ownership. Tests cover intent across reopen, cross-target
durable sharing, backpressure/reuse, already-unlinked files, filesystem refusal,
and publication holds followed by retry; all eight maintenance unit tests pass.
Compiled startup and framed endpoint coverage still need the final task review.

Operation-free maintenance content now has a separate private staging type.
It validates target/source digests and canonical JSON before creating a file,
enforces the preview-manifest/application structured-document bounds, and derives
the maintenance identifier without installation, operation, or artifact-slot
inputs. The synchronized stage supports verified reads and no-replacement
physical publication; drop removes only its own stage. Tests prove kind-separated
identities over shared content, private-before-publish behavior, deduplication,
drop cleanup, exact/exceeded size bounds, canonical-input refusal, and changed
stage refusal. Shared existing-content verification now limits scanning to the
expected length plus one byte. The caller must still reserve capacity and retain
a matching durable publication hold through association commit; that operation-
free hold API and runtime orchestration remain unfinished.

Schema migration 0012 adds a separate maintenance-publication owner record
alongside shared blob protection, preserving target, kind, and reviewed source
without an operation row. A target has at most one pending maintenance producer.
`retain_maintenance_publication` transfers a validated reservation into the
shared durable hold and owner record in one transaction and returns a distinct
maintenance token. The new test verifies missing-reservation refusal, byte-charge
transfer, duplicate-producer rollback, physical publication, token drop retaining
protection, and complete owner fields after reopen. Migration expectations now
include schema version 12 and the new table.

Startup now reconstructs a maintenance producer separately from operation-slot
owners, validating canonical target/source/content, the kind-derived result ID,
producer UUID, timestamp, and kind-specific document bound. Classification checks
the complete common inventory against the maintenance record before removing
that exact producer from operation-owner binding. Foreign/unknown producers
remain in the operation audit rather than disappearing through a query filter.
The runtime retains the operation-free recovery evidence without releasing its
hold or treating it as a successful result or maintenance approval. Reopen and
target-isolation coverage plus malformed identity/source, negative timestamp,
empty and oversized document cases pass; all 12 maintenance unit tests and all
15 migration/inventory tests pass, and the all-features daemon check succeeds.
The exact maintenance-hold completion API now validates the producer against
the target's retained document and its actual result association inside a write
transaction. It verifies published bytes through the bounded, digest-checked
reader before deleting only that producer; the companion owner cascades while
blob accounting and associations remain. An association may have become a
receipt-owned preview before completion without being reset to current.
Tests cover missing association/file, foreign target, stale producer after a new
same-document attempt, shared content with an unrelated producer, receipt-owned
preview/application completion, and same-length file corruption followed by
retry. All 13 maintenance unit tests pass; migration/inventory (15) and persistent
capacity (23) regressions pass. This closes the storage completion primitive,
not incomplete-publication reconciliation or framed maintenance production.

The durable runtime now owns `RuntimeWaiters`, sharing its cancellation scope.
Connection-owned handles release their exact reader ticket on cancellation,
terminal delivery, or drop, and the final reader removes the operation registry.
Per-operation bounds come from the runtime contract; aggregate local observers
are additionally capped by the foundation connection capacity. Publication with
no observers creates no history. Notification is enabled before queue inspection
to close the inspection/sleep race. Tests cover pending-reader wakeup, independent
client cancellation, runtime shutdown, catch-up, already-observed terminal state,
future-revision refusal, capacity saturation, and capacity reuse after drop.
The 14 waiter tests and ten minimal-protocol checks pass. Service integration
must still hold the runtime lock across persisted observation/registration and
commit/publication; these APIs do not themselves prove that endpoint integration.

Bound wait dispatch now reads the addressed persisted operation before attaching
to runtime observer state. It projects bounded current progress, validated
recovery evidence, or terminal revision; future observed revisions and missing
or malformed identifiers refuse without allocating a reader. The new
`WaiterCapacityExhausted` wire response distinguishes observation pressure from
execution queue pressure, and the CLI maps it through its unavailable-outcome
path. Dispatch tests cover SQLite reopen, initial catch-up, capacity saturation,
unchanged durable revision on refusal, terminal catch-up, already-observed
terminal state, and reader-slot release. All 17 dispatch tests pass. This remains
an internal handler until the service supplies the required runtime lock and
framed connection lifecycle.

- Wait requests now optionally carry the last observed revision; omitted values
  preserve the existing CLI/wire spelling. Review found that attachment could
  enqueue a newer catch-up snapshot while subsequent broadcasts queued older
  revisions behind it. Queues now reject updates behind either the delivered
  revision or queued observation. A full queue containing only critical
  transitions now preserves the latest recovery/resume transition rather than
  silently retaining obsolete state, as required by Plan 0004's architecture.
  Progress cannot displace critical state, and terminal state survives. All 11
  waiter tests pass, including catch-up before/after consumption, an already
  observed revision, and repeated recovery/resume transitions under pressure.
  The wire has canonical optional-revision and invalid-type coverage. Runtime
  registration, persisted-update publication, and framed wait cancellation are
  not yet integrated.
- The CLI golden greeting refusal was updated to the truthful unsupported-version
  response; both detached-owner golden tests now retain a nonce-bound cleanup
  guard so a failed assertion does not remove a live daemon's endpoint.
- The broader local-protocol run exposed existing hard-coded numeric values
  in runtime, artifact recovery, and daemon fixtures. SQLite page size and busy
  timeout now come from the runtime contract; independent digest widths,
  timestamps, progress percentages, and fixture observation windows have named
  constants without changing their values. The complete local-protocol suite
  passes, including the unchanged
  `no_other_repository_source_repeats_a_contract_value` check. Compiled startup
  and golden CLI sessions also pass (16 tests). All 50 affected daemon tests,
  including the full selected-author event/recovery matrices, subsequently
  passed; the live-event matrix completed normally rather than being restarted.
- The SQL-inventory review exposed raw fixture SQL in startup and maintenance
  recovery tests. Startup fixtures now use repository admission/settlement;
  cleanup fixtures use existing inventoried statements. All 15 migration and
  inventory checks, the startup binding unit test, and both cleanup recovery
  unit tests pass without weakening the inventory check.

The service boundary now advertises the installed operation version from a
runtime identity and distinguishes operation frames from control frames before
dispatch. Exact binding, admission, status, listing, result, recovery, and
maintenance metadata/preview/apply responses are rendered as bounded operation
frames. Preview manifests are retained by digest for the apply round trip and
are atomically revalidated; unknown digests are refused. Artifact and
operation-free maintenance readers are streamed as ordered start/chunk/end
frames, and wait requests return an already-queued catch-up update while
releasing the reader on connection close. A selected-runtime test sends a real
operation frame through `serve_connection` and validates the response. The
remaining gaps are long-lived wait broadcasts, cancellation propagation into
execution, and scheduler-driven execution after admission.
