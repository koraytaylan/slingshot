---
id: the-product-has-one-author-port
title: "The Product Has One Author Port"
workstream: "0075"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-agent-connection/src/lib.rs
  - crates/slingshot-daemon/src/author_agent_operation_executor.rs
  - crates/slingshot-daemon/tests/author_agent_operation_executor.rs
  - crates/slingshot-daemon/tests/author_agent_conformance.rs
status: completed
merged_as: "980c5a0"
---
# The Product Has One Author Port

`AuthorPorts` is the executor boundary, but only tests implement it. The agent-connection crate publishes structures and dependencies without a product connection, so no compiled command can reach an author service.

**Steps:**

1. Implement one product `AuthorPorts` adapter over the exact selected author endpoint, immutable target identity, verified author trust policy, bounded DNS/connect/TLS/request/response phases, and protocol codec already specified by Plan 0005.
2. Reject proxies, publisher endpoints, redirects outside the selected origin, ambient trust, trust-policy reload, response compression/framing ambiguity, and any command or result identity mismatch before returning evidence to the executor.
3. Preserve request-start retention, logical outbox/fence, reconnect, uncertainty, and terminal receipt distinctions without logging credentials or private identity material.
4. Drive the concrete adapter against a protocol-faithful fake author for every success/refusal/uncertain phase, including hostile additional CA, redirected origin, duplicate physical record, truncated response, restart, and cancellation.

- **Done when:** the concrete product adapter satisfies the complete existing agent conformance suite and the executor has no test-only or alternate path for network operations.

## Execution notes

The missing runtime interfaces are implementation work within this task, not
an external blocker. The executor, author ports, and protocol boundary now
return futures; storage exposes a transactionally consistent retained command,
runtime contract digest, and operation summary through `read_execution_input`.
Reading this input is not a scheduler claim or permission to resend.

The selected-author connector is exercised against a real loopback HTTP fake
through profile loading and integrity-inventory verification. Loopback HTTP
requires no insecure-transport warning; protected addresses always require TLS.

The finite HTTP/1.1 reader now handles Content-Length and basic chunked framing
with cumulative decoded-body bounds, idle/total deadlines, trailer rejection,
and EOF verification. Real socket fixtures cover both accepted framings and
conflicting lengths/codings, truncation, trailers, and surplus bytes. The common
gate independently rejects nonfinal/redirect statuses and missing media types;
response debug formatting redacts remote metadata and bodies. Chunk extensions,
HTTP/2, and route-specific media validation are not yet conformance-complete.

The selected-author submission driver performs one real POST with derived
headers, local/remote operation binding and installed-contract/digest preflight.
It preserves uncertainty after possible writes and never retries internally.
Socket tests inspect the emitted request and verify JSON charset/identity
refusals. Cross-checking the author servlet found and corrected the request
members to `canonical_arguments` (a string preserving the retained bytes) and
`artifact_manifest`. The CSRF route now uses the specified `token.json` suffix;
token debug output is redacted. Conflict responses require matching identity
echoes rather than settling on HTTP 409 alone.

The sibling author's immediate-command acknowledgement currently returns an
empty physical job set, contrary to this plan's nonempty-set requirement. Keep
the client refusal; do not relax conformance to make that implementation pass.
Full peer conformance remains part of this task's completion review.

The daemon's durable initial-submission coordinator now persists exact wire
bytes and contract/identity fields before issuing a non-cloneable, consumed
first-send permit. Only a newly inserted child can obtain that permit; an
existing matching child requires lookup regardless of a changed local attempt
or clock, and differing immutable bytes refuse. Separate-connection race and
restart/drop tests exercise this ownership. This does not reuse the remote
execution checkpoint as a client send lock. A queued child with no physical
association and zero retention is pending bookkeeping, not proof of author
acceptance. Accepted/duplicate outcomes now pass atomic durable association of
the complete physical-job set and bounded retention before leaving the permit.
Permits borrow their admitting repository, so the request cannot be acknowledged
into a different database. Association failure becomes lookup-required
uncertainty, and repeated acknowledgements cannot renew retention. Storage tests
cover reopen, identity mismatch, invalid sets, and rollback after a forced SQL
failure. The full storage run also found and fixed `remove_ended`'s outdated
two-parameter call to the four-parameter conditional delete; it now reads and
deletes the exact ended record in one transaction.

Recovery-authorized resends and complete protocol composition remain to implement.

The recovery integration audit found older route constants that agreed with
the fake and sibling servlet but contradicted Plan 0005. The client and fake
now use the normative `/bin/slingshot-agent` routes, including POST `jobs`,
operation lookup, physical snapshot, event high-water, and path-based artifacts.
Literal tests pin these separately from the fake's table. Artifact placeholders
refuse noncanonical identifiers/slots. The bounded HTTP query driver preserves
the selected context prefix, query order, and exact-once encoding, and refuses
duplicate/oversized queries before connecting. The submission idempotency
header now carries AgentOperationIdentifier as specified, not command digest.
The sibling servlet is not authoritative where it diverges from this plan;
its route/key/acknowledgement disagreements remain peer-conformance follow-ups.

Submission acknowledgements now require selected revision and the complete
versioned provenance echo before accepted, rejected, retired, or conflict
outcomes are exposed. Tests independently change format, transport/canonical
digests, all five command-contract members, and revision; missing mandatory
echoes fail decoding. Contradictory nonexecution flags/job sets remain unknown.

The snapshot wire type and schema now require full provenance, subscription,
revision, digest, physical set, progress/attempt, and retention. The schema's
exact-byte manifest was updated and the protocol suite verifies it. The bounded
decoder rejects missing/surplus fields, changed echoes, and invalid physical
sets before reconciliation. A real loopback test submits and then performs the
selected-author lookup. The lookup receipt separates the original grant from
the remaining request-start budget. The closed `lookup-absence.json` schema now
pins HTTP 404 `missing` and HTTP 410 `retired` documents. Missing echoes only
format/transport/generation/operation/target lookup context, not fabricated
command provenance; retired echoes the full saved provenance, subscription,
revision, and command digest. Status/document disagreement, bare status bodies,
missing/surplus members, and changed context fail. Both branches run through the
same selected finite transport, with real-socket coverage proving no new POST.
The durable lookup coordinator now requires an existing child and compares its
exact request bytes and complete contracts before opening the selected lookup.
Active nonterminal snapshots commit observation, physical associations,
watermark, and conservative retention in one transaction. Stale reads,
same-sequence disagreement, regressions, and terminal snapshots refuse this
nonterminal path. Fault-injection tests prove a final watermark-write failure
rolls back every preceding write. Terminal and absence receipts still need their
separate result coordinator. Missing lookup receipts now persist recovery under
the local operation revision CAS before returning. Grace stays anchored to the
durable request start, sampled full-jitter delays are persisted once, and retry
exhaustion pauses uncertain work without permitting another POST. Clock rollback
before request start and stale revisions refuse without mutation. Restart tests
preserve the original grace deadline; further tests distinguish manual-resume
eligibility from exhausted automatic attempts and forbid downgrading proven
remote success. The operation-submission suite (9), durable-idempotency suite
(10), and author conformance suite (7) pass. These are focused regressions, not
proof of complete product composition. Next: recovery-authorized resend and
terminal receipt coordination, then complete supervision/results/artifacts.

Use the ignored workspace `target` directory with `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, and `CARGO_PROFILE_TEST_DEBUG=0` for verification.
The prior `/tmp` targets exhausted temporary-storage quota. Local socket tests
require loopback networking permission in the sandbox.

Still required before completion: the concrete protocol implementation joining
authentication, retained submission/outbox evidence, HTTP codecs, recovery,
events, results, and artifacts, followed by the complete conformance tests.
`AuthorAgentProtocol` remains an interface, not evidence of that implementation.

The durable first-send permit now fetches a fresh authenticated CSRF token
immediately before its one POST instead of accepting a caller-held token. The
fixed token route preserves the selected context prefix and uses the common
finite-response gate. Only a closed, nonempty, header-safe bounded token document
is accepted; duplicate/surplus fields and invalid values produce no POST. The
token is local to this one attempt and is neither returned nor persisted.
Selected origin rendering now derives from typed scheme/host/nondefault port,
so Referer excludes the context path while request routes retain it. Real-socket
fixtures exercise the authenticated GET/POST sequence and malformed-token
refusals under an `/aem` prefix. Recovery-authorized resends and terminal receipt
coordination remain next; this change supplies the required per-attempt token
step for those paths rather than granting any new resend permission.

Capability discovery now performs an authenticated fixed-route GET over the
selected finite transport. Its closed bounded decoder requires the version,
nonzero generation, transport/canonical digests, command contracts, and authority
readiness; duplicate command names or document members refuse. The comparison
uses this build's installed selected command contract, not caller-supplied
digests. Durable first-send and retained-lookup paths recheck the stored
generation before token/POST or operation lookup. Socket fixtures verify the
context-prefixed route, credentials, compatible response, changed generation,
and unready authority. A pending child's earlier durable admission still does
not prove remote acceptance. Startup discovery before initial child derivation,
and concrete protocol composition remain
to integrate before declaring this task complete.

The connector policy review corrected bracketed IPv6 literals being passed to
socket resolution and TLS server-name parsing. Those APIs now receive the bare
literal while the HTTP Host and normalized origin retain URI brackets. A real
IPv6 loopback fixture exercises the context-preserving request and authority.
TLS configuration now explicitly selects the workspace's ring provider and
TLS 1.2/1.3, rather than consulting process-global provider/default selection;
the selected frozen roots remain the only trust source. This does not replace
the remaining full trusted-peer/hostile-peer TLS conformance scenarios.

Capability wire shape now lives in `slingshot-agent-protocol` with a closed
language-neutral schema and exact-byte digest in the schema inventory. Tests
compare serialized members with required schema members, check missing/surplus
member refusal and redacted debug output, and verify canonical schema bytes.
The connection decoder also rejects malformed unselected command entries rather
than accepting a valid selected entry alongside invalid advertised contracts.
Oversized documents refuse before deserialization. The focused capability and
identity/schema suites pass (18 tests); startup and complete protocol composition
are not claimed by these tests.

The product `submit_initial` wrapper now performs selected, installed-contract
and expected-generation capability validation before admitting a previously
absent remote child. It advances the admission timestamp by the measured
preflight duration with checked arithmetic. Existing children skip this new
admission preflight and remain lookup-only after exact-byte comparison. The
independent send-time capability check remains, so admission is not a cached
readiness grant. The durable-idempotency, operation-submission and executor
regression suites pass (29 tests). These suites do not yet provide a full socket
plus database crash test of this new wrapper ordering; that remains part of the
product composition conformance work, along with initial generation derivation.

The selected-submission admission integration fixture now drives the actual
wrapper through a profile-selected authenticated loopback author and file-backed
SQLite. A complete incompatible-generation capability response creates no remote
child and triggers no token/POST request. A dropped initial permit followed by
database close/reopen remains lookup-only through the product wrapper, retains
the exact stored record, and opens no socket. This closes those two ordering
checks (11 integration/durable tests pass), not the still-required successful
POST/acknowledgement and all crash-boundary composition scenarios. Initial
generation derivation and the concrete protocol implementation remain work.

The same selected-admission integration test now drives the complete successful
capability/pre-admission, capability/pre-send, fresh CSRF, POST and acknowledged
physical-association sequence. An independent SQLite connection observes no
child at the first capability request and the exact persisted wire bytes at all
later stages, including before the POST is answered. The accepted result is
returned with durable physical-job association and remaining retention. A second
scenario truncates the acknowledgement after POST: the wrapper returns uncertain
submission with no physical association and does not retry. Reopening storage
after either scenario preserves the exact record and the wrapper issues no new
request. These tests cover the initial handoff, not terminal results, artifacts,
event supervision, initial generation derivation, or the complete product.

Lookup review found that a local revision could move during the network request
while active-snapshot persistence guarded only the remote child's observation.
The product lookup now uses a storage entry point that reads the owning local
operation inside the same immediate transaction as physical association,
observation, retention and watermark writes. Missing/terminal local work,
selected-revision mismatch or stale local revision refuses before those writes;
the local and remote records must share the database. A separate-connection
regression proves a local recovery revision advance refuses unchanged, while
the current revision permits reconciliation. Terminal receipt coordination and
the full lookup socket/revision-race composition remain separate work.

The selected socket/database integration fixture now exercises the actual
retained-lookup coordinator for both an acknowledged child and a child whose
POST acknowledgement was truncated. After receiving the lookup request, the
peer drives a second database connection to advance the local recovery revision
before sending a complete valid active snapshot. The late response refuses and
leaves the child and physical set byte-for-byte unchanged. Repeating capability
and lookup under the current local revision succeeds and advances observation
and watermark without any POST. This verifies the in-flight local revision
guard through the product coordinator; terminal receipt coordination, events,
results/artifacts and complete runtime composition remain unfinished.

Validated retired lookup receipts now settle the local operation under its
expected revision after rechecking every submission echo and installed
derivation. Pre-truth retirement records RecoveryWindowExpired with
FailClosedIndeterminate/RemoteOutcomeUnknown; prior AuthoritativeRemoteSuccess
instead records ResultUnavailable without retracting success. The socket/database
fixture exercises both branches and proves terminal re-entry opens no socket.
The integration plus executor/submission suites pass (20 tests). The remote child
is retained as evidence; its terminal cleanup coordination, successful/failed
terminal snapshot result decoding, events, artifacts and full composition remain
to implement before task completion.

The retirement review added an independent local-command binding shared by
lookup preflight and retirement settlement. A transactionally read local command
must match the exact canonical submission bytes, command wire name, recomputed
fingerprint (including semantic version and selected revision), and installed
daemon runtime contract. Remote self-consistency alone is insufficient. The
integration fixture now supplies a correctly rederived different command and
matching retirement echoes under the same operation identity: both entry points
refuse without local mutation or network access. The valid socket flows and
operation-submission suite still pass (10 tests). Full runtime composition and
the remaining terminal result/cleanup paths are still outstanding.

Initial product submission now uses the same independent local-command binding
as lookup and requires an explicit expected local revision. It refuses missing,
terminal, mismatched-command/runtime or stale-revision local work before any
capability request or remote child admission, and rereads after capability
preflight. The socket fixture now admits the local operations before handoff
and proves changed command bytes/stale revision create no child and open no
socket. All complete handoff/recovery scenarios and durable-idempotency tests
pass (11 tests). These checks are not a scheduler claim or a cross-transaction
execution fence; product runtime claim composition and remaining terminal paths
are still required.

The consumed first-send permit now checks local command/revision state itself,
and the selected submission transport offers a final caller preflight after
fresh-token acquisition and before opening the POST connection. The product
permit uses it to reject changes during capability/token work. A socket fixture
advances the local revision from a second connection while answering the token
GET: no POST follows, the pending child remains, and no physical association is
created. This closes that asynchronous preflight interval but is explicitly not
a lease spanning connection/write; scheduler/fence composition remains required.

Diagnostic review removed derived Debug from both the in-memory Submission and
stored AgentSubmission, which otherwise printed canonical private command bytes
and operation identity. Their compact and alternate debug forms now redact the
entire value. Tests pin redaction independently from exact-byte wire cloning and
database round trips; persistence and serialization retain the request unchanged.
This addresses those request-bearing values, not a complete diagnostics audit of
the unfinished product runtime.

The retained-lookup path now preserves a validated monotonic Succeeded snapshot
as local ResultAcquisition recovery with AuthoritativeRemoteSuccess rather than
discarding it because result payload transport is not yet implemented. It does
not settle a successful local result or invent payload bytes. Repeated success
proof leaves an existing success-acquisition schedule unchanged. The socket
fixture now obtains success from a real snapshot before testing post-success
retirement, replacing its earlier direct recovery injection; integration and
operation-submission suites pass (10 tests). Review identified the next required
step: atomically guard the remote observation together with this local recovery
write against concurrent event folding. The current local revision CAS alone
does not prove that cross-record concurrency requirement. Terminal-result wire
payloads and complete composition are also unfinished.

The success-evidence concurrency follow-up now uses a storage operation that
compares the complete retained remote child and local operation revision in the
same immediate transaction as the local fact write. A missing, changed or ended
child refuses without mutation. The socket fixture advances the remote snapshot
watermark from a separate connection while the success response is in flight;
local success is not recorded from that stale read. The next lookup succeeds,
and a repeated success proof does not reset acquisition. The full storage suite
and selected socket integration pass. This guards evidence recording; it does
not yet implement terminal-result payload transport or remote-child cleanup.

Retirement settlement now uses that same cross-record transaction guard and
requires the retained request bytes to match the supplied submission. The
socket fixture changes the remote watermark while a tombstone response is in
flight, for both pre-truth and post-success retirement: local settlement refuses
unchanged. A fresh lookup then settles with the appropriate outcome distinction.
Integration, operation-submission and executor tests pass (20 tests), along with
the daemon compile check. Remote-child cleanup and full protocol composition
remain required.

Retirement cleanup now recognizes a settled local RecoveryWindowExpired or
ResultUnavailable owner without assigning an invented remote Succeeded/Failed
state. Eligibility respects both submission age and local settlement age and
matches the selected revision. The conditional delete rechecks the local outcome
inside its transaction. Maintenance preview carries that local disposition when
no remote terminal disposition exists; apply removes remote children before
local owners in the same transaction so the proof remains available. The full
storage suite passes. Socket integration exercises cutoff refusal, direct child
cleanup preserving the local outcome, and reviewed combined cleanup. No user
data was removed by this work; cleanup ran only against temporary test databases.
Full runtime composition and terminal-result payload transport remain unfinished.

Terminal-result envelope decoding now bounds the incoming document before
parsing, refuses unknown/duplicate/missing fields, checks expected provenance,
command name and submission digest, and validates exact canonical inline bytes
within the transport limit. It preserves those bytes and redacts both decoded
documents and legacy metadata-check results in diagnostics. Fourteen structured
result tests pass, including limit boundaries and malformed envelopes. Review
also corrected misleading legacy validation documentation: neither this decoder
nor the metadata helper proves schema, ordered-array or typed request correlation
validation. They cannot authorize settlement on their own. The concrete protocol
adapter and terminal-result transport remain implementation work, not an external
configuration blocker; task 7501 remains in progress.

The command-bound result decoder now verifies the expectation against the
installed contracts, applies transport and command-specific byte limits,
checks canonical arrays from the authenticated inventory, validates the closed
Draft 2020-12 result schema, converts to the catalog result type, and correlates
it with the retained command before artifact-metadata checks. The schema
validator uses no HTTP/file resolution features. Sixteen focused tests cover
this path, including schema-only null refusal, array ordering, typed path
grammar, another request's result, and forged matching expectations; all 64
installed result schemas compile. The full connection regression suite passes.
Artifact identity/content validation and transactional settlement still remain
separate gates; this helper is not yet the concrete network protocol adapter.

The schema review exposed an earlier task 6402 follow-up: generated create-page
arguments used the corrected 1024-byte title limit, while the committed schema
still advertised 65536. The schema, digest manifest, catalog fixture and public
compatibility snapshot now reflect the already-implemented correction. This is
not a new permissive schema change or an upgrade of retained submissions. The
domain schema and catalog suites now pass all 26 tests, including exact-byte
regeneration and manifest checks, which previously failed on this mismatch.

Artifact-result review found that the legacy metadata helper chose load's
inline/artifact branch using the whole result envelope length. The command-bound
decoder now relies on the typed result's logical-content checks instead, and
compares the envelope artifact echoes exactly with the typed descriptor. It
rejects omitted, duplicate, or changed length/media/slot/suggested-name echoes.
Both valid load-artifact and package results pass real schema/typed decoding,
and the descriptor's identifier and content digest are retained for subsequent
identity/transfer checks rather than discarded. All 17 structured-result tests
and the daemon compile check pass. This fixes the decoder's artifact branch;
comparison with the deterministically derived retained artifact identity and
full network composition remain unfinished.

The daemon now has a byte-decoding gate that compares the typed remote artifact
identifier with the artifact store's existing deterministic derivation from
installation, selected target digest, local operation and command-declared slot.
It accepts no caller-provided replacement artifact identifier and performs no
transfer or state mutation. A real canonical package-result fixture verifies
acceptance and refusal after installation, target, operation or returned-ID
substitution, with opaque diagnostics. The bound-result test and the existing
author conformance/executor regressions pass. The runtime still needs to invoke
this gate with the retained command and recomputed submission expectation under
its durable observation/revision guards; this helper alone is not settlement
authority or a complete product protocol implementation.

Result decoding now has a durable-owner entry point: it checks the local
revision and nonterminal state, verifies retained canonical arguments and their
fingerprint/runtime contract, recomputes submission derivation, constructs the
typed command from those retained bytes, and obtains the installation identity
from the stored owner before artifact identity validation. It accepts no loose
caller-selected command, installation or result expectation. The SQLite reopen
test proves absent-owner, revision and both unrecomputed/recomputed argument
substitution refusal with unchanged storage. Both bound-result tests and the
daemon compile check pass. This remains read-only: remote-child comparison and
the local revision guard must still be applied atomically at result publication,
and concrete network/runtime composition remains unfinished.

Successful publication now offers a retained-agent transaction guard. It reads
and compares the complete remote child, rejects an ended/missing/changed child,
checks the selected revision against the local owner, and applies the existing
local revision/lifecycle CAS before any artifact associations or result writes.
All checks and the complete local success use one immediate transaction. The
SQLite result fixture advances the remote watermark on a separate connection,
proves stale remote/local refusal leaves the owner unchanged, then publishes
from a fresh record and refuses replay. The two bound-result tests and all 30
operation-repository regressions pass. This is a storage primitive, not remote
success evidence: the concrete protocol coordinator must still join validated
terminal payloads, artifact completion and this guarded publication path.

Publication rollback review now injects a database refusal at the final owner
update after artifact blob/association inserts. The test proves both inserts
roll back, the owner's prior recovery and lifecycle remain intact, and the
retained remote record remains unchanged. It additionally refuses missing-child
and changed-command-digest expectations. Removing the fault commits both artifact
rows and clears recovery together with success. All 26 agent-job tests and the
full storage regression suite pass. This validates the transaction primitive;
it does not establish completion of the still-unwired protocol coordinator.

The inline/no-artifact result coordinator now joins retained command decoding,
exact remote-child provenance/request binding, persisted authoritative remote
success, and guarded local publication. Payload bytes without that success
evidence refuse. Both reads of local evidence are pinned to the requested
revision, and publication rechecks it with the complete remote child in its
transaction. Invalid/cross-request payloads and stale local/remote observations
leave recovery untouched; a current valid inline result commits exactly once
and clears recovery. A schema-valid result requiring local externalization
returns pending without publication or recovery loss. The bound-result and
executor suites pass all 12 tests. Remote-artifact completion, externalization,
terminal payload acquisition and full concrete protocol composition remain
required before task 7501 is complete.

Externalization review found that artifact reservation counts and their insert
were separate autocommit operations. They now run under one immediate SQLite
transaction, preventing already-open connections from reserving the same
remaining bytes. The previous "contending" test was sequential; an additional
eight-thread/eight-connection test synchronizes actual reservation attempts and
admits exactly the two reservations the budget covers. All 19 capacity tests
pass. Startup database opening still performs abandoned-reservation cleanup;
the unfinished runtime composition must distinguish that startup-only recovery
from live account access. Structured-result externalization remains to wire.

Reservation ownership review found copyable public tickets, allowing a stale
copy to release a later reservation if SQLite reused the ticket. Reservations
are now non-copyable/non-cloneable guards with private tickets and a borrow of
their originating database. Drop releases that reservation on its own connection;
explicit release/commit consume the same guard. An error conservatively retains
capacity until startup reconciliation. Tests cover cross-database release,
drop/replacement ownership, simultaneous held reservations, crash-style abandoned
rows, and compile-time refusal of double consumption. All 20 capacity tests,
the full storage suite and daemon compile check pass. These ownership fixes are
prerequisites; artifact externalization and full runtime composition remain open.

Live database connections now have an explicit `open_live` path that preserves
active artifact reservations and refuses missing/outdated databases instead of
running startup migrations or abandoned-reservation cleanup. It retains the
existing pinned-path, physical-budget, settings and authorizer checks; callers
must already hold daemon namespace ownership. The live-open test proves that a
missing file is not created, an existing reservation survives another connection,
both accounts share the aggregate bound, and dropping either guard releases only
its own capacity. All 21 capacity tests and the full storage suite pass. The
startup path still reconciles crash-abandoned reservations. Runtime composition
must select these paths at the appropriate lifecycle points; externalization's
filesystem/database handoff and the concrete protocol adapter remain unfinished.

Live-open refusal review moved the current-schema requirement into the read-only
preflight, before mutable open and settings application. An old rollback-journal
database now refuses with exact database bytes, journal mode and user version
unchanged, and without creating WAL/shared-memory files. The later version check
remains as a recheck. All 22 capacity tests and the full storage suite pass.
This closes the live-open review follow-up; externalization and concrete
protocol composition are still required.

Submission review found that the selected transport checked installed provenance
and digest derivation but not the argument contract. Its preflight now checks
raw canonical bytes, authenticated argument-array ordering, the installed
Draft 2020-12 schema, typed command conversion and request usability before
network access. The socket fixtures now use valid query arguments instead of
empty or unrelated objects. Tests refuse missing/surplus members, whitespace,
typed-invalid paths and schema-invalid null options, while preserving canonical
Unicode bytes through the actual POST. All 13 selected-author environment tests
and the durable admission/restart socket integration pass; the final focused
submission rerun also passes. This hardens the real send path but does not
complete externalization or the product protocol composition.

The broader argument review added ordered/duplicate package-root cases and a
schema-valid, typed but unusable move into its own subtree. It exposed a pinned
inventory defect: package roots are strings, but `/roots` named the comparator
for objects carrying `repository_path`. The inventory now uses the existing
`utf8_ascending_unique` string-set comparator. Its authenticated digest is now
`b493d188f04f13f130649f0c2ce18230ad4dc9eeec8e158ae6172c49c285e8ef`;
all 128 annotated role schemas and their manifest/catalog/compatibility digests
were regenerated accordingly. Old-digest retained records are not relabelled,
and peers must advertise the corrected installed contracts. Both argument-gate
tests, 26 schema/catalog tests, 10 wire-contract tests, four compatibility tests,
17 structured-result tests, two durable-result tests and the real admission/
restart integration pass. Task 7501 still requires externalization, artifact
transfer and full product protocol composition.

The full domain and agent-connection suites now pass after the canonical
inventory correction. Capacity-reuse review also found that a known content
digest skipped reservation without comparing the supplied length to the stored
blob length. Conflicting lengths now refuse opaquely inside the reservation
transaction; tests prove committed and reserved usage remain unchanged. All 22
capacity tests and the daemon compile check pass. These broader regressions do
not prove the remaining externalization/transport/composition requirements.

Artifact storage now offers `install_verified`, which accepts an expected
length/digest and checks them before publishing any digest-named content file.
Expected digest grammar and the individual byte bound are checked before staging;
streaming reads at most the expected length plus an overshoot probe. Short,
oversized or different content removes the private stage and publishes nothing.
The independent SHA-256 `abc` vector tests exact publication, bounded reads,
refusal cleanup and preservation of previously verified content. All 26 artifact
store tests pass, and the daemon compiles. Capacity reservation, command/slot
validation and guarded database publication remain caller prerequisites; wiring
this primitive into complete externalization/remote transfer is still required.

Terminal-result review found that the envelope carried command provenance and
submitted digest but omitted operation context. Identical commands can share a
digest, so the decoder now requires a closed operation echo (generation, remote
operation, target and selected revision) plus the retained subscription. The
daemon builds that expectation from its retained submission and independently
checks the operation derivation at its artifact-binding entry point. Missing
echoes and each changed identity component refuse even with an unchanged command
digest. All 18 structured-result tests, two durable-result tests and the daemon
compile check pass. Terminal payload acquisition, a complete published wire
schema and full runtime composition remain required.

Terminal-result envelopes now live in the shared agent-protocol crate and have
a published closed schema recorded in the job-schema digest manifest. The
connection decoder uses that schema with embedded-only reference resources,
after the outer byte bound and duplicate-rejecting deserialization. Connection
callers retain the existing re-export, so there is no parallel envelope type.
Matching caller expectations no longer make an empty/oversized subscription,
zero generation or invalid artifact metadata acceptable. All 19 structured
result tests, 11 agent-store contract tests and two durable-result tests pass.
This closes the standalone envelope-schema gap, not terminal payload acquisition
through snapshots/events or the concrete product protocol composition. Task 7501
remains in progress; those are implementation work, not a request for user input.

Found snapshots can now carry the shared optional `terminal_result` envelope.
The published snapshot schema permits it only on success; the decoder rejects
explicit null, non-success placement and every mismatched nested operation or
provenance echo. Absence remains an acquisition-pending state. Durable lookup
records authoritative success under the retained-child guard before attempting
the existing command-bound inline publication coordinator. The loopback fixture
now delivers a cross-request result followed by a valid result: the first leaves
remote-success recovery intact and publishes nothing, while the second persists
the exact inline bytes, clears recovery and settles without another POST.
All 54 affected reconciliation, structured-result, protocol, daemon conformance,
durable-result and socket-admission tests pass. Review also moved envelope schema
validation after provenance/identity and raw canonical-byte checks. This is a
working lookup-to-inline-result path, not completion of task 7501: terminal remote
child/physical-set/retention publication, artifact completion, authoritative
failure/event handling and the concrete product protocol adapter remain open.

Successful inline snapshot publication now uses one immediate transaction for
the local result/artifacts/lifecycle/recovery and the remote successful
observation, watermark, physical-job set, terminal disposition and conservative
remaining lifetime. The transaction compares the complete retained child and
local owner revision, refuses forgotten physical attempts, non-success or
regressing observations and integers SQLite cannot represent exactly. A remote
write fault injected after local publication rolls back the local result,
artifact rows and newly inserted physical job. Storage tests also exercise
invalid snapshot inputs and successful publication of the complete graph; the
loopback fixture reopens the database and observes the remote terminal state and
watermark. All storage tests and the durable-result/socket integration tests
pass. This closes atomic publication for the validated inline lookup branch;
pending artifact-result truth/retention, artifact completion, event/failure
handling, scheduler authority and concrete product composition remain required.

Artifact installation now separates verification in a private stage from explicit
digest-file publication. `StagedArtifact` is non-cloneable, exposes measured
metadata read-only and removes only its own inode on drop; existing installation
APIs use this same path. Publication checks the stage identity/length/modification
snapshot and content again, and the Linux no-replace link verifies that same
identity at the final handoff. Tests cover unpublished staging, abandonment,
successful publication, duplicate abandonment, conflicting destinations, changed
content and replacement names that must not be deleted. All 29 artifact-store
tests pass and the daemon compiles. This provides controlled staging ownership,
not a completed file/database commit protocol: durable capacity ownership,
post-publication failure/crash reconciliation and artifact-result coordinator
wiring remain required before claiming complete externalization.

Schema migration 0010 adds producer-specific durable artifact-publication holds.
After staging, the capacity account can atomically record the content byte charge
and publication hold while consuming the exact same-connection reservation;
existing content gets a separate hold without a second byte charge. These holds
survive startup reservation cleanup and participate in maintenance's current
reference check. Pending records are bounded to two per retained-operation budget
(one command artifact plus one structured result), including duplicate producers.
The reservation guard disables its release after committed handoff so ticket
reuse cannot release another reservation. Tests inject a hold-insert failure,
verify rollback, reopen after publication, check maintenance protection and reach
the duplicate-producer row bound. Migration fixtures now require schema 10 and
the new table. All storage tests pass and the daemon compiles. Exact hold
consumption in successful completion, interrupted-publication reconciliation and
the artifact-result coordinator are still required; the new hold is deliberately
not released merely because an in-memory token is dropped.

Lookup now accepts runtime-owned artifact resources for local structured-result
externalization. The coordinator validates retained command/result context,
reserves capacity before staging, persists the durable publication hold, publishes
the exact verified bytes, then commits the artifact association, successful local
and remote state and exact hold consumption together. Capacity-byte refusal
records PersistentCapacityUnavailable with authoritative remote-success evidence;
remote-artifact results remain pending. The durable-result test reads the complete
externalized bytes and verifies committed/reserved/hold accounting. The loopback
lookup fixture now delivers both small and over-inline successful results and
reads the latter through the verified artifact reader without another POST.
The storage review injects failure at final hold deletion and proves rollback of
local/remote success and associations; successful consumption preserves another
producer's hold for the same content. The full storage suite, enhanced 26-test
agent repository suite, durable-result tests and loopback admission test pass.
This proves successful local externalization, not full task completion: failed
publication reconciliation, capacity-resume scheduler authority, remote artifact
transfer, event/failure handling and concrete product composition remain open.

Externalization retries now recover a sole pending publication only when its
artifact identifier, content digest and recorded byte length exactly match the
freshly validated result. A bounded two-row query exposes ambiguity rather than
selecting an arbitrary producer; changed or multiple holds remain protected.
The coordinator re-stages/verifies the bytes but reuses the same durable hold,
so repeated failed handoffs do not accumulate holds or byte charges. The durable
fixture exercises two refused final publications, a deliberately absent content
file rebuilt on retry, a fresh database connection and eventual complete result
publication with zero remaining holds. All 23 capacity, 13 migration and two
durable-result tests pass. This covers recoverable known-result retries, not
automatic cleanup of ambiguous/orphaned holds or authority to resume paused work;
those and the remaining remote-transfer/runtime requirements are still open.

Capacity recovery review found that the shared pause gate considered retry
exhaustion only, allowing a newly capacity-blocked operation at attempt zero to
run again. PersistentCapacityUnavailable now pauses immediately when marked
manually resumable, and the result-completion coordinator applies the same gate
before result/file work. The loopback fixture exercises a real capacity refusal,
verifies nonterminal authoritative remote success and no publication hold, then
proves that lookup emits no request and completion refuses even with available
capacity. The operation remains unchanged. Predicate tests retain the distinction
between manual eligibility and actual retry exhaustion for other categories.
The durable-result and loopback tests pass. Persisted receipt activation and
scheduler/fence authority for an explicitly authorized resume remain to be wired;
this pause enforcement is not proof of the complete resume workflow.

Persisted resume receipts now have a separate guarded activation step. The
daemon checks selected transport/submission, retained command fingerprint and
the category-bound receipt source; storage compares the exact saved receipt,
its quoted local revision and complete remote child in one transaction before
clearing the pause/resetting its retry schedule at the next revision. Replays,
later recovery cycles and terminal owners receive no activation. The loopback
fixture admits a real resume receipt, refuses forged receipt data/stale child
state, activates once, pauses again on another capacity refusal, refuses the old
receipt, then activates a new receipt and completes the known result without a
new POST. A storage-injected activation failure preserves the pause; retry
succeeds and replay through a new connection remains inert. The full storage
suite, enhanced 26-test agent repository suite and loopback test pass. Activation
does not confer a scheduler lease or submission permit; durable wake dispatch,
execution-fence ownership and the remaining concrete runtime composition still
require implementation and conformance proof.

Scheduler follow-up: elapsed retry delay previously made capacity-paused or
retry-exhausted work selectable despite the transport/completion pause gates.
Selection now shares the pause predicate; a zero-delay paused row cannot consume
the last available slot ahead of ready work. An admitted resume remains selectable
for guarded activation, and an activated recovery can run. All seven scheduler
tests pass, including the starvation regression and existing fairness/bound tests.
Scope review confirms transactionally leased/fenced scheduler claims belong to
task 7504, whose implementation must retain its own task commit. Earlier notes
listing that unfinished work are not an additional prerequisite requiring 7501
to implement 7504. Task 7501 must preserve logical outbox/fence evidence while
finishing the concrete author adapter and remote-artifact conformance paths.

Remote-transfer preparation: artifact storage now exposes an incremental private
writer accepting bounded transport chunks without retaining them. Each append
checks the remaining length before writing/hashing; any refused append permanently
poisons the writer, so ignored errors or later empty chunks cannot make it valid.
Finish verifies exact expected length/digest and synchronizes a still-private
stage; publication and durable capacity ownership remain separate gates. Existing
synchronous install/stage APIs use this same incremental writer, preserving one
verification path. Tests cover every split of an independent digest vector,
abandonment, short/different content, overrun poisoning and refusal cleanup. All
30 artifact-store tests and both durable-result tests pass. The selected HTTP
streaming driver, framing/deadline proof and remote-artifact coordinator still
need wiring; a finished writer alone is not evidence of a complete HTTP transfer.

The selected HTTP/1.1 connector now streams remote artifact bytes through a
bounded 8 KiB sink without collecting the artifact. Local selected-identity,
slot, digest grammar and length bounds precede connection work. The driver
builds its own selected-context route and checks the shared response head,
exact media type, framing, idle/total deadlines, expected length, SHA-256 and
socket EOF before returning an opaque completion receipt. A sink error or
cancelled future yields no receipt; a final elapsed-time check also refuses a
synchronous sink that outlasts the async timeout (it does not preempt that sink).
Real-socket tests cover both framings, transfers exceeding the finite JSON-body
limit with bounded peer/sink memory, invalid local identity/slot before network,
short/different/surplus content, length mismatch, compression, trailers, chunk
overrun, sink refusal and partial-transfer cancellation. This is transport
proof, not command-manifest or publication proof. The remote-artifact
coordinator must still bind the validated terminal manifest to private staging,
capacity ownership and atomic completion; typed artifact-unavailable responses,
chunk extensions and HTTP/2 remain unfinished.

Remote-transfer staging now composes the selected socket reader with the actual
incremental private file writer and SQLite capacity account. It checks local
request identity/slot/media agreement, reserves before file/network work, waits
for the complete transport receipt, finishes storage verification, and returns
the private stage with a durable publication hold. Transfer/staging failures or
cancellation remove the private file and release the transient reservation.
A repeat transfer reuses the matching durable hold; dropping its stage does not
publish bytes or erase the hold, and reopening SQLite preserves that charge.
Real socket/SQLite tests assert empty files and zero reserved/committed bytes
after corrupt, short, surplus and cancelled responses, no network on capacity
refusal, and one three-byte hold across retries/reopen. This helper is not yet
invoked by retained-result completion: its caller must supply terminal-manifest
validation, loaded-content semantic verification, recovery classification and
atomic operation settlement. These obligations remain part of task 7501.

Loaded-document review found that the typed inline result previously checked
only its outer echoed path. Shared request-relative document validation now
also checks the document root, immediate-parent relationships, duplicate child
paths, requested depth, and refusal of truncation flags above the depth boundary.
Inline result acceptance invokes this check. A byte-bounded document decoder
combines canonical-byte validation and closed typed decoding with that same
structural check for the future downloaded-content path. Tests cover a correct
outer echo hiding another document, unrelated/duplicate descendants, excessive
depth, depth-zero boundary, whitespace, unknown and duplicate fields. This is
observable structural validation, not proof of repository completeness; remote
staging still needs to invoke document validation before successful settlement,
and child ordering/remaining schema constraints require their own full gate.

The shared loaded-tree check now enforces the contract's child order by name
bytes and numeric same-name-sibling index (including `[2]` before `[10]`), and
validates property names with the repository-name type. Strict ordering also
refuses duplicates without a second per-parent set. Regression coverage drives
the actual terminal-result decoder with an unchanged valid outer path and
mutated document root, parent, order, depth and truncation flags; these cannot
reach validated result evidence. This closes those structural follow-ups, not
the still-unwired streaming document verification and result settlement path.

Private stages now expose a verified reader without exposing their path or
creating a digest-addressed object. Opening first checks the original stage
handle identity; private and published content then share the same full first
digest pass, rewind and second-pass reader/finish verification. The first pass
also checks handle stability before returning the reader. Tests prove private
readability without public addressability, refusal of partial validation and
same-length mutation, and cleanup after refused publication. All 31 artifact
store tests pass. This supplies private bytes to semantic validation; callers
must still finish the reader and validate the loaded document before settlement.

Loaded staging now invokes a bounded incremental canonical JSON reader on its
private verified handle before creating/reusing a publication hold. It refuses
the load byte maximum before capacity or network work; the reader bounds total
bytes, scalar-token spelling and container nesting, checks unique ascending
object keys, canonical scalar spellings and exact EOF, and retains no complete
array/object value tree. Scalar spelling delegates to the existing canonical
codec. The verified artifact reader must also finish successfully. Four tests
cover partitioned agreement with the existing codec, inclusive limits, invalid
UTF-8/I/O failures and a million-item array generated without a backing input
buffer. Real-socket staging tests additionally reject correct-digest loaded
responses containing whitespace or duplicate/unordered keys, with zero files,
reservations and publication holds afterward. Document schema and retained
request semantics still require an incremental pass before publication, and
the complete remote-result settlement coordinator is not yet installed.

Loaded staging now also runs an incremental closed-document pass before its
publication hold. The pass retains only current scalar/ancestor path-order
state, not child or multivalue arrays; it checks exact ordered resource/property
fields, canonical strings, repository names/paths, numeric sibling ordering,
root/parent/depth/truncation relationships, the twelve declared JCR types and
single/multiple cardinality. Individual scalars use the existing typed JCR
validators. The selected load submission supplies path/depth, and the private
reader must finish after this pass as well. Every committed property vector and
mutated tree is checked by domain regressions. Real socket/SQLite tests now use
a load-command submission and cover valid loaded content, wrong-root or empty
document shapes, and prior canonical-byte failures; refused stages leave no
file, transient charge or publication hold. These tests exercise staging, not
terminal artifact-disposition/retained-owner binding or atomic result settlement;
the latter remain the next coordinator obligations in task 7501.

Retained lookup with artifact resources now invokes the asynchronous completion
coordinator. It validates the retained logical result and full child/revision/
remote-success guards before deriving the artifact request, reserving optional
local-result space, streaming/verifying remote content, publishing verified
stages, and atomically committing associations, logical result, local success,
remote snapshot and publication-hold consumption. Capacity refusal preserves
remote success and records manual capacity recovery. Local-only completion
returns directly so a capacity-pause revision is not revalidated as stale.
Review exposed an older settlement-model mismatch: inline logical descriptors
must coexist with downloaded command artifacts. Only inline bytes combined
with the `structured_result` fallback are mixed result representations; the
domain now makes this distinction, retaining the machine inline-byte bound.

A retained package coordinator test uses a real selected-author GET and SQLite:
stale entry revision issues no request; wrong digest leaves no content/charge
and preserves remote-success recovery; a revision change during transfer leaves
one durable publication hold but no local/remote settlement; retry reuses and
consumes that hold and commits the exact inline descriptor plus artifact and
successful snapshot. The fixture injects initial persisted remote success, so
it is not proof of the whole lookup-to-artifact network sequence. Domain
lifecycle tests, full storage tests, durable-result tests and selected admission
regressions pass. Remaining conformance includes that full lookup sequence,
loaded-result completion/restart/capacity cases, elapsed retention accounting,
typed unavailable responses and complete concrete runtime composition.

The retained-artifact conformance fixture now covers both package output and a
loaded JSON document above the 262,144-byte agent-inline threshold. Each checks
digest refusal, a concurrent revision change, durable hold preservation and
retry through reopened operation/remote-child repositories. The final retry
uses the actual selected capability GET, logical lookup with terminal result,
and derived artifact GET through `lookup_retained_operation_with_completion`;
it verifies exact published bytes, unchanged inline logical result, local and
remote success and consumed hold. Initial success is injected only to exercise
the earlier coordinator race; the final recovery obtains a fresh socket-backed
successful lookup. A separate branch for each artifact type proves capacity
refusal precedes network/file creation, retains authoritative success and manual
pause, and remains paused even when capacity becomes available. All three
retained-artifact socket tests pass. This supplies the missing lookup-to-artifact,
loaded-result, repository-reopen and capacity evidence; full process composition,
elapsed retention accounting and typed unavailable conformance remain open.

Remote-artifact completion now measures its validation/transfer/publication
interval monotonically, rounds elapsed time upward to milliseconds, deducts
it from the successful snapshot's remaining author lifetime with saturation,
and advances the settlement timestamp by that interval. Successful local
settlement permits zero remaining remote lifetime: already verified local
bytes do not become unusable because upstream retention expired, and the
daemon must not invent a positive residual lifetime. This change is confined
to the guarded successful-result transaction, not wire response acceptance or
active recovery. Delayed package/load socket tests prove transfer time is
deducted; the atomic publication/rollback storage test now also proves success
with zero remote lifetime and no loss of result/hold-consumption atomicity.
Failure/retry and local-only elapsed accounting still need the broader timing
conformance review; this is not a claim of complete transport conformance.

Successful inline and local structured-result externalization now use the same
conservative snapshot-aging function as remote artifacts. Validation before
branch selection is accounted before delegating to local completion; subsequent
inline validation or local staging/publication accounts its own elapsed phase.
Pure boundary tests cover no elapsed time, sub-millisecond rounding, exact
milliseconds, retention equality/expiry and integer saturation while preserving
the observation/physical set. The durable externalization test reads back a
reduced upstream lifetime, and all five selected socket admission/completion
tests pass. Failed/cancelled recovery timing and the remaining unavailable/
runtime-composition conformance are still unfinished.

Artifact-unavailable review found the legacy policy input compares a content
digest, while the wire contract requires deterministic ArtifactIdentifier.
The shared protocol now declares a closed, redacted unavailable document with
retained provenance, generation, operation, artifact identifier, slot and reason;
its published schema is covered by the schema-integrity manifest. A bounded
decoder independently checks installed provenance, exact expected identities,
and 404/missing versus 410/retention_expired before producing opaque validated
evidence. Tests reject swapped status/reason, identifiers/slot/generation,
transport/canonical/role drift, digest substitution as an unknown field,
duplicates, truncation and oversized bodies. The decoder and all 11 logical
execution/schema contract tests pass. The legacy metadata helper is explicitly
not wire evidence. HTTP error-body acquisition and durable unavailable/grace
classification are still the next integration work, not completed by this codec.

The selected HTTP/1.1 artifact exchange now returns either a verified transfer
receipt or identity-checked unavailable evidence with elapsed request time.
404/410 bodies use the same bounded Content-Length/chunked finite reader as
other finite requests, with total/idle deadlines, strict JSON media, trailer
and surplus-byte refusal and EOF proof before decoding. Offered Location values
are refused even on unavailable responses. Error bodies never reach the artifact
sink. The success-only compatibility method delegates to the same exchange and
still cannot treat a non-200 as successful artifact bytes. Staging supplies its
locally derived artifact identifier and carries verified unavailable evidence
as a separate refusal, releasing its private file and transient reservation.
Socket tests cover valid 404/410, wrong reason/status/identity/media, truncation,
surplus bytes, trailers and offered locations. All 14 selected connection tests,
the additional location regression and all five daemon socket tests pass.
The completion owner still preserves remote success on this refusal; durable
grace versus ResultUnavailable classification remains to be connected.

Verified RetentionExpired evidence now reaches the retained completion owner's
guarded terminal transaction and records ResultUnavailable with
AuthoritativeRemoteSuccess, without changing remote success into nonexecution.
The timestamp includes elapsed acquisition work. Package and above-inline-load
socket tests prove wrong artifact identity cannot end recovery, Missing remains
recoverable, matching retirement ends it, private files/charges/holds are absent,
and a later invocation performs no request. All four retained-artifact tests
pass. Missing cannot yet expire: a dedicated restart-stable grace anchor is
still required; using each retry's request time would incorrectly reset grace,
and using the original command POST time could expire it before result
acquisition began. That persistence/classification remains unfinished.

Missing-artifact grace now has a durable first-acquisition anchor on the retained
child. Its identifier, slot, digest and start form a validated all-or-nothing
tuple; the guarded transaction checks the exact child, local revision, selected
revision and authoritative remote success before creating or reusing it.
Capacity admission and private staging precede the anchor, which precedes the
GET. Retries cannot refresh the start or substitute a different artifact.
Verified Missing within grace records bounded jittered recovery and respects
the automatic-attempt cap; Missing at or beyond the saved deadline records
ResultUnavailable while preserving AuthoritativeRemoteSuccess. Verified
RetentionExpired still settles immediately. Package/load socket tests reopen
repositories between retries, reject stale revisions and artifact drift, and
exercise expiry of the original anchor. The complete storage suite, including
the new tuple-constraint test, passes. This closes this persistence gap, not
the remaining concrete protocol-adapter and full conformance work.

The HTTP/1.1 chunk-extension follow-up is implemented in the shared finite and
artifact body paths. The allocation-free size-line parser validates RFC 9112
section 7.1.1 token/quoted-string syntax, escaped characters and allowed bad
whitespace, ignores extension metadata, and refuses integer overflow or
malformed extensions even on the terminating chunk. The existing incremental
line bound includes extension bytes and CRLF; body limits, idle/total deadlines,
trailer refusal, EOF proof and artifact digest verification remain unchanged.
Two grammar tests and all 14 selected-connection socket tests pass, including
valid finite/artifact extensions, invalid final extensions, and exact-inclusive
versus oversized line bounds. Concrete protocol composition, terminal-failure
decoding and remaining transport/event conformance are still unfinished.
The subsequent complete `slingshot-agent-connection` test suite also passes.

Executor integration review found that `AgentSettlement::Outstanding` could
express only execution certainty, not AuthoritativeRemoteSuccess, and rebuilt
recovery with zero attempts/delay/anchor and a different manual-resume decision.
It now carries the complete RecoveryFact without reinterpretation. A separate
terminal settlement carries the complete TerminalFailure, preserving
ResultUnavailable/AuthoritativeRemoteSuccess and the distinct remote-failure,
proven-nonexecution and fail-closed dispositions. Executor tests assert exact
field preservation and that neither branch invokes artifact completion; all
12 executor tests pass. This repairs the settlement boundary needed by the
concrete protocol implementation, which is still not installed or complete.
The seven existing author-conformance tests, two bound-result tests and six
selected-admission socket tests also pass after this change.

Retained-child handoff now has a lookup-only disposition. Previously the
durable submission coordinator's LookupRequired instruction became generic
Unknown, causing the executor to return before reaching reconciliation on
every re-entry. ReconcileRetained advances to settlement without asserting
acceptance or granting another send. Fresh transport ambiguity and RetryAfter
still return recovery rather than bypassing scheduling. The disposition matrix
and executor tests cover this distinction; the selected socket test verifies
the actual reopened-child result permits lookup but no resend and leaves the
listener untouched. All 13 executor, 10 durable-submission, six disruption and
six selected-admission tests pass. This fixes another composition prerequisite;
it does not establish a complete concrete protocol adapter or scheduler claim.

Artifact completion now uses a typed Published/Recovery/Unavailable decision
at both author-port boundaries instead of reducing every DownloadRefusal to
a fresh generic transfer retry. Recovery carries the exact saved fact;
Unavailable maps only to ResultUnavailable/AuthoritativeRemoteSuccess and
does not expose the partial inline result. Tests prove saved retry fields and
capacity pauses survive, unavailability never retracts remote success, and
only verified publication produces an executor success. All 15 executor tests
and seven existing author-conformance tests pass. This completes the outcome
vocabulary needed for durable completion composition, not its concrete wiring.
All six selected-admission socket regressions also pass after this change.

`RetainedAuthorProtocol` is now the first concrete AuthorAgentProtocol
implementation. It binds one invocation to the admitted command, derived
submission and runtime-owned repositories/authentication/artifact resources,
and uses only the supplied selected transport. Submission delegates to the
single-permit durable coordinator; settlement delegates to lookup/completion
and rereads durable state even after refusal so an already recorded remote
success is not lost. Saved pause and retry-time decisions prevent network work.
Completed artifacts are read from durable associations, checked against local
deterministic identities and the typed retained result, and verified on disk;
changed content yields acquisition recovery without retracting remote success.
The package and above-inline-load socket scenarios now execute through real
ProductAuthorPorts and AuthorAgentOperationExecutor, not scripted network ports.
They prove reopen/lookup, transfer and atomic success through this composition;
capacity-pause and post-publication corruption regressions also pass. The 15
executor, seven existing conformance, two bound-result and six selected socket
tests pass. This is not full task completion: startup construction, shared
resource-owner validation, initial-handoff coverage through the concrete port,
terminal-failure codecs and remaining event/transport conformance still need
implementation/review before the adapter is considered complete.

Concrete-protocol resource review now rejects cross-database composition.
OperationDatabase retains its opened main-file identity and compares both
connections against their still-bound private files, without exposing paths or
identities through the comparison API. Separate live connections to the same
file are accepted; different files, distinct in-memory databases, missing paths
and platforms without stable identity fail closed. Capacity accounting uses
the same check. Protocol construction and subsequent invocation reads require
the operation repository, remote-child repository and capacity account to
share that object. Tests cover matching/different objects, pathname movement,
mixed protocol resources before network access, and same-database product
execution. The full storage suite and all 15 executor/six selected socket tests
pass. Artifact-root/startup namespace composition and the previously listed
initial-handoff/terminal-failure/event conformance remain to review.

Initial-handoff coverage now uses ProductAuthorPorts with RetainedAuthorProtocol
instead of calling the durable coordinator directly. The selected socket fake
checks capability-before-admission, persisted exact submission bytes before
CSRF/POST, the one-use CSRF token, authorization, selected context path and
idempotency key. A different typed command refuses before network access.
Accepted and truncated acknowledgements retain their distinct handoff outcomes;
a concurrent local revision change during CSRF prevents the POST and leaves
the pending child recoverable. Reopening the remote repository and constructing
a fresh concrete protocol/port yields ReconcileRetained and no request, for
both acknowledged and lost-ack children. The first concrete initial-handoff
socket run passes; remaining startup and terminal/event conformance is unchanged.
All six selected socket, 15 executor and 10 durable-idempotency tests pass in
the subsequent combined run, including the recreated-port restart assertions.

Failed capability/lookup exchanges now consume the durable automatic recovery
budget instead of leaving the concrete adapter's old recovery fact unchanged.
The update runs only after retained local/remote binding validation and uses
the exact retained-child/local-revision guarded transaction, so a concurrent
newer decision cannot be overwritten. It preserves existing execution evidence,
keeps post-success work in ResultAcquisition, records elapsed request time,
uses the installed jitter ceiling and pauses at the automatic-attempt cap.
A concrete socket regression reopens the repository between failed capability
exchanges, verifies every saved attempt/delay/evidence field, and proves the
paused invocation issues no further request. This covers failed preflight/lookup
exchanges, not the still-unfinished terminal-failure/event protocol branches.
All 15 executor, two bound-result and seven selected socket tests pass after
the retry-accounting change.

The failed-exchange review now includes an in-flight competing local update.
The fake advances the operation revision through a separate live connection
before returning the failed capability response. The concrete protocol retains
the old recovery evidence/attempt/timestamp without overwriting that newer
revision; subsequent uncontended failures still advance the durable budget to
its cap across reopens. Capability and lookup failures also use one shared
accounting closure, eliminating duplicated timing/guard orchestration. The
new race regression passes.
The combined run of all seven selected socket and 15 executor tests passes.

Terminal-failure transport work now has a closed shared envelope and published
schema authenticated by the logical-execution schema manifest. It carries exact
retained operation/subscription/provenance/digest fields and preserves the raw
canonical failure as a string; it cannot carry success artifacts. The bounded
decoder checks retained echoes, canonical bytes and the embedded envelope
schema before returning a redacted document. This remains an untrusted command
failure: the decoder deliberately grants no terminal/effect disposition until
the selected typed refusal and request-correlation checks run. Tests cover
identity/generation, all command-contract fields, missing/extra/duplicate fields,
noncanonical payloads, truncation, oversized bodies and redaction. The initial
decoder test, all 20 structured-result tests and 11 logical-execution/schema
tests pass. Snapshot integration and typed failure settlement remain unfinished.

Snapshots can now carry an optional closed terminal-failure envelope. Presence
requires Failed kind; explicit null is refused, and nested generation,
operation, target, selected revision, subscription, provenance and command digest
must exactly match the already validated outer snapshot. The published snapshot
schema expresses the same kind restriction and its manifest digest is updated.
Success and failure payloads cannot coexist. This transports untrusted failure
data to its next validation stage, without claiming a command-specific failure
or effect disposition. The 14 snapshot, one failure-envelope, 11 schema/logical
execution and seven existing author-conformance tests pass. Typed refusal
validation and guarded terminal settlement remain to implement.

Creation failure decoding now validates CreatePageRefusal and AddComponentRefusal
against the retained typed command after installed provenance, envelope and
canonical-byte checks. The existing closed enums own category/field acceptance,
computed-target correlation and no-effect evidence; no remote boolean can
substitute for those checks. The validated value exposes only the registered
category and whether no effect is proved, with redacted Debug. Every committed
creation/addition failure vector passes, including mutation_outcome_unknown
remaining distinct from proven nonexecution; changed targets, unregistered
categories and surplus fields refuse. This decoder deliberately covers these
two command families only. Other failure families and the final guarded
snapshot/failure transaction remain unfinished.
The combined 14 snapshot, two failure decoder, seven add-component and nine
create-page tests pass after this change.

Load/package refusal review found missing request-correlation methods in their
existing closed domain types. LoadRefusal now requires every reported location
to lie inside the requested subtree at or above its resolved depth boundary;
budget-only failures retain their closed fieldless-location shape. Package
refusals require an exact requested root, or a valid index in the specifically
named existing filter collection. Descendant roots, other subtrees, prefix
lookalikes, over-depth locations, absent collections and out-of-range/u64-max
indices refuse. All 20 load and 10 package domain tests pass. Connecting these
checks to typed transport failure decoding and terminal settlement remains
unfinished; no broader failure conformance is claimed by these domain checks.

Typed transport decoding now connects the load/package refusal checks to the
retained command and independently installed provenance. Separate opaque load
and package evidence types keep publication uncertainty distinct from mutation
nonexecution; neither grants replay. Every committed load/package refusal
fixture is tested after canonical wire encoding, with wrong command families,
matching-but-uninstalled transport provenance, unrelated locations, invalid
filter indices, unknown categories and surplus members rejected. This review
exposed serde's ignored extra members on internally tagged package unit variants;
the wire gate now compares the received shape with the typed serialized shape
without repairing incoming bytes. All 37 snapshot, structured-result and failure
decoder tests pass. Guarded terminal settlement and remaining command families
are still implementation work within this active task, not external blockers.

Failed lookup snapshots now enter the concrete retained protocol's settlement
path for load, package download, create-page and add-component. Plan 0005's
locally derived mapping records validated no-effect refusals as Rejected with
AuthoritativeNonExecution/ConfirmedNotExecuted, atomically with the failed remote
observation, complete physical set, watermark and terminal marker. The transaction
guards the full retained child, local revision, selected revision, monotonic
observation and prior success evidence. Injected remote SQL failure rolls back
the local terminal write and physical associations together. Lookup also rejects
mixed databases, conflicting equal-sequence observations and forgotten physical
records. Ambiguous mutation/publication outcomes keep the child open and record
bounded OperationLookup recovery with RemoteOutcomeUnknown; no replacement send
is authorized. Invalid or unsupported failures consume the existing lookup budget
without asserting a terminal outcome.

All 27 agent-storage tests and the combined eight selected-author/15 executor
tests passed during review. The expanded failure socket test subsequently passed
through RetainedAuthorProtocol, ProductAuthorPorts and AuthorAgentOperationExecutor
for all four command families, including unknown mutation/publication and an
unrelated load path; it verifies reopened storage and absence of another request.
Other command-family mappings, command-specific maintenance diagnosis (including
package staging cleanup), complete terminal conformance and the remaining task
requirements still need implementation; this is not a task-completion claim.

The package-cleanup follow-up now persists a content-free, bounded maintenance
diagnosis in the same guarded terminal transaction: agent maintenance is required
and package rebuilding is forbidden. The diagnosis comes from a local closed
enum after typed category validation, never from remote free text. The socket
test checks the metadata after reopening storage and re-enters the concrete
executor to prove that the saved terminal failure causes no request or rebuild.

Configuration-inspection refusals now use their installed closed domain enum,
independently installed provenance and exact typed-shape comparison (including
fieldless variants). Every committed configuration failure fixture passes;
private identifiers, keys, values, filters, maps and undeclared budget/reason
literals refuse. The concrete selected-author executor settles the validated
configuration no-effect branch and keeps malformed private-payload responses in
bounded recovery. Four failure-decoder tests, 27 agent-storage tests, 15 executor
tests and all eight selected-author tests pass. Discovery/replication failure
families and broader task conformance remain unfinished.

The six repository-discovery commands now decode their closed failures and use
the guarded no-effect settlement path. Root-based failures require the exact
requested root; page-reference failures require the exact requested page and
cannot cross into root-based commands. Budget failures deserialize only the
five domain budget variants and have exactly failure/budget members. The five
continuation failures contain only their category and require a request carrying
a continuation token. No matches, counts, private payload or next token can ride
alongside any refusal. The shared budget enum now supports closed deserialization
without changing its existing serialized contract.

Review covers all six commands, both root categories, all three page categories,
all five budget names, all five continuation categories, mismatched/descendant
anchors, partial results and token-free requests. The real-socket fixture drives
each family through the concrete product executor, checks reopened storage and
re-enters terminal executions without another request. The five failure-decoder,
15 executor, eight selected-author, 11 discovery-budget and ten result-window
tests pass. Replication and expanded registry failure coverage, plus the remaining
transport and runtime conformance requirements, are still unfinished.

Replication failure settlement now preserves all three locally derived admission
branches: preflight/zero admission is Rejected with authoritative nonexecution;
positive accepted count followed by confirmed rejection/budget exhaustion is
RemoteFailed with authoritative remote failure; admission_outcome_unknown stays
nonterminal RemoteOutcomeUnknown regardless of the accepted count. The same
guarded snapshot transaction handles both terminal branches and prevents either
from retracting proven success. Remote state, watermark, complete physical set
and local terminal pairing commit or roll back together.

AdmissionRefusal now checks that counts and stopping path are possible for the
retained request: nonzero remaining count, checked bounded total, nonrecursive
single-item shape, exact source for zero accepted items and a strict descendant
after positive admission. This is request correlation, not reconstruction of the
agent-private manifest; the existing manifest-specific consistency check remains
the stronger author-side check. Typed wire decoding rejects surplus fields,
unregistered categories, unrelated paths, zero remaining and overflowing counts.
Review covers every replication category and both count branches, exact maximum
count boundaries, injected transaction failure, stale owners, known-success
protection, restart and no-network terminal re-entry through the concrete executor.
All ten replication-domain, 27 agent-storage, 15 executor, eight selected-author
and six failure-decoder tests pass. Expanded registry coverage and the remaining
runtime/transport task requirements remain unfinished.

Expanded registry failure handling now includes update/move/delete page and
update/delete/reorder component. The decoder uses each existing closed refusal
type, its exact request-correlation method and its no-effect predicate; mutation
outcome uncertainty continues through bounded lookup recovery. Reference-policy
and placement-dependent failures cannot contradict the retained command. Remote
paths and arbitrary details remain absent from the validated evidence value.

Every committed refusal fixture for these six commands passes through the wire
decoder; the covered category set is compared with the installed registry and
must match exactly. Changed page/component/source/destination paths, unknown
categories, surplus fields, ignored-reference rejection and missing-sibling
failure without a named sibling all refuse. The concrete selected-author executor
socket suite now includes all six commands and uncertain mutation outcomes;
reopened settlement and no-network terminal re-entry pass. The focused decoder
matrix, all six domain command suites, 15 executor tests and eight selected-author
tests pass. Asset/fragment and remaining expanded registry failure families,
plus the remaining runtime/transport requirements, are still unfinished.

The mutation decoder/settlement path now additionally covers create asset,
create asset folder, move asset, delete asset, update asset metadata and all six
content/experience-fragment create/update/delete commands. Each branch uses its
existing closed refusal type, computed-target or exact-address correlation and
effect predicate; none infers no effect merely from the failure's spelling.
Fragment variation and reference-policy constraints remain command-specific.

The authoring decoder matrix now covers 17 mutation commands. Every committed
refusal fixture is checked, and each command's covered categories must equal its
installed registry category set. Altered asset/fragment/variation/target/source/
destination paths, private fields and unknown categories refuse. A missing
content-fragment variation cannot be reported when the request named none, and
referenced-asset/fragment refusal cannot contradict an ignore-references policy.
The 17-command hostile-input matrix passes, as do all eleven added domain
command suites, 15 executor tests and all eight selected-author tests. Concrete
socket cases include each new family, uncertain mutations, reopened settlement
and no-network terminal re-entry. Identity, operational/process and other
remaining registry coverage, plus runtime/transport conformance, remain unfinished.

Identity mutation failure settlement now covers create user/group, delete
authorizable, update user profile, set user disabled and add/remove group member.
The shared typed refusal methods bind the authorizable identifier or both group
and member identifiers; group-has-members cannot answer a deletion expecting a
user. No-effect predicates retain the distinction between rejection and unknown
mutation, and validated evidence exposes neither private identifiers nor payloads.

The identity decoder matrix checks every committed refusal fixture and compares
each command's categories with the installed registry exactly. A test setup
initially named a different group than the shared membership fixtures; correcting
the setup retained the refusal rather than weakening correlation. The final
matrix passes with syntactically valid alternative identifiers, surplus fields,
unknown categories and wrong expected-kind checks. All five identity domain
suites, 15 executor tests and eight selected-author tests pass; real socket cases
cover all seven commands, uncertain outcomes, reopened settlement and terminal
re-entry without another request. The fixture checker is shared with the existing
authoring matrix, with identity cases independently runnable. Operational/process,
remaining read-command failure coverage and broader runtime/transport conformance
remain unfinished.

Operational mutation failure settlement now covers cancel Sling job, start
workflow, terminate workflow instance, set workflow suspension, flush replication
queue, retry queue entry, update/delete OSGi configuration and set OSGi bundle
state. Each branch uses its closed command refusal, exact request correlation
and effect predicate. PlatformControlOutcomeUnknown enters bounded unknown
recovery rather than authoritative failure or permission to repeat the action.
QueueExpectationMismatch requires a retained expected-entry count. Configuration
identifiers and bundle names are checked but never exposed by validated evidence.

The independently runnable nine-command operational decoder matrix passes every
committed refusal fixture and compares each covered category set with the current
registry exactly. Hostile identifier/name substitution, unknown categories,
surplus private fields and an expectation mismatch without an expectation all
refuse. All nine domain command suites, 15 executor tests and eight selected-author
tests pass. Real socket cases cover every new command, unknown control outcomes,
reopened settlement and no-network terminal re-entry. Remaining read-command
failure coverage and the broader runtime/transport requirements are unfinished.

Targeted read failure settlement now covers inspect Sling job, inspect workflow
instance, inspect replication agent, resolve resource path and map resource path.
Closed refusal decoding binds the exact job/instance/agent or original resolution
subject. An independent selected-registry category check prevents a shared type
from admitting another command's category: request_address_rejected is legal for
resolution and explicitly refused for mapping. Validated evidence is opaque and
contains no exposed private identifier, address, result fragment or remote-selected
effect disposition.

The five-command decoder matrix passes every declared category and rejects
altered/missing subjects, unknown categories, private fields, matches and tokens.
All four underlying domain suites, 15 executor tests and eight selected-author
tests pass. Real socket cases exercise all five commands through the concrete
product executor, reopened terminal settlement and terminal re-entry without
another request. Windowed/listing failures and remaining runtime/transport task
requirements are still unfinished.

Windowed read failure settlement now covers child-page listing, group-member
listing, asset-rendition listing and replication-queue inspection. Queue and
rendition listings lacked closed domain refusal types; those now contain only
their exact typed anchor and registry category, with request-correlation methods.
The wire gate accepts only the selected registry's categories, checks exact
anchors, and admits the five common discovery budgets only in the two-member
failure/budget shape. Continuation refusals contain only failure and require a
retained continuation request; they never return partial results or a new token.

The read decoder matrix now exercises all declared categories for these listings,
all five budgets, every continuation category, wrong/missing anchors, extra
fields, invalid budget names and token-free requests. Test requests were corrected
to include the group-listing contract's mandatory include_indirect choice, not
to supply an implicit default. The decoder matrix, all four listing domain
suites, 15 executor tests and eight selected-author tests pass. Concrete socket
cases include all four listings and a budget refusal, reopened settlement and
no-network terminal re-entry. A source-level registry comparison identifies ten
remaining failure-decoder command gaps; those and the broader runtime/transport
task requirements remain unfinished.

The ten remaining failure-decoder gaps now have concrete routing: configuration,
job and workflow discovery; bundle, component, replication-agent, resource-mapping,
job-queue and workflow-model inventories; and content-fragment reads. Inventory
refusals use a content-free typed vocabulary with an independent selected-registry
category check and exact wire-shape validation. Lookup budgets are closed and
continuation failures require a retained continuation request. Fragment refusals
bind the exact fragment and cannot claim a missing variation when none was asked
for. The durable failure dispatcher is now exhaustive over all 64 commands, so
adding an unhandled command is a compile error rather than a fallback to recovery.

Review found two fixture requests missing the mandatory job/workflow state sets;
the fixtures now provide explicit states without changing the request contracts.
All ten underlying domain suites (45 tests) and the 15 executor tests pass. The
19-command read decoder matrix checks declared categories, budgets, continuation
requirements, subject correlation, surplus private fields and redaction. These
changes close failure-routing coverage, not the broader runtime/transport
conformance or subsequent product runtime-builder tasks.

All eight selected-author integration tests pass, with the ten new commands
exercised through real sockets and the concrete executor, followed by reopening
durable settlement and terminal re-entry without another request. Unknown effect
evidence remains recoverable and never grants a resend.
The final read decoder matrix also passes the cross-command inventory-category
checks: a valid shared refusal variant is rejected for another selected command.

The transport review found that real TLS coverage stopped at rejecting a peer
that spoke plaintext. A new six-case loopback matrix now runs the actual selected
author connector against a TLS server restricted independently to TLS 1.2 and
TLS 1.3. Both versions complete a finite authenticated request with the selected
root and matching IP subject alternative name. An unrelated selected root and a
trusted certificate with a mismatched hostname each fail at connection setup
before the server receives any HTTP bytes. The server's negotiated version is
checked rather than inferred from configuration. Test-only certificates and the
explicitly nonproduction leaf key are committed; no signing key is retained in
the repository. The provider fixture accepts an explicit verified platform
snapshot only in test setup, leaving the product constructor unchanged.

All 15 environment-provider tests and seven finite-response tests pass after
review. These checks add live TLS evidence; they do not prove obsolete-protocol
rejection, HTTP/2 transport, complete event supervision or complete task 7501.

The response-boundary review found a real HTTP/1.1 mismatch: the raw line reader
applied the decoded name-plus-value field limit to delimiters, surrounding
whitespace and CRLF too, rejecting valid exact-bound fields. Header fields now
have a separate incremental reader. Every wire byte still consumes the aggregate
raw-head budget; only name and trimmed value bytes consume the decoded field
budget. Pending whitespace becomes decoded interior whitespace only when another
value byte arrives. Extra fields and oversized decoded fields refuse before
collection of the offending byte, without waiting for the line ending.

Fourteen real-socket parser cases cover exact/over name and value bounds,
surrounding/interior whitespace, exact/over raw aggregate bounds, empty head
termination at the field-count limit, early extra-field refusal and malformed
line endings. Hostile peers deliberately withhold line endings and wait for the
client to close, proving immediate refusal rather than EOF-based validation.
The full provider fixture also checks exact/over installed field limits through
the concrete finite exchange. All five connection unit tests, 15 provider tests
and seven finite-response tests pass. HTTP/2 still requires its separate bounded
encoded/decoded reader and is not claimed by this correction.

HTTP/2 now has a separate bounded wire-frame reader as the next implementation
layer, not an enabled product protocol. It requires the initial server SETTINGS,
keeps the receiving frame maximum at 16,384 regardless of peer settings, charges
encoded HEADERS/CONTINUATION bytes before payload allocation, strips only header
padding/priority from that charge, and refuses interleaved continuations, other
response streams, server push and any second/trailing header block. One request
uses stream 1; no multiplexed connection reuse is claimed. Failed or cancelled
reads poison the reader so a partial frame cannot be resumed as a fresh head.

Seven focused tests pass, including exact/over frame and encoded-block limits,
peers withholding oversized payloads, padding/priority and reserved-bit handling,
empty/nonempty trailing headers, continuation ordering after END_STREAM, malformed
preambles and cancelled/truncated reads. The daemon still compiles. These are
wire-layer tests, not HPACK or end-to-end HTTP/2 evidence. Decoded field/count/list
limits, pseudo-header/status checks, flow control, negotiation, enclosing phase
deadlines and route integration remain required before exposing HTTP/2 outcomes.

The HTTP/2 wire-layer review against RFC 9113 sections 6.1, 6.3, 6.5.2 and 6.9
found missing control-payload checks and a reset-state gap. Server SETTINGS now
reject enable-push values other than zero, excessive initial windows and invalid
maximum frame sizes, while preserving ordered repetitions and unknown settings.
WINDOW_UPDATE rejects zero increments after masking the reserved bit. PRIORITY
rejects self-dependency. DATA padding is validated but retained in the frame
payload so later flow-control accounting includes every wire payload byte.
RST_STREAM closes response admission even before the first response header;
later HEADERS or DATA cannot be accepted as continuation of that request.

Four new test matrices cover valid and invalid setting boundaries, reserved-bit
window/dependency behavior, padding and reset-before/after-head ordering. All
16 connection unit tests pass. This closes the discovered wire-layer follow-ups;
HPACK/decoded-header and product HTTP/2 integration remain unfinished.

The HTTP/2 decoded-header layer now accepts name/value bytes incrementally from
a future HPACK decoder. It charges field count before starting a field and checks
decoded field and aggregate bytes before storing each octet. Aggregate accounting
includes :status and explicit per-field/final separators, independently of the
wire reader's encoded-block charge. The only response pseudo-field is one initial
:status; informational, redirect and out-of-range statuses refuse. Uppercase names,
invalid value boundaries, connection-specific fields, response TE, trailers and
alternative-service migration refuse without normalization. Duplicate ordinary
fields remain intact for the shared singleton gate. Partial fields, invalid call
order and any earlier refusal prevent finishing or retrieving a partial head.

Six new tests cover exact/over limits, refusal before excess-byte storage, field
count, partial/call-order failures, pseudo/status rules, hostile fields, redacted
debug output and duplicate content-type rejection through the shared response
gate. All 22 connection unit tests and seven finite-response tests pass, and the
daemon compiles. This layer still needs an HPACK decoder that feeds it before
collecting decoded values; no product HTTP/2 support is enabled or claimed yet.

HPACK Huffman strings now decode incrementally into a fallible octet sink. The
RFC 7541 Appendix B numeric codebook builds a fixed 513-node trie at compile
time; no peer-controlled tree or decoded-string allocation is needed, and work
is linear in encoded bits. Literal EOS, non-EOS padding and padding longer than
seven bits refuse. Any sink refusal poisons the string and prevents finishing.

Five tests pass: independent RFC string examples, all 256 octets and eight legal
padding lengths, malformed EOS/padding, permanent sink refusal and direct feeds
into the real decoded-header reader at exact/over installed field bounds. The
full run passes 27 connection unit tests and seven shared-response tests; the
final exact-bound extension also passes the focused Huffman suite. The daemon
compiles. This is the Huffman primitive, not a complete HPACK parser: integer
prefixes, literal/indexed representations, static/dynamic table state and frame
integration remain required before product HTTP/2 can be enabled.

HPACK prefix integers now decode incrementally with immutable per-use value
limits, checked shift/multiply/add arithmetic and a ten-continuation-octet cap.
Incomplete prefixes never become usable lengths or indexes. Overflow, usage
bound violations and extra octets poison the reader. Bounded nonminimal encodings
remain accepted rather than silently canonicalized. Length-delimited literal
strings compose this integer reader with raw-octet or streaming Huffman emission,
without collecting encoded or decoded strings. Exact boundaries prevent consuming
the following instruction; malformed padding, sink refusal or truncation prevents
completion, with no fallback to treating compressed bytes as raw text.

Seven new tests cover RFC integer examples, all prefix widths and u64 boundaries,
overflow and truncation, exact/over literal lengths, raw/Huffman/empty strings,
refused sinks, surplus bytes and literal feeds through the actual decoded-header
gate. All 34 connection unit tests and seven shared-response tests pass, and the
daemon compiles. Static/dynamic table handling and the complete header-block
parser still need implementation; product HTTP/2 remains disabled.

HPACK now has the complete 61-entry static table and a per-connection dynamic
table bounded by the fixed 4,096-byte decoder allowance. Dynamic entries include
the protocol's 32-byte overhead, index newest-first starting at 62, preserve
duplicates, and evict oldest entries before copying new bytes. Oversized entries
clear the table without insertion rather than becoming malformed-header errors.
Size updates cannot exceed the advertised decoder allowance; zero capacity,
shrink and regrowth retain the specified eviction behavior. Missing/zero/overflow
indexes refuse, and neither tables nor borrowed fields reveal private bytes in
debug formatting. Block-position rules for size updates belong to the upcoming
header-block parser, not this storage layer.

Five new tests cover pinned static indexes, dynamic order/duplicates/eviction,
exact and oversized capacities, zero/shrink/regrowth, repeated bounded insertion,
redaction and indexed-field feeds through the real decoded-response gate. Indexed
request pseudo-fields, redirects and connection-specific response fields still
refuse. All 39 connection unit tests and seven shared-response tests pass, and
the daemon compiles. The full HPACK header-block parser and product HTTP/2
integration remain unfinished.

The HPACK response-block parser now composes the bounded primitives for one
fresh single-request connection: indexed fields, all three literal forms,
literal/indexed names, raw/Huffman strings and start-of-block table-size updates.
Every representation passes the same incremental decoded-header checks. Encoded
bytes are independently bounded, incomplete states cannot finish, and every
error permanently poisons the block. Indexed-field capture is capped at the
decoder-table allowance and occurs only after the header reader accepts each
octet; a larger valid indexed field clears the dynamic table without an unbounded
duplicate allocation. Table changes cannot escape a failed block.

Eight new tests exercise all representation forms and every byte split of valid
literal/Huffman blocks, dynamic references/eviction/nonindexing, invalid indexes
and late/oversized table updates, truncation, exact/over encoded and decoded
limits, repeated-index field-count expansion, oversized capture and redaction.
An integration test feeds bounded HEADERS/CONTINUATION payloads from the wire
reader directly through the parser. All 47 connection unit tests and seven
shared-response tests pass, and the daemon compiles. Request encoding, flow
control, negotiation, body/framing completion and runtime route integration are
still required before the product can enable HTTP/2.

HTTP/2 request-head encoding now shares the immutable selected-origin URI/query,
body-size and caller-header validation path with HTTP/1.1. That review also closed
caller overrides for proxy-connection, keep-alive and TE. The encoder emits the
four request pseudo-fields first, derives authority/context path and exact-once
ordered queries from the selected transport, borrows authentication and forces
identity coding and the exact body length. All HPACK fields use literal
never-indexed representations, including credentials and identity/query material.
Caller-supplied HTTP/2 field values with surrounding whitespace refuse rather
than being normalized. HEADERS/CONTINUATION fragments stay within 16,384 bytes;
END_HEADERS appears only on the last fragment and END_STREAM only on the first
HEADERS of an empty-body request. No DATA is emitted without flow control.

Two unit tests pin literal lengths and exact/over fragment boundaries and flags.
A selected-profile test checks pseudo-field order, origin/context/query bytes,
authentication, never-indexed markers, prohibited overrides and malformed values.
All 49 connection unit tests, 16 provider tests and seven shared-response tests
pass; the daemon compiles. This encoder does not connect or authorize a send.
Flow control, negotiation and full response/body/runtime integration remain next.

Single-request HTTP/2 flow accounting now keeps connection and stream send
windows separate, reserves DATA before writes, caps each reservation at 16,384
bytes and blocks when either window has no credit. SETTINGS reductions may make
the stream window negative without changing connection credit. Zero/invalid
increments and overflow refuse; multi-setting frames apply in order and commit
atomically. A failed or cancelled write requires discarding the connection, not
reconstructing uncertain send credit.

Receive accounting charges the whole DATA payload before exposing unpadded
content. A non-cloneable permit returns connection/stream WINDOW_UPDATE frames
only when the consumer explicitly releases it. Dropping a permit restores no
credit, empty DATA emits no invalid zero increment, and failed receive charges
are atomic. The driver must write released credit before admitting more data and
discard the connection on failed/cancelled credit writes.

Seven tests cover exact exhaustion, independent replenishment, negative stream
windows, overflow/invalid updates, ordered-settings atomicity, dropped receive
credit, padding and actual wire-frame integration. All 56 connection unit tests
and seven shared-response tests pass; the daemon compiles. Negotiation and the
complete network driver, response/body completion and runtime route integration
remain unfinished, so product HTTP/2 is still not enabled.

Finite HTTP/2 responses now assemble behind the frame, HPACK and receive-window
layers. The shared finite-head policy runs before collecting body bytes, so
invalid coding, duplicate singleton metadata and trailer declarations refuse
early. Content-Length is optional but, when present, must be unique, unsigned,
bounded and exact at END_STREAM. DATA padding consumes flow credit without
entering the body. Short, long, oversized, partial and post-ending responses fail
closed; 204 and 205 content restrictions are explicit. No partial body is exposed.
The driver must still prove the final connection boundary before invoking the
assembler's finishing method; END_STREAM alone is not that proof.

Seven focused tests cover valid length-delimited/unlengthened bodies, early head
refusals, body bounds and length mismatches, padded credit, incomplete heads,
trailers/extra DATA, no-content statuses, redaction and a complete bounded
HEADERS/CONTINUATION/HPACK/DATA pipeline. The full regression run passes 62 unit,
16 provider and seven shared-response tests; the added pipeline test then passes
in the seven-test focused suite. The daemon compiles. Negotiation, the live
network driver and runtime integration remain unfinished.

The selected-author connector now freezes HTTP/1.1-only and h2-only ALPN
configurations at construction. Both share the same immutable root verifier,
TLS versions, crypto provider, hostname validation and phase deadlines. Existing
HTTP/1.1 callers retain their path and may accept a legacy peer without ALPN;
the explicit HTTP/2 connector requires exactly h2 for protected authors and
never silently downgrades. Neither connection method sends application bytes.
Selected permitted cleartext HTTP/2 is prior-knowledge only: the future driver
must validate its preface/settings exchange before sending request headers,
without an HTTP upgrade or alternate endpoint.

A 16-case real TLS matrix covers TLS 1.2/1.3, peers offering both versions,
incompatible-only and absent ALPN, h3-only refusal, actual negotiated protocol
and absence of application bytes during negotiation. All 17 provider tests and
seven shared-response tests pass, and the daemon compiles. The existing finite
driver remains HTTP/1.1 until the full HTTP/2 network driver and runtime route
integration are implemented and verified.

The HTTP/2 frame reader now issues a single-use opaque transport-end proof only
after complete headers, END_STREAM and clean EOF. Early EOF, partial frame
headers or payloads, unfinished continuations, resets, trailing response frames,
and cancelled reads cannot issue the proof. Finite response completion consumes
it rather than trusting a caller's assertion that END_STREAM was sufficient.
The wire-to-HPACK-to-finite-response test now obtains this proof from actual EOF.
Review caught and closed the reset-versus-END_STREAM distinction explicitly.

The frozen selected-author connector now exposes prepare_http2, composing its
existing strict TLS ALPN / permitted prior-knowledge socket with a bounded
preface and SETTINGS handshake. It disables push, acknowledges peer settings and
PINGs, preserves send-window changes, waits for both the client's SETTINGS ACK
and permission to open a stream, and never sends request headers or body during
negotiation. Invalid prefaces, premature response frames, GOAWAY, idle-stream
window updates and deadline expiry refuse as pre-request connection failures.
No alternate endpoint, protocol fallback or retry is introduced.

Duplex tests verify exact wire bytes, settings/credit retention, zero concurrent
stream permission followed by a later grant, PING acknowledgement, incompatible
peers and silence. A real selected-profile loopback test verifies successful
preparation and HTTP/1.1 refusal without application request bytes. All 70
connection unit tests, 18 provider tests and seven shared-response tests pass;
the daemon compiles. Full duplex request/response driving, artifact and event
transport integration, and runtime route composition remain unfinished. This
is implementation work to continue, not an external blocker or task completion.

Finite HTTP/2 now has a live selected-author exchange method, not just separate
wire primitives. It encodes the authenticated selected-origin GET/POST before
connecting, performs the strict existing preparation, and joins bounded reader
and writer futures over one connection. Request DATA requires both flow windows;
the reader can continue handling settings, PINGs and window updates while the
writer waits for credit. HEADERS/CONTINUATION remain uninterrupted. Response
credit is acknowledged by the writer after both WINDOW_UPDATE writes before
the reader admits another DATA frame. Neither future is detached, so failure or
caller cancellation drops both socket halves without a retry or later send.

The request-write deadline includes flow stalls. Request completion updates the
response-head deadline without cancelling an in-progress frame read. Finite
idle deadlines reset on actual read bytes, independently of the absolute body
deadline, so small control frames cannot indefinitely extend an exchange.
An early complete response stops the unfinished request half with a NO_ERROR
reset. Single-use GOAWAY/write shutdown is followed by clean peer EOF proof;
no finite receipt escapes before both I/O futures complete successfully.
Post-ending control frames cannot enqueue repeated close commands. Reset,
truncation, actual trailers, extra DATA, missing EOF and missing TLS close_notify
refuse without returning a partial response or asserting POST nonexecution.

Nine duplex tests cover 100,000-byte requests and responses exceeding initial
windows, zero-credit waits, PING processing, early response cancellation, trailing
control handling, hostile framing, distinct phase deadlines, byte-wise idle
progress, total-deadline control flooding, absent final EOF and caller drop.
The real selected-profile socket fixture checks exact authenticated encoded
request bytes, DATA and GOAWAY over permitted cleartext and TLS 1.2/1.3, with
separate missing-close-notification refusals for both TLS versions. Review found
and fixed premature receive-credit reuse and repeated closure-command races.
All 79 unit, 19 provider, seven shared-response and ten route/policy tests pass;
the daemon compiles. This completes the finite HTTP/2 driver, not the full task:
artifact/event transport and runtime route/protocol composition still remain.

HTTP/2 artifact retrieval now uses the same joined I/O driver through a private
response-consumer boundary. The selected connection and retained submission are
checked before one fixed operation/slot GET; media, digest spelling, artifact
length limits and local artifact identifier are checked before connection work.
Successful responses pass bounded HPACK/shared head policy and exact artifact
media/length checks before any staging write. DATA is streamed in bounded frames
to the caller's private sink with incremental SHA-256, padding-aware flow credit
and exact-length accounting, without a whole-artifact buffer. Only clean EOF,
exact length and the expected digest produce an artifact receipt.

404/410 heads instead enter the existing bounded finite-response assembler,
using shared decoded-head installation and Content-Length parsing rather than
re-decoding or collecting compressed headers again. Their bodies never enter
the artifact sink. The public wrapper still requires the closed unavailable
document's complete retained provenance, generation, operation, artifact and
slot identity before returning unavailability evidence. Successful artifacts
use the named artifact idle/total deadlines; error documents keep finite
deadlines. No status alone proves unavailability, and no failure publishes staged
bytes. Review fixed the streaming consumer's post-END_STREAM refusal so it also
permanently poisons completion, matching the finite consumer.

Six focused consumer tests cover padding, fragmented empty responses, malformed
heads, short/excess bodies, digest drift, sink refusal, poisoned post-end input,
bounded unavailable bodies and a 2,097,153-byte stream beyond the finite-document
limit. The selected-profile socket fixture adds eight HTTP/2 artifact cases for
success, digest/trailer/sink refusals, valid missing/retired documents and wrong
or bare identities, plus a no-connect selected-revision mismatch check. All 85
unit, 19 provider, seven shared-response, eight artifact-download and one
artifact-unavailable tests pass; the daemon compiles. Event streaming and runtime
route/protocol selection remain unfinished; task 7501 is still active.

Event-stream integration review identified a chunk-boundary correctness gap:
the decoder's collecting push API discarded earlier complete items when a later
item in the same transport chunk failed. A new push_each boundary delivers each
validated item immediately without a chunk-sized item batch. Earlier committed
items remain delivered; a decoder or consumer refusal closes the decoder before
any later item, and even a caught consumer panic cannot reopen it. The existing
collecting API remains available through the same parser. Line bounds now refuse
before storing the excess byte, and decoder Debug no longer exposes buffered
payloads or retained expectation material.

The joined HTTP/2 driver can now consume an established-stream liveness deadline
instead of installing finite total/raw-byte idle deadlines. The deadline belongs
to the response consumer and is renewed only when that consumer validates a
complete stream item; HTTP/2 PING/settings/window traffic does not renew it.
Finite and artifact consumers retain their existing absolute/idle policies.
A paused-clock driver fixture proves live item activity can exceed both finite
budgets while PING-only activity still expires. Four decoder tests cover every
split of valid-prefix/malformed-tail input, consumer refusal, 100,000 incremental
heartbeats without batch collection, caught panic and diagnostic redaction.
All 86 unit, 18 decoder, eight heartbeat, 14 reconnection, 19 provider and seven
shared-response tests pass, and the daemon compiles.

Live event attachment itself remains to implement. Its review must also address
the current decoder's single-command terminal expectation: a filtered
subscription carries multiple retained operations, so terminal provenance and
SubmittedCommandDigest must be resolved against the associated operation before
cursor or terminal persistence. Do not wire a single fixed command expectation
as though it represented the complete product subscription. Reset documents,
durable per-item folding, reconnect/authentication refresh and runtime route
selection also remain part of task 7501, not reasons to stop or advance tasks.

The event decoder now supports per-operation terminal expectation resolution for
a filtered subscription. It parses the closed event and checks its requested
subscription/generation before invoking the resolver with the operation key.
The resolved record must independently match subscription, generation and
operation, carry installed transport/canonical/command provenance, and contain a
canonical 64-character lowercase submitted digest. Only then is the terminal
document compared and delivered. Thus matching corrupted remote and retained
contracts cannot authenticate each other, and a failed lookup produces neither
a terminal item nor cursor delivery. Resolver debug output remains redacted.

The existing fixed-expectation constructor remains compatible for its
single-command callers; the new subscription constructor accepts a resolver
without requiring an eager, potentially unbounded map of every operation.
Runtime callers must source it from retained state, not the event's claimed
contract/digest. Four tests cover interleaved query_paths/create_asset terminals
with different contracts/digests at all existing chunk sizes; wrong retained
key/subscription/generation/digest/contract and missing records; rejection before
lookup for another stream; and identical remote/retained transport, canonical or
digest corruption. All 86 unit, 22 decoder, eight heartbeat, 14 reconnection and
13 event-reducer tests pass; the daemon compiles. The resolver interface is now
implemented and verified, but live attachment and its durable storage resolver,
folding, reset/reconnect/authentication and runtime integration remain unfinished.

HTTP/2 now has a live selected-author event attachment method. It checks selected
execution identity, subscription/generation and bounded header-safe committed
cursor before connecting, then builds the fixed context-prefixed event route
with ordered once-encoded generation/subscription query members and Last-Event-ID.
It uses the same authenticated request encoder, strict preparation, bounded
frame/HPACK gates and joined I/O driver as finite and artifact exchanges.

An exact 200 event-stream head installs the per-operation resolver decoder and
complete-item heartbeat policy. Events are delivered one at a time for caller
folding; later malformed data, actual trailers, partial EOF or callback refusal
cannot retract independently delivered earlier events. HTTP/2 padding is charged
as flow credit but never decoded as event data, and optional Content-Length must
be unique, unsigned and exact. There is no finite total-body limit on a live
stream. Complete comments/events renew the named heartbeat; PINGs and unfinished
event bytes do not. Expiry has a distinct EventHeartbeat failure and remains
connection evidence only, never nonexecution or terminal job evidence.

Non-stream responses instead use bounded finite JSON handling and clean EOF.
They remain unclassified response receipts: the caller must still validate
closed reset documents and apply route-aware authentication/retry policy, never
infer reset authority from a 409/410 status. A cleanly ended event attachment
likewise establishes only closure, not job completion. No automatic retry,
cursor persistence or job mutation is added to the transport.

Five driver/consumer tests cover a minute-long paused-clock stream beyond finite
budgets, validated terminal delivery, valid prefixes before malformed/trailing/
partial/unknown-operation tails, callback failure, PING/partial-byte starvation,
finite error responses and forbidden heads/length mismatches before item delivery.
A real selected-profile socket test pins context/query escaping, authentication,
Accept and committed cursor, GOAWAY/EOF, and preflight no-connect refusals. The
full targeted regression set passes 91 unit, 20 provider, seven shared-response,
22 decoder, eight heartbeat and 14 reconnection tests; the daemon compiles.
HTTP/1.1 event attachment, closed reset decoding, durable storage resolution and
folding, reconnect/authentication refresh and runtime protocol selection remain
unfinished, so task 7501 is not complete.

HTTP/1.1 now has the matching selected-author event attachment. Both versions
share request/cursor preflight, per-operation decoder construction, independent
item delivery and heartbeat accounting in selected_author_events. Shared cursor
validation rejects leading/trailing whitespace as well as injection/size defects
before connecting; HTTP/1.1 cannot normalize a cursor differently from HTTP/2.
The shared delivery boundary also refuses a synchronous consumer that returns
after its heartbeat deadline instead of granting activity at callback completion.
The previous HTTP/2 outcome path remains available as a compatible re-export.

HTTP/1.1 reuses its existing bounded head, chunk-extension and finite-body
parsers. Fixed and chunked live bodies stream through a bounded 8,192-byte buffer
without a finite total-body limit. Chunk sizes/extensions/delimiters and partial
event bytes never refresh the application heartbeat; each blocking framing/read
step uses the current absolute complete-item deadline. Zero chunks require the
empty terminator, trailers and surplus bytes refuse, and closure still requires
clean EOF with no partial event. Non-200 JSON responses remain bounded finite
receipts requiring later route-specific validation, not reset authority.

Four response-driver tests cover minute-long fixed/chunked streams with terminal
correlation, malformed/partial/trailing/surplus/short bodies and sink refusal,
chunk-metadata/partial-data heartbeat starvation, finite errors and invalid
informational/redirect/coding/trailer/length heads. The public socket fixture now
checks both versions, including both HTTP/1.1 framings, exact context/query/
authentication/committed cursor and shared no-connect preflight refusals.
All 95 unit, 20 provider, seven shared-response, 22 decoder, eight heartbeat and
14 reconnection tests pass with required loopback access; the daemon compiles.
Closed reset decoding, durable storage resolution/folding, reconnect and
authentication refresh, runtime protocol selection and full product conformance
remain unfinished. Neither task 7501 nor the overall plans goal is complete.

Event resets now have a closed versioned wire document and inventoried schema.
It carries transport identity, filtered subscription, explicit requested
generation/nullable Last-Event-ID echoes, current generation, captured high-water
cursor and generation_changed/cursor_expired reason. Subscription-wide resets
do not invent single-command provenance. Missing nullable echoes are refused
through required-field deserialization rather than treated as an omitted None.
The schema's byte manifest was updated and its fields/bounds are contract-tested.

The reset codec requires a complete finite JSON response and independently held
request context. It rejects unknown/duplicate/missing fields, wrong echoes or
transport version, malformed/oversized/header-unsafe cursors and inconsistent
status/reason/generation combinations. HTTP 409 requires a genuinely different
current generation; HTTP 410 requires the events route, an actual requested
cursor and unchanged generation. High-water requests cannot claim cursor expiry.
Neither raw status nor matching malformed documents become reset evidence.

Both public event transports now return a distinct validated Reset outcome for
409/410 or refuse the response. The reconnection boundary additionally compares
the evidence with its still-current subscription, generation and committed
cursor and refuses stale context or already-outstanding recovery without
mutation. Accepted evidence enters degraded reset recovery while preserving the
old generation and cursor; the captured new cursor is not installed. The
snapshot/high-water reconciliation algorithm still has to finish separately.

Five codec/recovery tests cover valid reasons and request preservation; every
required field plus duplicate/surplus/wrong echoes; closed route/status/generation
relations; byte bounds, unsafe cursors/media and malformed bodies; and stale
cursor/generation/subscription/conflict guards with no early installation.
Ten real socket cases cover HTTP/1.1 and HTTP/2 valid resets and wrong-cursor,
bare-body and truncated refusals. The protocol schema/serialization test and
11 schema-inventory/logical-execution tests pass. The connection regression set
passes 95 unit, 20 provider, five reset, 14 reconnection, 22 decoder and seven
shared-response tests; the daemon compiles. Durable reset orchestration and
high-water capture, per-item storage folding, authentication/reconnect scheduling
and runtime protocol selection still remain within task 7501.

Authenticated high-water capture now has explicit HTTP/1.1 and HTTP/2 selected
transport entry points, preserving the selected context prefix and exact query
encoding. It sends JSON Accept and authentication but no Last-Event-ID. A closed,
inventoried response schema binds format, transport digest, subscription,
generation and captured cursor. The decoder independently verifies the request
echoes, finite head policy and cursor byte/header safety before producing opaque
validated capture evidence. HTTP 409 uses the generation-reset codec; other
statuses remain generic responses, never cursor installation authority.

Three capture decoder tests and one schema/serialization contract test pass,
alongside the protocol suite and existing reset tests. Route-specific socket
coverage for capture and durable all-job snapshot reconciliation still need to
be completed. The capture API deliberately performs no ledger mutation; runtime
composition is not complete and this task remains planned.

The selected-environment provider suite now also exercises fourteen real socket
high-water cases across HTTP/1.1 and HTTP/2: valid capture, changed generation
echo, truncated response, valid generation reset, bare reset status, cursor-expiry
status without reset authority, and authentication failure. It checks the exact
selected context path/query, JSON Accept, authentication value, absence of
Last-Event-ID, and refusal before connection for invalid subscription/generation
or moved selected revision. All 21 provider tests pass. The full connection
regression suite from the capture implementation also completed successfully.

The next reconciliation review found a required wire-model gap: JobSnapshot
contains a per-job sequence but no subscription watermark. Architecture's reset
algorithm requires every snapshot to prove inclusion through the captured
subscription position; comparing that position to a job sequence cannot supply
this proof. Add the independently validated subscription watermark and its
closed schema/fixtures before joining capture to all-job atomic reset. The
existing raw ledger install method alone is not evidence of that transaction.

Snapshots now require a distinct `subscription_watermark` in their closed wire
schema and typed representation. Decoding applies cursor byte and header-safety
bounds before exposing it; missing, null, numeric, duplicate, empty, oversized,
or unsafe values refuse. The exact-byte schema inventory and snapshot-producing
socket fixtures were updated without adding the field to submission acknowledgements.
Both wire and decoded snapshot debug output redact private evidence.

Validated capture can check whether one independently identity-validated snapshot
names the same subscription/generation and covers the captured cursor. Tests
exercise older/equal/newer watermarks with zero, low, and maximal job sequences,
proving the sequence cannot replace subscription coverage. Wrong generation,
wrong subscription and unsafe typed cursors refuse. This is a per-member check,
not permission to install a cursor or clear an incident. Durable membership,
all-job reconciliation and atomic reset installation remain to implement.

Storage recovery now reads subscription ledger and complete unsettled membership
inside one read transaction, using fixed 256-row keyset pages and a checked
namespace-wide total bound. Membership includes every retained generation and
selected revision, plus remotely terminal children whose local owner remains
nonterminal. Terminal local-independent children and other targets/subscriptions
are excluded. The returned view is redacted and explicitly not a frozen permit:
the eventual atomic installation must revalidate membership and all facts.

The older ledger-only installation could clear an incident without examining
jobs. It now refuses if any unsettled member exists, with that guard in the same
write transaction as installation. Nonempty subscriptions must use the future
all-job atomic path. The new storage test crosses the page boundary with 257
members, covers old generation/revision and remote-success/local-pending state,
checks target/subscription/terminal exclusion, bound exhaustion, restart, new
membership after a prior read, and unchanged incident/cursor on refused reset.
All 28 agent repository tests pass. The prior snapshot change's durable daemon
submission suite also completed successfully (eight tests).

The migration/inventory regression exposed a fixture-order race: two raw SQLite
fixture connections could initialize SQLite before the product's process-wide
no-spill setup. Those fixtures now initialize through the product first; the
deliberately late-initialized refusal probe remains isolated and unchanged.
All 15 migration/inventory tests pass, including that refusal subprocess, and
the daemon compiles. This repairs the discovered test race without weakening
the product's initialization refusal.

Reset transaction review found that capture supplies no event digest. A captured
boundary now uses the existing high-water column while the observed-event cursor
and digest remain null; ledger readers and cursor guards use the effective
cursor `COALESCE(cursor, high_water_cursor)`. This preserves the schema's paired
observed-event cursor/digest invariant without manufacturing event evidence or
requiring a table rebuild. An equal captured boundary with no retained digest is
stale cursor-only; once a later actual event commits, exact replay and conflicting
digest checks apply normally.

The empty-subscription reset path now binds its recovery view to the owning
database handle, compares the complete ledger again under a write transaction,
rechecks still-empty membership and retained event accounting, then atomically
installs generation/high-water/compaction boundary, clears the incident and
removes old events. Same-generation cursor-expiry recovery is supported without
rolling the cursor backwards. Invalid bounds/control bytes, wrong owner, stale
ledger or a newly admitted member refuse. The post-update deletion fault test
proves rollback of the cursor, incident, events and counters. Reopen, old/equal
boundary replay, subsequent real event replay/conflict and generation change
are covered. All 30 agent repository and 15 migration tests pass; daemon check
passes. Nonempty all-job snapshot installation and runtime orchestration remain
required; this empty case is not a substitute for that algorithm.

The existing active-snapshot writer is now shared inside caller-owned
transactions, retaining its independent local revision/selected revision,
remote identity/contracts/bytes, monotonic state/sequence, retention and physical
association checks. Recovery views now also capture local owner records and
physical-job sets in the same read transaction as membership and ledger.

A same-generation active-membership batch reset re-reads that complete view
under the write transaction, requires exactly one ordered covering snapshot for
every member, and applies every job observation, per-job sequence watermark,
physical association and conservative retention before installing the shared
captured boundary and clearing its incident. Missing local owners, prior
generations and terminal snapshots do not qualify for this active path; those
still require their own generation-loss or terminal settlement integration.

The two-job test covers successful reopen plus eighteen refusal/rollback cases:
old watermark, incomplete/duplicate membership, newly admitted child, moved
remote facts/physical set/local owner, post-update event-deletion failure,
second-job watermark-write failure, changed/prior generation, terminal result,
zero retention, omitted physical job, missing owner, unsafe watermark, integer
overflow and stale sequence. It checks no partial remote observation, physical
association, retention, watermark, cursor or incident mutation. All 31 agent
repository and 15 migration tests pass; daemon check passes. Daemon-side
authenticated capture/all-snapshot orchestration and terminal/generation-loss
batch completion remain required before the whole reset workflow is complete.

The daemon now has a concrete active-subscription reset attempt joining selected
authentication, durable membership, high-water capture, per-operation lookups,
coverage validation and the atomic storage batch. It reconstructs submissions
from their closed retained wire bytes and independently checks stored contracts,
identity, installed derivation and the admitted local command before the first
socket. Lookup supports explicit HTTP/1.1 and HTTP/2 without fallback. All
snapshots stay staged until complete coverage; cancellation or refusal writes
neither staged job facts nor the cursor. Retention includes lookup validation
and subsequent staging time. Lookup also refuses Location-bearing responses.

The two-job socket fixture exercises both protocols, checking exact selected
routes/authentication, unchanged SQLite state at every network phase, successful
atomic installation after reopen, old watermark, wrong digest, truncated body,
bad capture echo, synchronized second-snapshot cancellation, modified retained
contracts/bytes before any socket, and Location refusal. SQLite corruption is
test-only through the existing workspace-pinned dependency. Generation changes
return a distinct validated recovery outcome; terminal/absent members defer to
operation recovery without installing any part of the reset. Those recovery
branches and authentication/retry supervision still need integration, as does
the compiled runtime startup/event loop. Task 7501 remains incomplete.

Reset now preserves validated terminal and absence receipts in a private-field,
single-use recovery handoff bound to the original local repository, execution
identity, submission and expected revision. Consuming it reuses the same guarded
lookup settlement implementation as ordinary operation recovery, without another
capability/lookup request or POST. Missing receipts persist request-start-anchored
grace; verified retirement settles recovery; success without a complete result
preserves authoritative remote success and enters acquisition recovery. No such
handoff clears the subscription incident or commits any other staged member.

Handoff retention is reduced by elapsed time, never renewed. An expired remote
acquisition budget is not permission to discard already validated terminal
truth: terminal success remains authoritative at zero remaining budget, and a
complete typed no-effect failure can still settle through the existing validator.
Active staged snapshots continue to require positive retention for batch reset.
The socket test now also covers missing/retired/terminal handoff, stale revision,
wrong database and delayed success/failure consumption, with no additional socket
and unchanged subscription/other-member facts. The broader ordinary recovery and
executor conformance suites also passed after sharing the settlement code.
Generation-loss physical-job lookup, reset supervision (including paused-member
preflight), authentication refresh and compiled startup/event integration remain
within the unfinished task.

Reset preflight now honors the existing automatic-recovery pause policy for
every member before opening the capture socket. Before each subsequent member
lookup it rechecks the retained command, original local revision, terminal state
and pause policy. A moved/paused member returns operation recovery without
fetching its snapshot or changing staged job/cursor/incident facts. The socket
matrix now has 40 cases across both protocols, including exhausted automatic
attempts, immediate capacity pause, a pause introduced while capture is in flight,
and manual-resume eligibility without actual pause. All pass; eligibility alone
still permits active reconciliation. Generation-loss physical lookup and the
remaining supervisor/startup/event composition are still outstanding.

Selected transport now exposes physical Sling job snapshot retrieval over both
HTTP/1.1 and HTTP/2 on the fixed `/bin/slingshot-agent/jobs/snapshot` route with
only the canonically encoded `sling_job_identifier` query. It preserves the
retained logical operation/generation, target, subscription, selected revision,
contracts and submitted digest and additionally requires the queried physical
identifier in the validated bounded snapshot set. Identifier and selected-context
refusals occur before connection. Retention remains request-start conservative.

Twelve physical snapshot socket cases cover valid recovery, a different physical
job, wrong submitted digest, truncated body and otherwise valid logical
missing/retired documents over both protocols. Logical absence does not echo the
queried physical job and therefore cannot become physical-loss evidence. A
distinct closed physical-absence proof and the all-persisted-job generation-loss
coordinator are still required; these snapshot methods deliberately do not claim
that a refused lookup means a job is missing. Query coverage includes a context
prefix, spaces, separators and Unicode with exact-once encoding.

Physical absence now has a closed, inventoried `physical-job-missing.json`
contract: format, transport digest, missing discriminator, independently observed
current generation and exact queried Sling job identifier. It invents neither
logical operation nor command provenance for an unknown physical record. The
codec requires complete HTTP 404 JSON, acceptable nonredirected/noncompressed
head policy, bounded bytes and exact request/current-generation echoes before
returning private-field redacted evidence for that one job.

The selected physical lookup now returns a distinct Found/Missing result from
the same single request on either protocol. Found preserves retained operation
generation/provenance and validates the physical set; Missing checks the separate
current generation. Logical absence, changed generation, wrong physical identity,
malformed/truncated responses and HTTP 410 never become physical-missing evidence.
Three codec tests, schema/serialization and schema-inventory tests pass; the
provider suite passes 21 tests and daemon test targets compile. The physical
socket matrix additionally exercises current-generation absence against an older
retained submission. The all-persisted-physical-job generation-loss coordinator
must still consume these proofs without treating one missing job as whole-operation
loss or granting resubmission; runtime supervision/composition also remains open.

Generation-loss recovery now has a bounded read-only pass over the physical IDs
captured in the subscription's consistent recovery view. It restores the exact
retained submission, checks the selected context and local recovery eligibility,
queries every retained physical ID using the selected HTTP protocol, and refuses
a changed local operation, membership, ledger or queried physical set. One
refused request does not prevent the remaining independent probes and never
counts as absence. Results distinguish no physical IDs, every ID missing,
unanswered requests, coherent recovered snapshots and conflicting snapshots.
The report grants no resubmission, cursor installation or lifecycle mutation;
durable generation-loss settlement remains a separate required integration.

Review additionally closed snapshot consistency gaps: recovered accounts cannot
regress retained observations, drop retained physical IDs, disagree at one job
sequence, regress between increasing sequences, or change terminal payloads.
Receipt retention is debited again on consumption without erasing terminal truth.
The real HTTP/1 and HTTP/2 reset fixture now includes eleven generation-loss cases
per protocol: all missing, one invalid absence, mixed missing/found, conflicting
same-sequence accounts, coherent advancement, empty physical sets, dropped IDs,
progress regression, coherent snapshots returned in reverse sequence order,
conflicting facts at the retained sequence, and snapshots older than retained state.
All cases assert unchanged ledger, remote children and local operation revision;
each nonempty probe must issue both authenticated physical GETs without POSTs.
Validation passed: the complete 62-case reset/probe socket matrix, all 31 storage
repository tests, daemon test-target compilation and `git diff --check`.

Coherent physical recovery now has a single-use durable handoff retaining its
original operation repository, selected execution identity, exact restored
submission and local revision. It chooses the highest coherent job sequence
independently of response order, debits receipt retention, and reuses the existing
guarded snapshot/terminal reconciliation without capability re-fetch, logical
lookup or submission. Active truth updates the retained prior-generation child;
terminal success enters authoritative result acquisition. Neither installs a new
subscription generation, clears the incident nor assigns a replacement identity.
Non-found report statuses cannot use the recovered-snapshot handoff.

The selected socket fixture exercises durable mixed-found/missing recovery,
increasing and reverse-order snapshots, original-owner binding, a changed local
revision, terminal-success acquisition, and preservation of already-known
success against a later active snapshot. Review added an explicit common-path
guard against that active-after-success regression. Generation-loss disposition
for unavailable truth and whole-subscription runtime supervision remain open.
Validation passed: all nine selected-admission/result-completion integration
tests, then the final 70-case reset/physical-recovery matrix after isolating the
stale-revision case from retry exhaustion. Daemon test targets compile and the
diff passes whitespace checks.

Unavailable generation-loss truth now has a consuming guarded disposition path.
Only a complete all-physical-jobs-missing report or a captured no-association
report may enter it; unanswered transport requests and conflicting snapshots
remain unresolved. The report retains its original ledger and complete recovery
view as well as its local operation owner. A single write transaction rechecks
ledger, membership, every local owner, every remote child and every physical set
before settling the selected local operation as RemoteStateLost with
FailClosedIndeterminate. Existing SubmissionUnknown or RemoteOutcomeUnknown is
preserved; neither remote nonexecution nor an authoritative remote ending is
fabricated. The remote child, physical associations, generation, cursor and
subscription incident remain unchanged, and no replacement work is submitted.

Known success is deliberately refused by this absence-only disposition: missing
physical records do not themselves prove required results/artifacts unavailable.
The existing result-acquisition path retains that responsibility. Confirmed
nonexecution and terminal remote facts likewise cannot become unknown execution.
Review added settlement-time range and backward-time checks. Sixteen storage
cases cover success, retained uncertainty, stronger known evidence, changed
physical/local/remote/ledger/membership state, wrong view owner, generation and
revision errors, timestamp bounds, replay and rollback after the local write.
The complete 32-test storage suite and 70-case HTTP/1-and-HTTP/2 matrix pass.
Whole-subscription generation advancement, unresolved-request supervision and
startup/event composition remain required before task completion.

Generation completion now closes the local-settlement membership gap. Both the
paged recovery query and the transaction's ledger-only guard exclude locally
terminal operations even when their retained remote child has no authoritative
terminal disposition. Orphaned active children remain visible and terminal
remote success with a still-open local result-acquisition owner remains visible.
This does not fabricate or rewrite remote terminal truth.

The selected reset path exposes a guarded generation-finish operation and invokes
it directly when authenticated high-water capture reports a generation change
with no unresolved local members. It requires the same database/selected context,
old generation and any echoed committed cursor, then uses the existing atomic
empty-membership/boundary installation. Repeated finish attempts, one remaining
operation and pending result acquisition cannot install the new cursor. The
HTTP/1 and HTTP/2 fixture now covers two independent settlements followed by
generation advancement, and the already-settled/restart entry-point case. Retained
old generation and operation identifiers are unchanged after the transition.

Review found and fixed the corresponding retention leak: reviewed maintenance
now includes aged RemoteStateLost local dispositions in the same guarded
retained-agent cleanup policy as recovery-window expiry. Tests prove the strict
maintenance cutoff, reviewed disposition, removal of both retained halves and
physical associations, and absence of orphaned recovery membership afterward.
The full storage suite and final 74-case reset/recovery socket matrix pass;
daemon test targets compile and whitespace checks pass. Unanswered-request
supervision and startup/live-event composition remain required for task 7501.

An unanswered complete physical probe now has a consuming durable defer path.
It charges one attempt for the full bounded physical-ID pass, not one per socket,
and shares the ordinary lookup backoff/cap constructor. Existing execution
evidence is preserved, including authoritative success on result acquisition.
Exhaustion remains nonterminal and manually resumable; the next automatic probe
is refused before network work. No absence, terminal ending, cursor movement,
new generation or replacement submission is inferred from a failed request.

The scheduling transaction revalidates the full original subscription view and
local revision before writing, requires exactly one attempt-count increment,
and rejects evidence replacement. Ten storage cases cover ordinary and
success/SubmissionUnknown evidence, changed physical/ledger/local state, wrong
view ownership, skipped attempt counts, replay and rollback after the local
revision write. HTTP/1 and HTTP/2 cases cover ordinary deferral, cap exhaustion,
success preservation and a stale report. The full storage suite and expanded
80-case socket matrix pass; the complete selected-admission suite also exercises
the shared ordinary-lookup retry path. Runtime scheduling/dispatch and live-event
composition remain required before task 7501 can be completed.

Generation recovery now has a single-use scheduled dispatch step bound to the
original repositories, selected transport/authentication, reset proof, operation
identity and local revision. Construction reconstructs one bounded residual
delay from retained retry facts; dispatch waits on Tokio's monotonic clock,
rechecks the scheduled revision before and after the physical pass, and routes
coherent snapshots, unanswered requests and unavailable truth through their
existing guarded persistence paths. Known-success absence requests independent
result acquisition, and conflicting snapshots request integrity recovery. No
branch resubmits or implicitly schedules another attempt.

Review closed a race between the pre-dispatch revision check and the probe's own
read, and prevented backward wall-clock samples from making subsequent durable
timestamps regress indefinitely. The residual wait remains bounded by the
original chosen delay while persistence uses a nonregressing retained time
anchor plus monotonic elapsed time. Unit vectors cover equality, partial waits,
both wall-clock directions and addition-overflow edges. Real socket cases cover
the minimum wait before any physical request, post-wait deferral, stale revision
refusal and cancellation while waiting. The cancellation fixture uses paused
virtual time and an explicit pending poll/drop rather than a racing short timer.
The existing coherent, missing, conflicting and proven-success-unanswered cases
also exercise scheduled dispatch. This supplies one scheduled pass, not yet the
recurring partition supervisor or startup/live-event composition.
Validation passed: the residual-wait unit vectors, final 86-case HTTP/1 and
HTTP/2 reset/recovery matrix, daemon test-target compilation and whitespace checks.

Supervisor review closed a dispatch fairness defect before recurrent composition:
`next_due` now removes the selected pending item, and another attempt requires an
explicit requeue after its durable outcome. Equal deadlines use queue order, so
an immediately requeued operation cannot win every tie by identifier. Refreshing
the same operation/category replaces its pending schedule instead of duplicating
it. Detachment rejects late requeues and stops dispatch without remote
cancellation. Relative fairness counters rebase at their numeric ceiling rather
than overflowing or saturating into permanent deadline-only ordering.
Tests cover consumption, repeated equal-deadline rotation, schedule/pause refresh,
late completion after shutdown, and category fairness at the counter ceiling.

Task-boundary review: the complete product runtime builder and readiness lifecycle
are explicitly task 7502, and leased executor claims are task 7504. Those must not
be silently pulled into task 7501's completion criterion. This task remains open
for complete concrete author-adapter protocol/conformance coverage; later startup
composition will own and connect the verified recovery/supervisor components.
Validation passed: 13 supervisor integration tests, the counter-ceiling unit
test, seven existing author-conformance tests, daemon test-target compilation
and whitespace checks. The existing conformance suite's simulated/pure coverage
is not by itself proof that the concrete network adapter covers every scenario.

Concrete event conformance review found that the SSE decoder accepted arbitrary
operation strings despite the committed event schema's exact 64-character
lowercase-hex identity grammar. The decoder now enforces that grammar before
delivery for nonterminal as well as terminal events; an unassociated but
well-formed identity remains valid subscription news. Valid fixture identities
were mechanically updated without changing their sequencing, corruption cases
or wire delimiters. A file-specific attribute preserves the intentional CR wire
fixture while retaining normal whitespace checks.

The closed terminal member now distinguishes omission from explicit null:
`terminal: null` is malformed, not a nonterminal event with an absent terminal
member. Refusals preserve a previously delivered heartbeat but emit no invalid
event or later item and poison further input. Event and terminal-correlation
debug views are redacted rather than exposing operation/digest/provenance data.
Six concrete HTTP/1-and-HTTP/2 socket cases verify valid event delivery and
identifier/null refusal with authenticated selected-origin requests, in addition
to decoder vectors for length, case, alphabet, Unicode and debug redaction.

The audit also confirms larger event-envelope gaps remain: the current minimal
wire/event schema does not yet carry all physical Sling job/state/attempt/progress
facts required by Plan 0005. The sibling Java encoder likewise still emits the
minimal event document; passing simulated conformance is not evidence of a fully
compatible live event path. Those protocol gaps remain task 7501 work, not a
reason to relax the selected decoder or advance the task status.
Validation passed: 25 decoder tests, all 21 provider tests, 13 event-reducer tests,
seven existing author-conformance tests, daemon test-target compilation and
`git diff --check` with the intentional-CR fixture attribute.

The complete event envelope now lives in the protocol crate as JobEventDocument;
the old JobEvent remains explicitly an identity/sequence projection, not a wire
document. The closed envelope and inventoried event schema require subscription,
physical Sling job identity and explicit logical state alongside operation,
generation, kind and sequence. Attempt and progress are optional unsigned
counters: omitted values remain None, explicit zero remains Some(0), and null or
noninteger values refuse. Terminal correlation moved into the protocol crate
without changing its public connection re-export or redacted diagnostics.

The actual SSE decoder consumes this shared wire type, checks physical-identifier
byte bounds and exact kind/state agreement, and preserves state/physical identity
and optional counters in delivered events. Schema conditionals require terminal
correlation only for terminal kinds. The event schema's recorded SHA-256 is now
197c86c60c89ba960d80d69b32038fb461f93cc49b380b2ecee68159ad02ea44.
Valid fixtures were migrated while keeping their framing/refusal intent; the
minimal old envelope is not silently defaulted into a valid current event.

Validation passed: the full protocol suite including schema inventory and new
serialization/required-member checks, 26 decoder tests, all 21 provider tests,
daemon test-target compilation and whitespace checks. The provider suite now
includes sixteen HTTP/1-and-HTTP/2 event-envelope cases; decoder vectors cover
all kind/state pairings, physical byte/Unicode boundaries, optional/max counters,
missing fields and invalid values. The sibling Java peer still requires envelope
adoption, and the durable event fold must consume optional counters without
inventing zero or losing monotonicity. Full live-event conformance remains open.

Decoded events now expose the canonical digest and UTF-8 byte count of the
complete validated event envelope, computed before its job-specific projection.
The existing command canonical-JSON writer supplies deterministic member ordering
and integer/Unicode spelling. SSE framing and JSON whitespace do not affect this
identity; omitted counters and explicit zero remain different event accounts.
This provides the full-envelope identity needed by the subscription ledger,
rather than hashing only the four-field job projection.

A decoded-event reducer entrypoint now binds the retained operation key and
generation before projecting counters. Omitted counters preserve the retained
observation, explicit regressions refuse, stale sequences remain cursor-only,
and gaps still require snapshots. Its selected revision comes from the retained
association, never the wire. This entrypoint is deliberately a pure observation
fold, not terminal settlement or a separately committed cursor update.

Validation passed: 28 decoder tests (including real decode-to-reduce vectors),
13 job-reducer tests, all 21 selected-author provider tests, and daemon test-target
compilation. Review checked that the digest covers physical identity, operation,
sequence and optional counters, that Unicode accounting uses bytes, and that
reordered/whitespace-varied JSON produces the same identity. Atomic ledger/job/
physical-association persistence and its live runtime caller remain unfinished;
these are implementation work, not an external permission blocker. Task 7501
and the overall goal remain incomplete.

The subscription ledger now has record_active_event, a single IMMEDIATE
transaction for a validated contiguous nonterminal event's cursor/accounting,
physical Sling association and remote observation. The existing cursor writer
was extracted into a transaction-local helper so all three effects roll back
together. The opaque consistent recovery view is rechecked under the write lock,
including local-owner revisions, full remote rows and physical membership; views
from another repository owner refuse even when they address the same file.
Events do not renew retention, advance snapshot watermarks or settle local work.

The transaction independently checks generation, operation association, selected
revision, contiguous monotonic observation, physical/cursor/digest bounds and
SQLite integer representability. Existing integrity incidents and authoritative
remote success refuse active mutation. The latter guard was added during review
so an active event cannot contradict stronger retained execution evidence.

The new 24-case storage matrix verifies normal/new and already-held physical
associations, three injected write failures, ledger/remote/physical/local drift,
missing and wrong owners, gaps, terminal observations, generation/key/sequence
substitution, malformed identities, integer overflow, backwards time, physical
capacity, unresolved incidents and known success. Reopened state proves rollback
of cursor/accounting, observation and physical membership; successful writes
preserve every other remote field exactly. Initial injection fixtures named the
wrong SQLite column/table; these were corrected against the migration/inventory
before the rollback claims passed. The full storage suite, expanded matrix,
daemon test-target compilation and whitespace checks passed after the final
success-evidence guard. The daemon live-event caller and remaining cursor-only/terminal
reconciliation paths still need integration; task 7501 is not complete.

The daemon now has fold_selected_event, a selected-author event consumer that
restores the exact retained submission, checks the independent local command and
selected transport binding, reduces decoded counters, and calls the atomic active
event transaction. Missing cursors and foreign subscriptions/generations refuse.
DecodedEvent now retains its validated subscription identity so the consumer can
check this itself. Gaps, terminal observations and job conflicts return explicit
reconciliation outcomes without advancing past them; terminal correlation alone
never becomes a typed result or failure settlement.

Unknown operations and believed stale/equal job sequences use the new guarded
cursor-only transaction. It rechecks the complete view under the same write lock,
requires associated sequences to be already covered, and creates neither remote
jobs nor physical associations. An eleven-case storage matrix checks unknown and
associated cursor writes, unapplied/partial associations, stale and foreign-owner
views, injected event-row failure, generation mismatch, malformed digest and
integer overflow, with unchanged jobs/physical identities in every case.

A real selected-author HTTP/1.1-and-HTTP/2 matrix now feeds the decoder directly
into this daemon consumer and SQLite: sixteen cases cover active progression,
omitted counters, gaps, explicit counter regression, terminal handoff, missing
cursor, unknown jobs, job replay and stale sequence. It verifies selected route
and authentication, rejects a foreign subscription at each consumer call, keeps
the committed prefix on later refusal, suppresses subsequent heartbeat delivery,
and preserves original retention anchors. The full storage suite, 28 decoder
tests and 13 reducer tests passed. This proves the concrete event consumer, not a
startup-owned recurring attachment loop; attachment/resolver ownership and durable
job-conflict incident/terminal recovery integration remain to be completed.

Job-event integrity conflicts now commit the subscription's bounded unresolved
incident before returning NeedsIntegrityRecovery. The storage entrypoint rechecks
the complete captured view inside an IMMEDIATE transaction and uses the existing
single-slot inventoried statement, preserving the first incident on repetition.
No cursor, event accounting, remote observation or physical association changes.
The next consumer invocation sees the durable incident and refuses streaming
progress until the existing authenticated whole-subscription reset clears it.

Review also fixed conflict precedence: different canonical contents at the held
subscription cursor are recorded as an integrity incident before job sequence
classification. A stale job event or a sequence gap can no longer hide a
subscription-position conflict. Snapshot boundaries without an observed digest
remain boundaries rather than fabricated event identities.

Validation passed: the expanded 22-case HTTP/1.1/HTTP/2 live-event matrix, the
full storage suite (including seven new incident transaction cases), daemon
test-target compilation and whitespace checks. The socket cases prove durable
job, cursor-plus-gap and cursor-plus-stale conflicts across database reopening;
storage cases prove repeat idempotency, stale ledger/physical views, foreign
repository ownership, injected incident-write rollback and cursor validation.
Recurring attachment/resolver ownership and terminal recovery integration remain
unfinished; neither task 7501 nor the overall goal is marked complete.

attach_selected_events now owns one selected-author event attachment end to end:
it loads the subscription generation and committed cursor (including a captured
snapshot boundary), preflights exact retained submissions and independent local
commands before connecting, resolves each terminal event from fresh retained
state, and feeds every decoded event to the durable consumer. Callers can no
longer supply a fabricated terminal resolver or cursor to this daemon entrypoint.
HTTP/1.1 and HTTP/2 are explicit; reset evidence is returned intact, and recovery
outcomes carry the remote operation key without authorizing settlement/resubmit.
Persisted incidents stop before any socket. Event timestamps conservatively add
monotonic elapsed time to the supplied wall-clock anchor with checked arithmetic.

Review removed repeated whole-subscription reads during member preflight: one
captured view supplies membership, while terminal-event resolution still reads
fresh membership and local command evidence. The live-event matrix now executes
all eleven scenarios through both the direct consumer and the owned attachment,
over both protocols (44 cases). Owned cases verify the stored boundary cursor
appears in the request, persisted conflicts prevent a subsequent socket, and
retained contract drift refuses before reconnecting without changing the ledger.
The expanded matrix, daemon test-target compilation and whitespace checks pass.
This is a single owned attachment, not the recurring startup supervisor; terminal
lookup dispatch, recurrence and runtime ownership remain open work.

Terminal-recovery routing review found a prerequisite gap: capability discovery
was exposed only over HTTP/1.1 even though event attachment and logical lookup
support explicit HTTP/2. SelectedAuthorTransport now provides
discover_capabilities_http2 through a shared discovery implementation. Both modes
derive required contracts from the installed build, bind the selected execution,
perform one authenticated finite GET and apply identical generation/readiness/
contract decoding. There is no fallback request or caller-selected digest.

The same review found that a 200 capability response with Location was accepted;
both discovery modes now reject that redirect hint without following it. The
existing real selected-submission provider test now exercises forty capability
exchanges across both protocols (compatible, wrong generation, unready authority
and Location cases), checking the exact selected route and authentication bytes.
The expanded targeted test, all 21 provider tests, daemon test-target compilation
and whitespace checks passed. This closes
the discovery prerequisite, not the remaining terminal-dispatch or artifact
transport-mode wiring; task 7501 and the full goal remain incomplete.

Artifact staging and retained snapshot completion now expose explicit-mode
entrypoints. Existing public callers retain HTTP/1.1 behavior; the shared private
staging path dispatches to artifact_http1 or artifact_http2 once and never falls
back. The mode passes through retained-result completion to the actual download,
without duplicating capacity reservation, acquisition anchoring, canonical/typed
document validation, digest checking, private staging or settlement logic.

Seven new HTTP/2 staging cases exercise valid bytes, truncation, changed digest,
surplus content, wrong media, Location and pre-network capacity refusal. Requests
are authenticated and checked against the exact operation-scoped artifact route.
Review corrected two test assumptions against the existing contract: the route
is operation-scoped, and successful staging intentionally retains a durable
publication hold after its handle is dropped. Cases now use isolated databases;
successful staging retains exactly that hold without publishing a content file,
while rejected transfers leave no reservation or committed-content charge.
The expanded staging test, all ten selected-author daemon integration tests,
daemon test-target compilation and whitespace checks passed. Explicit mode is available through completion,
but durable lookup/terminal dispatch still need to select and pass it; task 7501
and the full goal remain incomplete.

Durable lookup now exposes lookup_retained_operation_over. One explicit mode is
threaded through capability discovery, logical snapshot lookup and retained-result
artifact completion; existing entrypoints retain their HTTP/1.1 default. All
revision, command, provenance, retry, retention and terminal-evidence checks stay
in the shared reconciler. A failed HTTP/2 exchange does not fall back or resend.

Review extended this to captured evidence: CapturedResetRecovery and
PhysicalRecoveryReport now privately retain the mode used to obtain their
snapshots and pass it into common reconciliation. Bypassing discovery for already
captured evidence no longer silently restores an HTTP/1.1 artifact download.

The new package-recovery integration test runs the complete successful recovery
over three real HTTP/2 connections: installed capability discovery, terminal
snapshot lookup and exact operation-scoped artifact acquisition. Every request
is checked for the selected route and authentication, and the existing fixture's
local/remote settlement and publication assertions prove the end state. The
targeted test, all eleven selected-author integration tests, daemon test-target
compilation and whitespace checks passed. Review added an explicit HTTP/2 artifact
delay so the retention deduction assertion cannot depend on incidental socket
latency; the targeted test passed again afterward. Terminal-event dispatch
still needs to invoke the recovery boundary, and automatic protocol negotiation
and recurring runtime ownership remain unfinished task 7501/7502 work.

Owned attachments now return a sealed, non-Clone CapturedTerminalEvent instead
of only a terminal-recovery reason. It retains the original database owners,
submission, local revision, subscription view, event sequence/kind/cursor,
physical identity, optional counters and explicit HTTP mode. Capture rechecks
the event correlation against the restored submission and refuses paused or
changed work. Diagnostics expose none of that retained wire context.

Consuming the handle performs selected capability discovery and logical lookup,
rechecking retained state before each request and after the snapshot. It admits
only a snapshot of the same terminal kind at or beyond the event sequence and
subscription cursor, with monotonic counters and the complete held/event physical
identity set. Active, older, absent and contradictory accounts do not settle the
event. Valid evidence enters the existing typed result/failure reconciler without
another lookup or POST, preserving the chosen artifact mode. Checked monotonic
elapsed time ages local timestamps and post-response retention conservatively.
No terminal-event cursor is installed by this handoff.

The live-event matrix now has 72 direct/owned HTTP/1.1/HTTP/2 cases. Owned terminal
cases exercise successful no-result recovery into authoritative-success/result-
acquisition state, older/active/behind-watermark/missing-physical snapshots,
changed local revision/physical membership and a different repository owner.
Preflight failures open no socket; snapshot refusals preserve local and remote
facts; all leave the stream at its committed prefix. The expanded matrix, all
eleven selected-author integration tests, daemon test-target compilation and
whitespace checks passed. Failed-exchange retry scheduling, terminal
result/failure variants through this specific handoff, cancellation coverage and
recurring supervisor integration remain open; this is not task/goal completion.

Review identified a specific remaining terminal-settlement replay issue: the
attachment resolver currently reads only unsettled recovery membership. Once
typed reconciliation fully settles the local owner, that member is no longer in
the view, while its terminal event cursor was deliberately not advanced. The
resolver must still authenticate the retained completed operation on replay (or
an equally crash-safe cursor/settlement design must handle it). The new no-result
success case stays unsettled and does not prove this completed-owner path. This
follow-up remains within task 7501 and must be resolved before advancing tasks.

The completed-owner replay follow-up is now implemented. CompletedEventView reads
the ledger, terminal remote child, terminal local owner and physical identities
consistently without adding completed work to unsettled recovery membership. The
terminal resolver falls back to this retained evidence and still restores the
exact submission and independent local command. The consumer reduces completed
events against their retained terminal observation instead of treating them as
unknown jobs. New job progress cannot reopen completed work.

Validated stale/equal replays use record_completed_event_cursor: one IMMEDIATE
transaction rechecks the complete completed view, generation, sequence coverage,
physical identity, cursor and accounting bounds, and writes only the event ledger.
Changed counters remain an integrity conflict, not a replay. Full terminal result
settlement can therefore survive a crash before cursor commit and authenticate
the same terminal event after reopening, without repeating local settlement.

The live matrix now has 80 direct/owned protocol cases, including an inline
query-result terminal recovery that fully settles local and remote work, leaves
recovery membership empty, reopens storage, replays through a real attachment and
advances only the cursor. A subsequent contradictory replay records an incident
without altering the settled result. Eleven new storage cases cover valid/stale
completed replay, uncovered sequences, generation/physical mismatch, ledger/
physical/local drift, foreign view ownership, injected write rollback and another
subscription. A second completed path uses a validated query failure with known
nonexecution; it remains rejected after replay and detects a later contradictory
event, with no fabricated inline result. The injection fixture's revision column was corrected to the actual
migration name before its race assertion passed. The full storage suite, expanded
live matrix and daemon test-target compilation passed. All eleven selected-author
integration tests passed before the final failure-replay extension, and the
expanded event matrix passed again afterward. Broader terminal variants,
failed-exchange scheduling, cancellation and recurring supervision still require
their remaining task coverage; the overall goal remains active.

Terminal-event recovery now honors a retained retry delay before its first
request, using a monotonic deadline and only the unelapsed persisted duration.
Forward wall-clock movement consumes the delay; backward movement cannot erase
it. The persisted timestamp anchor never falls behind the prior retry fact.
Captured ownership is checked both before and after the wait, and dropping the
waiting future sends no request and charges no retry attempt.

Failed capability/lookup exchanges and unusable snapshot observations now record
one attempt through the existing complete-view guarded recovery transaction.
The shared retry policy preserves stronger execution evidence, chooses bounded
jitter, keeps known-success work in result acquisition, and pauses at the
automatic cap without another request. Stale handles and wrong owners still
refuse without charging another operation. No failure advances the event cursor,
changes the remote child or converts uncertain work into a terminal disposition.
This deliberately extends the earlier snapshot-refusal behavior: its only new
local effect is durable retry accounting, rather than an unlimited uncharged
sequence of unusable snapshots.

The event matrix now has 100 direct/owned HTTP/1.1/HTTP/2 cases, including errors
at each lookup stage, known-success preservation, cap exhaustion and cancellation
while waiting on a retained delay under a backward wall-clock reading. A unit
matrix covers partial/elapsed/zero/backward delay arithmetic. All eleven selected-
author integration tests passed before the final unusable-snapshot accounting
extension; the complete 100-case event matrix passed again after that extension.
The delay unit test, daemon test-target compilation and whitespace checks pass.
Recurring supervisor ownership and remaining terminal/cancellation variants are
still open task work, not a reason to mark the goal complete.

Review found that a valid successful snapshot without a terminal result could
repeat indefinitely without consuming acquisition attempts. The shared durable
lookup reconciler now records one bounded acquisition attempt after preserving
authoritative remote-success evidence. This applies equally to direct lookup,
captured terminal-event recovery, reset handoff and physical recovery. It does
not settle the local operation, advance the event cursor, change the remote job
or authorize a replacement submission. Known success is preserved at exhaustion.

The live-event matrix now contains 108 direct/owned HTTP/1.1/HTTP/2 cases,
including first missing result, repeated missing result and acquisition-cap
exhaustion. Direct repeated-lookup and generation-handoff expectations now check
the additional durable retry revisions instead of expecting uncharged success.
Verification: all eleven selected-author integration tests passed, including
the complete 108-case event matrix; all fifteen executor tests and seven
existing simulated conformance tests passed. Daemon test-target compilation and
whitespace checks also passed. These regressions do not claim completion of the
remaining concrete-adapter conformance or startup composition work.

The terminal acquisition-cap event vectors now also reconstruct the concrete
`RetainedAuthorProtocol` and `ProductAuthorPorts` over reopened SQLite handles
and drive the actual operation executor. Both the original invocation and a
later clock/higher-attempt invocation must return the entire persisted recovery
fact unchanged, without any accepted socket, local/remote mutation or cursor
advance. This closes a concrete-adapter coverage gap left by scripted-port retry
tests; it does not claim automatic scheduling or startup ownership is installed.
The complete 108-case live-event matrix, daemon test-target compilation and
whitespace checks passed after adding these concrete restart assertions.

The selected transport now exposes guarded fresh-token submission over HTTP/2.
Both protocol modes share derivation/argument checks, token parsing, the final
durable-owner callback, exact request construction, acknowledgement validation
and transport-phase uncertainty mapping. The HTTP/2 path performs one token GET
and at most one POST, without retry or fallback. It uses the existing bounded
duplex HTTP/2 driver rather than a separate network client.

Twenty new real-socket cases cross the five existing media/identity fixtures
with valid submission, invalid token, final-guard refusal and truncated POST
response. They inspect the selected routes, authentication, token/idempotency/
origin fields and exact retained wire bytes, and reject additional sockets.
Protocol selection still needs to reach durable first-send admission and the
concrete runtime adapter; this transport entry point alone is not completion.
All twenty-one provider tests passed, including the new submission vectors;
daemon test-target compilation and whitespace checks passed as well.

Durable admission now carries the chosen HTTP mode through pre-admission
capability discovery, the consumed first-send permit's second discovery,
fresh-token acquisition and its one POST. `RetainedAuthorProtocol::new_over`
holds that mode privately and uses it for submission, logical lookup and
artifact completion; legacy constructors retain their HTTP/1.1 default.
All existing local revision, immutable command, outbox and acknowledgement
guards remain shared, and no protocol branch issues fallback or another send.

The concrete admission fixture now adds HTTP/2 accepted, lost-acknowledgement
and token-time local-revision-race cases. It observes SQLite before each request,
checks exact submitted bytes and physical association, and reopens successful
or uncertain admission through the default HTTP/1.1 adapter to prove that a
mode change does not grant another POST. The HTTP/2 package completion fixture
now drives the real executor and product adapter instead of the lookup helper,
covering capability discovery, snapshot and artifact retrieval on that mode.
Automatic negotiated protocol selection and startup composition remain open;
explicit immutable mode propagation does not claim those requirements complete.
Verification: all eleven selected-author integration tests, fifteen executor
tests and seven existing simulated conformance tests passed. Daemon test-target
compilation and whitespace checks also passed after the review edits.

The selected connector now offers an automatic TLS ALPN mode containing only
`h2` and `http/1.1`, cloned from the same frozen trust/provider/version policy
as the explicit modes. A sealed, non-cloneable negotiation result retains the
original socket alongside its accepted codec choice; consumers can take that
pair without a probe/reconnect race. A protected peer without ALPN and permitted
cleartext select HTTP/1.1, without upgrade bytes. Unsupported ALPN refuses;
explicit HTTP/2 still requires h2 and cannot silently downgrade.

The TLS fixture now covers fourteen selections for each of TLS 1.2 and TLS 1.3,
including server preference, single supported protocol, absent ALPN and h3-only
refusal, and asserts no HTTP bytes or second connection. A separate cleartext
fixture proves HTTP/1.1 selection without upgrade/probe bytes. The HTTP response
drivers still need to consume this same negotiated socket before the concrete
adapter can use automatic mode; this connector change does not claim that
end-to-end dispatch or startup composition is finished.
All twenty-two provider tests passed, including these negotiation fixtures;
daemon test-target compilation and whitespace checks passed as well.

Finite automatic dispatch now consumes the original negotiated socket. HTTP/1.1
shares its existing write/head/body path with explicit mode; HTTP/2 negotiates
SETTINGS and drives its existing duplex finite consumer on that same socket.
Request encodings are checked before connecting, with both-invalid input refused
immediately. Review corrected an overly restrictive first version: only the
selected encoding must pass its protocol-specific bound, not the unselected one.
Retention timing includes connection and protocol negotiation, and neither branch
can reconnect, retry or switch protocols after an exchange failure.

The provider tests now drive automatic finite requests through the TLS 1.2/1.3
HTTP/1.1 trust/hostname matrix, TLS HTTP/2 exact authenticated POST and clean/
unclean shutdown fixtures, and the complete cleartext HTTP/1.1 framing matrix.
Each checks that no second connection is accepted. Route-level automatic mode,
stream/artifact automatic dispatch and concrete startup remain unfinished work.
All twenty-two provider tests, daemon test-target compilation and whitespace
checks passed after the preflight review and additional framing coverage.

Capability discovery and guarded fresh-token submission now expose negotiated
route entry points. They delegate to the same installed-contract, generation,
token, exact request and acknowledgement validators as the explicit modes.
Token and POST connections each use their own accepted ALPN codec on their
original socket; a negotiated choice never authorizes retry or replacement
submission. The durable final callback still runs after the token and before
opening the POST connection.

The capability matrix adds automatic cleartext compatibility/generation/readiness/
Location cases. Twenty TLS-negotiated HTTP/2 sequences now run capability
discovery, token acquisition and at most one POST across the existing media/
identity fixtures and token/guard/truncated-acknowledgement defects. They check
selected routes, credentials, origin/token/idempotency fields, exact retained
wire bytes, clean TLS completion and absence of any additional connection.
Automatic logical lookup, event/artifact transport and durable runtime mode
composition remain to be implemented before automatic operation is complete.
All twenty-two provider tests passed after extending the TLS sequence to include
discovery; daemon test-target compilation and whitespace checks also passed.

Logical lookup, physical snapshot retrieval and physical lookup with separately
observed-generation absence now expose negotiated route entry points. All modes
share selected submission preflight, physical identifier validation, fixed route
construction, response framing/media/status gates, snapshot identity and closed
absence decoding. Negotiation neither changes the retained generation nor
permits another POST; the same finite socket produces the evidence and elapsed
retention accounting.

The real TLS h2 sequence now continues discovery/token/submission into a logical
snapshot GET, checking the complete operation query, progress/sequence, physical
association and bounded remaining retention. The physical lookup matrix adds
automatic-mode invalid identifiers/revisions/zero generation and successful,
contradictory, truncated and closed-missing responses; logical Missing/Retired
fixtures also run in automatic mode. Automatic stream/artifact transport and
durable runtime mode composition remain open task work.
All twenty-two provider tests passed with the extended lookup fixtures; daemon
test-target compilation and whitespace checks passed as well.

Artifact acquisition now has a negotiated entry point that dispatches its
original socket to the existing HTTP/1.1 stream consumer or HTTP/2 duplex
artifact consumer. Shared preflight binds the selected submission, artifact
slot/media/digest/length and artifact identifier before connection. The chosen
codec retains bounded streaming, private sink ownership, exact length/SHA-256,
framing/trailer checks and closed identity-bearing 404/410 validation; elapsed
retention accounting includes connection and negotiation. Neither path buffers
successful artifacts as finite JSON or reconnects after a transfer refusal.

The artifact socket matrix now contains thirty-two cases across explicit h2,
negotiated TLS 1.2/1.3 h2 and negotiated cleartext HTTP/1.1. Each covers successful
bytes, digest mismatch, trailers, sink refusal and valid/mismatched/bare absence;
requests bind the selected path and authentication, unavailable documents never
enter the sink, and no additional connection is accepted. Negotiated identity
and empty-artifact-identifier preflight also refuse before any socket. Automatic
event streaming and durable runtime mode composition remain unfinished.

Follow-up review found that the shared HTTP/1.1/HTTP/2 stream unit fixture still
used a noncanonical operation identifier and omitted the required physical job
and state fields. It now supplies the current complete event envelope; the
wrong-operation vector changes only the operation identifier and keeps its
other correlation fields unchanged. This repairs the test input rather than
weakening the production decoder. The complete provider suite (22 tests) and
daemon selected-author integration suite (11 tests) passed for the artifact
change; all 95 connection unit tests passed after the shared-fixture repair.

Event attachment now exposes negotiated transport while retaining the original
socket. HTTP/1.1 shares its request/receive/classification path; h2 shares its
SETTINGS, duplex response consumer, heartbeat deadline and per-operation terminal
resolver. Both modes preflight the selected subscription/generation/cursor and
require the selected encoding before application bytes. Neither transport can
persist a cursor, mutate a job or issue a reconnect when the attachment ends.

The event/reset matrix now has sixty-five cases across explicit HTTP/1.1/h2,
negotiated cleartext HTTP/1.1 and negotiated TLS 1.2/1.3 h2. Valid events preserve
optional counters; malformed envelopes deliver only the prior heartbeat; reset
responses require matching generation/cursor evidence. The live request checks
the exact encoded subscription query, committed cursor and authentication;
each attachment/reset rejects an additional socket. Invalid selected request
preflight also runs through automatic mode. Negotiated high-water capture and
automatic mode in durable/runtime composition remain unfinished task work.
Verification: all 95 connection unit tests, 22 provider tests and 11 daemon
selected-author integration tests passed. Daemon test-target compilation and
whitespace checks passed after the final query/cursor/no-retry assertions.

Negotiated high-water capture now shares the fixed selected query, JSON media,
captured-generation and identity-bound reset gates with explicit HTTP modes.
Its thirty-five-case matrix crosses HTTP/1.1, h2, automatic cleartext and automatic
TLS 1.2/1.3 h2 with valid capture, generation disagreement, truncation, validated
reset, bare reset and non-reset error statuses. Invalid selection/subscription/
generation refuses before connection; no result causes a retry or cursor install.

Automatic mode is now propagated through durable admission and its consumed
send permit, logical lookup, artifact staging/completion, owned event attachment,
captured terminal recovery, reset high-water/snapshots and generation-loss
physical probing. `RetainedAuthorProtocol::new` now selects this policy by
default; explicit-mode constructors remain available. Sealed recovery handoffs
retain that policy, while each request uses its original negotiated socket.

The durable event matrix now has 162 direct/owned/transport combinations and
the complete reset/generation matrix also includes automatic mode. A concrete
automatic first-send case verifies capability-before-admission ordering, exact
outbox bytes, physical association and no resend after reopening. Remaining
authentication policy and full concrete-adapter conformance are still open
7501 requirements, not external blockers. Runtime startup ownership remains
the following task, 7502, rather than a prerequisite for closing this adapter.
Verification: all 95 connection unit tests, 22 provider tests, 11 selected-author
integration tests, 15 executor tests and seven existing simulated conformance
tests passed. The automatic first-send case added during review passed in a
separate rerun of its containing integration test. Daemon test-target compilation
and whitespace checks pass.

Authentication-boundary review found that request authorization bytes had no
selected target/revision binding. Basic and Cloud credentials, including forced
refresh results, now carry their immutable provider selection privately. Both
HTTP encoders require that binding before request serialization/connection, so
all explicit and negotiated finite, artifact and event paths inherit the check.
The test rebuilds equivalent selections and separately changes principal/target
or only publisher metadata/revision, proving only equivalent credentials are
accepted and mismatches open no socket. Debug output remains fully redacted.

The same review found that a lease from another cache identity could retrieve
the receiving cache's usable token or initiate its exchange. Forced refresh now
refuses that lease before locking, exposing bytes, invalidating a token or
calling the source. Empty and populated-cache tests pin the stable target-
mismatch failure and unchanged exchange count/generation. Cache identity, lease
and cache Debug implementations now redact identifiers and generations rather
than relying on derived output; the old redaction assertion only searched for
a token string and did not prove this requirement.

These fixes close prerequisite binding/refusal gaps. Request-scoped refresh and
401 handling, plus full concrete-adapter conformance, remain open 7501 work.
Final credential-construction review also replaced the temporary unprotected
Base64 String with a `SecretValue` buffer before prefix assembly, preserving
exact Basic wire bytes while scrubbing the intermediate credential transform.
Verification: all 95 connection unit tests, seven token-cache tests and eleven
daemon selected-author integration tests passed. All 23 provider tests passed
again after the final Basic buffer cleanup; daemon test-target compilation and
whitespace checks passed as well.

Request-scoped provider authentication now has a finite GET-only transport
entry point. It checks the provider's immutable target/revision before invoking
the token source, sends on the original negotiated connection, and permits
exactly one refreshed GET after a completely validated Cloud 401. Basic 401,
403, malformed responses and failed refresh never produce another request;
a second 401 is returned without recursion. There is no method/body parameter
that could turn this policy into a job POST retry. Elapsed time conservatively
includes initial authentication, the first exchange and refresh as well as the
final exchange, and no background retry task is spawned.

Loopback TLS tests cover ten Basic/Cloud response scenarios, distinct old/new
Bearer values, byte-identical request routing/fields across refresh, terminal
second rejection, refresh failure, truncated bodies and absence of extra
connections. Selection tests additionally prove foreign Cloud providers are
refused before any token exchange. Capability discovery exposes this provider-
authenticated path with the same shared final compatibility/readiness/generation
decoder as the existing explicit and automatic transport entry points.

This is implementation progress, not a new external blocker or completion of
7501. Other route consumers and the retained product adapter still need provider
ownership/refresh integration; job POST uncertainty must remain lookup-first.
The existing synchronous token source/cache does not yet establish cancellation-
safe asynchronous exchange ownership. Full concrete-adapter conformance also
remains open. Verification: 95 connection unit tests, seven token-cache tests
and 24 provider tests passed before the capability entry point was added; its
expanded capability fixture is separately rerun, and daemon test-target
compilation and whitespace checks pass.

Logical lookup, physical snapshot and current-generation physical-job lookup
now expose provider-authenticated GET entry points. Existing explicit and
negotiated calls share the extracted request preflight and final response
decoder with these paths. Invalid submission binding, physical identifiers and
zero current generations are refused before authentication; final snapshots,
logical tombstones and physical missing proofs retain all prior identity,
provenance, physical-membership, generation and retention checks. Refresh time
is included in the remaining-retention debit, never a renewed budget.

The physical-response matrix includes the provider path for ten response
vectors plus invalid identifier/revision/generation preflights, and the logical
absence matrix includes provider-authenticated missing/retired documents.
An end-to-end TLS Cloud logical lookup independently observes distinct Bearer
generations around 401, an identical second GET, and a validated same-operation
missing document. Basic performs no retry and its 401 remains a lookup refusal.
All 24 provider tests passed; the final expanded Cloud/Basic refresh test also
passed separately. All eleven concrete daemon selected-author integration tests
passed, including the live-event and reset matrices; daemon test compilation and
whitespace checks pass. Retained-adapter credential ownership and remaining
route integration are still open, not claimed complete by these route tests.

The submission transport now exposes a provider-authenticated guarded entry
point: selected submission preflight, bounded authenticated CSRF GET, the shared
closed token decoder, current provider credentials, final durable guard, then
one negotiated POST. A Cloud token-GET 401 may refresh and repeat that GET once;
the POST is never passed through the read retry helper. A completely received
POST 401 refreshes its Cloud lease for later lookup but always returns
SubmissionUnknown/LookupRequired, including when refresh fails. Basic cannot
refresh, and 403 never initiates token refresh or a repeated POST. Existing
explicit submission modes share this post-byte uncertainty classification.

The TLS authentication matrix now covers 24 Basic/Cloud scenarios, including
exact CSRF/idempotency headers and unchanged retained POST bytes, token-GET
refresh followed by one POST, rejected POSTs, failed post-rejection refresh,
empty tokens and final-guard refusal. Every scenario proves absence of extra
connections. Timing review moved the final request timestamp observation after
credential acquisition and the durable guard. These route-level changes do not
yet change RetainedAuthorProtocol's fixed-credential ownership; connecting the
provider paths through the durable coordinators remains open 7501 work.
Verification: all 95 connection unit tests and 24 provider tests passed before
the final malformed-token/guard-count additions; the expanded authentication
matrix passed again after those additions and the final timestamp correction.
All eleven concrete daemon integration tests passed (including event and reset
matrices), and daemon test-target compilation and whitespace checks passed.

Durable initial admission and the consumed initial-send permit now accept an
invocation authentication policy. Its provider branch borrows the immutable
provider, token source and source-domain monotonic clock, sampling a new reading
for each capability/submission phase rather than freezing a token for the
invocation. Its explicit fixed-credential branch preserves HTTP/1.1, HTTP/2 and
automatic callers. Both branches run the same local revision gates, capability
before admission, exact child persistence, capability after admission, final
token guard and durable physical acknowledgement; there is no duplicated
admission implementation or new send permission.

Three additional concrete SQLite/socket cases exercise provider acceptance,
lost acknowledgement and local revision drift during CSRF acquisition. They
assert the capability/persistence ordering and three phase-clock samples, exact
POST bytes, and physical association/uncertainty. Reopening an accepted or
uncertain child returns lookup-required without sampling authentication time or
opening a socket. The shared coordinator and the existing retained adapter's
fixed-credential path both remain covered. RetainedAuthorProtocol still needs
its provider branch connected through lookup and artifact/event recovery before
this constitutes complete runtime credential ownership.
Verification: the expanded admission test passed individually and all eleven
concrete selected-author integration tests passed together. All fifteen executor
tests and seven existing simulated conformance tests passed. Daemon test-target
compilation and whitespace checks passed; the conformance tests remain explicitly
insufficient evidence for completion of full concrete runtime composition.

Artifact transport now distinguishes a fully framed bounded JSON 401 from a
failed transfer. Both HTTP codecs collect that refusal under finite limits,
require complete framing/transport termination, and write none of it to the
artifact sink. The provider artifact entry point validates the selected
submission/manifest/identifier before authentication, retries one Cloud GET only
after that proof, and refuses Basic/second 401s. Partial transfers, malformed
401s, digest errors and sink refusal cannot refresh or retry. Verified transfer
and unavailable receipts include the entire authentication/refresh elapsed time.

The explicit/negotiated/provider artifact matrix now covers 55 cases, including
401 trailers and truncated framing on HTTP/1.1 and HTTP/2/TLS. The TLS
authentication matrix additionally proves Cloud 401-to-artifact success, second
401 termination, malformed 401 no-refresh and partial artifact no-retry, with
exact repeated routes, changed Bearer generation and private sink observations.

AuthorAuthentication now dispatches artifact staging as well as initial
admission. Private staging accepts that policy only after capacity reservation
and its existing pre-request guard; old explicit callers share the same staging
implementation. Its concrete provider test proves no authentication clock sample
on capacity refusal, cancellation cleanup, closed loaded-document validation,
unavailable evidence and durable publication holds. Explicit HTTP/2 fixtures
retain their separately held credentials. Retained snapshot completion still
enters through its fixed policy wrapper: connecting provider ownership through
that completion and the lookup coordinator remains open, not a blocker requiring
external action.
Verification: all 95 connection unit tests and 24 provider tests passed; the
final second/truncated artifact-401 additions passed in a separate rerun of
the expanded authentication test. All eleven concrete daemon integration tests
passed after correcting the explicit HTTP/2 test credential references during
provider-staging migration. Daemon test compilation and whitespace checks pass.

RetainedAuthorProtocol now stores the invocation authentication policy rather
than fixed authorization bytes plus a separate protocol selector. Its new
policy constructor shares all existing database/capacity/command binding checks;
legacy constructors wrap their fixed credentials and transport choice. The
provider path is wired through durable admission, capability discovery, logical
lookup, retained snapshot completion and manifest-bound artifact staging. It
does not cache a bearer value across those phases. Captured reset/physical
handoffs retain their existing fixed policy while using the same completion
implementation; their separate provider integration remains open.

Provider admission tests now execute through ProductAuthorPorts and the retained
adapter instead of only calling the coordinator. Acceptance, lost ack, final
revision race and reopened-child no-request behavior remain pinned, with no
authentication clock sampling on product restart. Concrete package and loaded
completion now exercise provider-backed capability/lookup/artifact acquisition
through the real executor, checking three fresh phase-clock samples and durable
publication/settlement. Terminal-failure and failed-lookup retry-cap fixtures
also use the provider adapter, preserving strongest recovery evidence and the
existing durable retry counters.

This closes the fixed-credential ownership gap for ordinary retained invocation
submission and settlement. Event attachment, reset and generation/terminal
handoffs still need provider policy propagation; asynchronous token exchange
ownership and full concrete-adapter conformance remain open 7501 requirements.
Verification: all eleven concrete selected-author integration tests passed;
the final product-restart no-authentication assertion passed in a separate
admission-test rerun. All fifteen executor tests and seven existing simulated
conformance tests passed, as did daemon test-target compilation and whitespace
checks. These checks do not establish completion of the remaining event/runtime
composition requirements.

High-water capture now has a provider-authenticated entry point that reuses the
same selection/subscription/generation preflight and final high-water/reset
decoder. One Cloud 401 may refresh the identical fixed-route GET; returned
positions remain capture evidence, never permission to install a cursor. The
42-case high-water transport matrix includes the provider path over TLS/HTTP2.
An end-to-end Cloud test verifies distinct Bearer generations, exact unchanged
subscription/generation query, no Last-Event-ID and the final validated capture;
Basic returns its single unauthorized response without retry.

Subscription reset now accepts the invocation authentication policy for both
high-water and every member lookup. It preserves complete durable-view checks,
staged atomic installation, cancellation and per-member revision checks. A
captured reset recovery retains that policy through its single-use handoff and
can reconcile with no separately borrowed credential or extra logical GET/POST.
Legacy fixed-credential reconciliation still accepts its explicitly supplied
current credential; a saved provider policy cannot be replaced by that argument.
The provider reset policy is included in the full concrete reset/race matrix,
including missing/terminal handoff reconciliation through the saved policy.

Generation-loss probing and terminal-event/event-attachment policy propagation
remain open; generation scenarios in this matrix use the provider for initial
reset capture but still use the existing fixed policy for their later probes.
Asynchronous token ownership and full concrete-adapter conformance remain open
as previously recorded. These are implementation requirements, not external
blockers or evidence that plan 20 is complete.
Verification: all 95 connection unit tests, 24 provider integration tests and
eleven concrete daemon selected-author integration tests passed, including the
expanded reset matrix and saved-provider handoffs. Daemon test-target compilation
and whitespace checks passed.

Generation-loss probing now accepts the invocation authentication policy and
uses provider-authenticated physical lookup for every retained identifier. The
complete-set walk, before/after durable-view checks, deterministic coherent
snapshot selection and failure-versus-absence classification are unchanged.
PhysicalRecoveryReport retains its policy; recovered evidence can reconcile
through that saved policy without a new physical/logical lookup or POST.
Fixed callers retain the compatibility entry point for supplying a current
bound credential; a saved provider is not replaceable through that argument.

ScheduledGenerationRecovery now stores the same policy and carries it through
the one delayed probe pass and recovered result completion. Residual-delay
reconstruction, cancellation, stale-owner refusal, unanswered charging and
fail-closed missing decisions share the existing implementation. The provider
mode in the concrete generation matrix now covers its later probes, scheduled
dispatch and report reconciliation as well as its initial reset capture.

A Cloud TLS physical-query test independently verifies one 401 refresh, exact
repeated Sling identifier routing, a validated missing proof in current
generation 8 and unchanged retained submission generation 7. Basic performs no
retry and the unauthorized answer remains a lookup refusal. Event attachment
and terminal-event policy propagation, asynchronous token ownership and full
concrete conformance remain open; this does not complete task 7501 or plan 20.
Verification: all eleven concrete daemon selected-author integration tests,
fifteen executor tests and seven existing simulated conformance tests passed.
The expanded TLS authentication test passed, including the new physical lookup
case. Daemon test-target compilation and whitespace checks passed.

Event attachment now has a provider-authenticated transport entry point. It
checks selection/subscription/generation/cursor before authentication, permits
one Cloud refresh only for a complete bounded JSON 401, and repeats the exact
committed cursor and query. Stream delivery or malformed framing never enters
that retry branch; prior delivered heartbeats/events are not retracted. Basic
and second 401 responses terminate the request without another attachment.

The daemon-owned attachment and its captured terminal ticket now retain the
invocation policy. Terminal recovery samples credentials after its persisted
delay and repeated current-owner checks, uses the shared capability/logical
lookup paths, and carries the policy into captured snapshot completion. Existing
fixed-credential callers retain their explicit mode/current-credential interface;
a saved provider cannot be replaced through that compatibility argument. The
now-unused fixed-only captured-lookup helper was removed after compiler review.

The concrete direct/owned live-event matrix now has 216 combinations, including
provider mode, completed replay/conflicts, missing terminal results, retry caps,
known success, cancellation during waiting, local/physical races and wrong-owner
refusals. The transport event matrix includes provider TLS/HTTP2 envelope and
reset validation. Cloud TLS tests independently prove 401-to-stream refresh,
exact cursor preservation, terminal second 401, malformed-401 no-refresh, and no
retry after a heartbeat followed by a partial stream.

Provider policy propagation now reaches ordinary retained execution, reset,
generation loss, event attachment and terminal recovery. This is not completion
of task 7501: cancellation-safe asynchronous token-exchange ownership and the
full concrete-adapter conformance audit remain required before task closure.
Verification: all 95 connection unit tests, 24 provider tests and eleven concrete
daemon integration tests passed; daemon test-target compilation and whitespace
checks passed without the removed helper's dead-code warning.
All fifteen executor tests and seven existing simulated conformance tests also
passed; those simulated tests do not replace the remaining concrete audit.

The asynchronous token-cache foundation now owns each exchange future directly:
no mutex spans an await and no exchange task is detached. Concurrent callers
join one completion channel. Dropping the owner drops the exchange, clears the
flight and wakes all joined callers with the same cancellation failure; dropping
only a waiter leaves the exchange running. Failed refreshes cannot serve the old
token, and an unusable returned token fails the flight without recursively
starting another exchange. Allocation-owned opaque lease identities refuse
foreign-cache invalidation before calling the source; stale leases cannot evict
a newer installed generation. Generation exhaustion fails before exchange.

Deterministic polling tests cover eight simultaneous success/failure callers,
actual waiter wakeups on cancellation, a later successful flight that cannot
overwrite earlier cancellation, waiter-only cancellation, stale/foreign leases,
redacted debug output, no fallback and no recursive short-token exchange.
All 96 connection unit tests and 12 cache integration tests passed. The first
sandboxed unit-suite attempt hit the existing loopback-listener restriction;
the same suite passed with scoped local-network permission.

This is a verified foundation, not runtime migration: the environment provider
and selected-author routes still use the synchronous source/cache API. They must
be migrated to cancellation-owned exchange without creating a second independent
cache authority. Concrete async exchange transport and full adapter conformance
remain implementation work in task 7501, not external blockers or task closure.

An asynchronous service-credential source now connects real assertion construction
to one awaited identity-management exchange and the existing closed response
validator/lease installer. Its anchor precedes transport invocation; its receipt
sample follows awaited completion, so transfer time consumes the advertised
lease. Assertion refusals precede transport invocation and no failure causes a
retry. The cache owns this source future and cancellation drops the pending
transport future while notifying joined callers.

Review also removed plain temporary credential copies from form construction.
Encoding borrows the assertion/client secret, computes a bounded exact capacity,
allocates once, and retains the completed form in SecretValue across the await.
Accepted response tokens enter SecretValue before token-type, byte and lifetime
checks, so later rejection does not leave that parsed token in an ordinary
string. The shared synchronous path uses the same form and validation changes.

New source tests verify exact ordered form fields and signed assertion, strict
usable-lease equality/one-millisecond boundaries, redirect/document/transport
refusals without retry, clock refusal before I/O, redaction and cancellation
through the real assertion-source/cache combination. These tests use an injected
pending transport, not a concrete network client. The production async network
transport and provider/route migration still remain required within task 7501.
Verification on the final encoder/source changes: 96 connection unit tests,
12 cache tests, 24 provider tests and 16 identity-management exchange tests passed.
Daemon test-target compilation and whitespace checks passed.

The identity-management route now has a direct TLS connector whose public
constructor accepts only IdentityManagementTrustInput. It freezes platform
roots and an explicit crypto provider/TLS 1.2–1.3 policy, dials the manifest IMS
host and port with a bounded connect phase, and authenticates with an independent
handshake deadline. ALPN selects HTTP/1.1 or HTTP/2 on the original socket; no
probe/reconnect, author-address override, proxy input or ambient root lookup is
present. No HTTP request bytes are sent by connection establishment.

Real local TLS tests cover both TLS versions, absent/HTTP1/HTTP2 ALPN, trusted
versus unrelated roots, hostname mismatch including the fixed IMS hostname,
redacted handles, pending-handshake cancellation and timeout socket closure.
The test-only handshake seam does not expose a production endpoint override.

Review corrected the preceding async source's lifetime observation boundary:
sampling before invoking a connector would include DNS/TLS setup in token age.
The async transport now receives the injected monotonic clock and returns a
redacted receipt naming the immediate pre-request and complete-body observations.
The source installs from those observations. A test advances the clock during
simulated setup and proves the request anchor, not the earlier source entry,
determines expiry. The preceding source-entry-anchor note is superseded.

Verification: 98 connection unit tests, 12 cache tests and 16 exchange tests
passed. The concrete IMS HTTP request/response codecs and whole-exchange deadline
composition have not yet been installed behind this connector; provider/route
migration and complete adapter conformance remain open in task 7501.

The IMS HTTP/1.1 codec now owns an authenticated stream for exactly one fixed
manifest POST. It writes the bounded form without copying it into the request
head, records the request anchor immediately before writing, and applies
separate write/head/body-idle/body-total deadlines. Header lines and decoded
field/count/aggregate charges are bounded before collection admission; ordinary
HTTP optional whitespace is removed before charging decoded values. Framing
rejects invalid/conflicting lengths, length-plus-transfer-coding, invalid coding
chains, malformed chunks, truncated bodies and surplus bytes. Fixed-length and
close-delimited success require complete EOF before a receipt is produced.

The first informational/final rejection ends the exchange without another
request. IMS media validation is shared with the original exchange validator.
Declared trailers and actual trailer sections are refused, including the empty
section following the last chunk; the codec does not erase that presence into a
trailer-free success. Trailer fields use the same incremental section bounds.
Partial response storage and the completed async receipt scrub their body buffers
on drop. No partial response escapes cancellation or malformed framing.

Seven byte-transcript/virtual-time tests cover exact POST fields and request
count, complete fixed/close framing, informational/redirect/media/trailer
checkpoints, head/body exact and next-byte limits, malformed/truncated/surplus
wire data, distinct deadline phases, expiry equality with an already-ready byte,
and cancellation closing a partial-response socket without retry. The broader
run passed 105 connection unit tests, 12 cache tests and 16 exchange tests; the
final whitespace adjustment passed all seven focused tests. Daemon test-target
compilation and whitespace checks passed.

This codec is not yet the composed IMS client: HTTP/2 response handling,
whole-exchange deadline composition around connection and codecs, the runtime
authentication migration and concrete conformance audit remain required. Task
7501 stays planned/incomplete and no subsequent task has started.

HPACK decoding now accepts a route-specific incremental decoded-header sink.
The default remains the existing author gate, preserving its accounting and
status policy. The IMS sink instead applies the canonical fixed five-byte
initial section / one-byte trailer section charge and three-byte ordinary-field
delimiter charge. The status pseudo-field is syntax-checked but is not an
ordinary field/count charge. Ordered duplicates remain distinct. Informational
and redirect statuses survive decoding for the later IMS status checkpoint;
trailer/Alt-Svc fields likewise reach route policy instead of being silently
filtered by the author gate. Connection-specific fields and malformed pseudo
ordering remain transport refusals.

The same bounded HPACK parser handles raw/Huffman strings, dynamic indexing and
arbitrary fragments for both routes. Its compressed-limit refusal is separately
observable from malformed syntax and decoded-limit refusal, without remote text.
A reviewed follow-up retains the original connection's compression table when
handing a completed head to a new bounded trailer-section sink; otherwise a valid
dynamic trailer reference could evade the required accounting checkpoint by
being misclassified as an unknown index. This permits charging, not accepting,
the forbidden trailer section.

Seven focused tests cover canonical exact/next charges, exclusion of :status from
ordinary bounds, empty trailers, informational/redirect visibility, duplicated
dynamic fields, fragmented Huffman decoding, compressed-versus-decoded failures,
poisoning, forbidden fields, redaction and cross-section table continuity.
The final unit run passed all 112 connection tests; the broader regression run
also passed 24 provider tests, 12 cache tests and 16 exchange tests. IMS HTTP/2
response assembly/network dispatch, whole-exchange composition, runtime migration
and complete conformance remain implementation work in task 7501.

IMS HTTP/2 response assembly now consumes the bounded wire frames and IMS HPACK
sink, preserves the connection table for trailer accounting, verifies status,
media and content length, and bounds unpadded DATA before body admission.
Receive credit accounts for the full padded frame and is returned only after
successful consumption. The first failure poisons the assembly with its stable
code; no later frame can replace that failure or expose a partial response.
Body storage is scrubbed on drop. Completion consumes the wire reader's clean
transport EOF proof as well as a complete END_STREAM response.

The shared frame reader has an opt-in route policy for one bounded trailer
section ending the stream and for an IMS-specific compressed-section limit.
Its default author mode still rejects a second HEADERS section. This permission
only lets IMS account for and reject trailers, including empty or dynamically
indexed sections. An encoded-limit flag preserves refusal classification before
an over-bound payload allocation.

Seven wire/assembly tests cover every split of a valid header block, padded
content and flow credit, EOF versus partial/extra frames, short/long/conflicting
lengths, coherent duplicate lengths, body exact/next-byte bounds, informational
and redirect statuses, media/declaration/trailer refusals, default author policy,
poisoning and redaction. Oversized initial/trailer sections are classified before
status/trailer refusal, including continued trailer blocks with retained dynamic
references. All 119 connection unit tests, 24 provider integration tests,
12 cache tests and 16 exchange tests passed.

The asynchronous IMS HTTP/2 writer/control/deadline driver and top-level
connection/codec/overall-deadline composition still remain to be implemented,
followed by runtime authentication migration and complete conformance review.
This assembly is not a completed or installed runtime client; task 7501 stays open.

The IMS HTTP/2 driver now owns joined reader/writer futures, reuses the bounded
SETTINGS/frame/window parsers with the IMS frame policy, emits one fixed POST
header block and exact form DATA, and processes control/receive-credit messages
through a bounded channel. Request starvation keeps reading window updates;
completed early responses reset an unfinished request rather than retrying it.
The driver closes its single-use connection and requires the assembler's clean
EOF proof. No network task is detached.

IMS-specific request-write, response-head, body-idle and body-total failures are
retained through the driver. Raw-byte idle progress cannot extend the absolute
body deadline, and pending credit/control waits remain bounded. The request
anchor is sampled by the writer immediately before its first application bytes;
the reader samples receipt at complete EOF. A concrete IdentityManagementClient
now combines the platform-only fixed-endpoint TLS connector with HTTP1/HTTP2
dispatch on that same socket and a whole-exchange deadline including connection
setup. Overall equality expires and drops the owned operation.

Review found an intermediate-buffer lifetime gap while joined I/O finishes.
Decoded IMS responses now scrub their bodies on drop and render redacted;
completed HTTP/2 frames and cancelled/failed partial frame payloads also scrub
their buffers. The receipt no longer needs a duplicate body-drop wrapper.
Existing test-only payload moves were adjusted for the owning frame destructor.

Six driver transcripts prove exact POST/body delivery through partial window
credit, early-response no-retry behavior, handshake/write/head/body deadlines,
total expiry despite control traffic, stable status failures and cancellation
of both I/O halves. Two overall-deadline tests prove pending-operation cleanup,
equality refusal and preservation of an earlier phase failure. Final verification
passed 127 unit tests, 24 provider tests, 12 cache tests, 16 exchange tests and
daemon test-target compilation; the added decoded-response redaction assertion
also passed independently.

The runtime EnvironmentAuthenticationProvider and selected-author route policy
still use the synchronous cache/source API. Migrating that single authority to
the new async cache/client, then completing composed concrete conformance and
startup integration, remains required. This does not close task 7501 or plan 20.

EnvironmentAuthenticationProvider now shares its immutable snapshot and exact
target/revision/transport checks across cache strategies. The async strategy owns
one AsyncCloudAccessTokenCache and, for Cloud only, one IdentityManagementClient
constructed from that snapshot's platform-only roots. Runtime authentication
builds its token source from the owned selected credentials/client and supplied
runtime clocks; Basic creates no IMS client and samples neither token clock.
Explicit trusted-source test/adapter entry points share this same cache rather
than installing another authority. Legacy synchronous callers remain available
while route migration proceeds.

The finite-GET adapter has an async-provider entry point. Both provider modes
use the same request/retry/elapsed-receipt implementation: only a complete Cloud
401 refreshes once, Basic never retries, and provider target/revision refusal
precedes authentication/network use. This path awaits the new provider's owned
exchange and uses its own selected credentials for refresh.

Four focused tests prove exact Basic bytes without token-clock access, runtime
and injected-source cache sharing, stale/foreign lease protection, no fallback
after a runtime assertion refusal, shared failure/cancellation across eight
waiters, and real-loopback async GET behavior including Basic 401/no second
socket and foreign-provider pre-I/O refusal. The loopback fixture was corrected
to omit the incompatible off-loopback insecure-transport opt-in; no validation
rule was weakened. All 127 unit tests, 28 provider tests, 12 cache tests and
16 exchange tests passed.

The daemon invocation policy and command-specific capability/submission/lookup/
artifact/reset/event adapters still require migration to the async provider.
No second cache has been added to an existing provider, but the daemon has not
yet switched provider strategies. Task 7501 and the full goal remain incomplete.

### Async command-specific finite reads

Capabilities, logical lookup, physical lookup/snapshot, and subscription
high-water capture now expose async-provider entry points. Each retains the
existing preflight and response decoder and uses the shared async finite-GET
adapter; no separate retry or response-validation policy was introduced.
Physical absence remains bound to the queried identifier and current generation,
and snapshot-only reads continue to reject absence.

The existing route matrices now exercise async Basic authentication: capability
generation/readiness/location checks, physical lookup response and preflight
refusals, logical missing/retired responses, and high-water success/reset/refusal
over negotiated TLS 1.3 HTTP/2. Panic-on-use token clocks verify that these Basic
paths do not request token-clock or assertion work. The high-water matrix is now
seven transport/provider modes. All 127 connection unit tests, 28 provider tests,
12 cache tests, and 16 exchange tests passed; daemon test compilation and
`git diff --check` also passed. The initial sandboxed socket test was denied by
the environment; rerunning with scoped local-socket permission passed.

Review confirmed that selection checks precede authentication, the shared
decoders remain authoritative, and these additions do not install a second
runtime cache. Cloud refresh is covered by the existing shared-provider/cache
tests, not claimed as a new composed IMS-to-command network proof here.
Submission, artifact/event adapters, daemon invocation policy, startup assembly,
and the remaining concrete conformance audit still need implementation. These
are actionable repository work, not a requirement for additional user input.
Task 7501 remains incomplete and the goal remains active.

### Async guarded submission

The selected transport now exposes guarded async-provider submission. It awaits
the shared authenticated CSRF GET, validates the token, obtains request-scoped
POST authentication, then runs the durable guard and rechecks submission binding.
The POST timestamp includes elapsed pre-send work. The existing single-exchange
implementation now accepts an awaited unauthorized callback, allowing Cloud
refresh to complete without duplicating the POST or changing its unknown outcome
into a pre-send refusal. Fixed and synchronous provider paths use that same
implementation with immediately completing callbacks.

The TLS submission matrix additionally exercises async Basic requests and cached
Cloud requests whose runtime refresh fails at the assertion clock. It checks
exact request bytes, guard counts, no extra socket, invalid-CSRF refusal, Basic
401/403 behavior, and Cloud cache invalidation only after POST 401. Cloud refresh
failure still returns submission uncertainty; 403 and pre-send refusals preserve
the cached token. The async fixture's distinct token bytes required correcting
the peer assertion; production validation was not changed to accommodate it.
Successful concrete IMS refresh during submission and cancellation while that
refresh is pending remain part of the outstanding composed conformance work.

Review checked the shared POST path, callback timing, provider cache ownership,
and pre-send guard ordering. All 127 connection unit tests, 28 provider tests,
12 cache tests, and 16 exchange tests passed after the fixture correction.
Daemon test compilation and diff whitespace checks passed. Artifact/event
adapters and daemon runtime installation still require async migration; task
7501 and the full goal remain active and incomplete.

### Async artifact and event adapters

Artifact downloads and event attachments now expose async-provider entry points.
They retain the existing selected-request preflight and negotiated streaming
drivers, use the provider's sole cache, and await at most one replacement after
the drivers report a complete pre-stream 401. No sink/consumer error or partial
wire response enters the refresh branch. Artifact elapsed receipts still include
authentication and refresh; event retries retain the exact committed cursor.

The Basic artifact matrix now includes six modes and the event matrix seven,
including async-provider artifact success/unavailability/digest/framing/sink
failures and negotiated TLS HTTP/2 event/reset validation. Token clocks panic
if used on those Basic paths. The cached-Cloud TLS matrix additionally checks
artifact and event refresh refusal, truncated 401s, partial artifact bodies, and
partial event delivery. Complete 401s invalidate the cache and fail closed when
the assertion clock is unavailable; partial responses preserve the cached token.
The peer observes no additional connection, and consumers retain their existing
delivery semantics. Successful composed IMS refresh and cancellation during
refresh are still outstanding conformance cases, not implied by these fixtures.

Review verified preflight ordering, provider binding, credential drops before
refresh, and terminal streaming failures. The 127 unit, 28 provider, 12 cache,
and 16 exchange tests passed with the Basic matrix extensions; the subsequently
expanded cached-Cloud integration matrix also passed. Daemon test compilation
and diff whitespace checks passed. Command-specific async transport entry points
are now present, but the daemon invocation policy and startup installation still
need migration. Task 7501 remains incomplete and the goal remains active.

### Daemon async invocation policy

`AuthorAuthentication::AsyncProvider` now carries the frozen async provider and
its borrowed monotonic/UTC clocks through every daemon author operation:
discovery, guarded submission, logical/physical lookup, high-water capture,
artifact download, and event attachment. Sized borrowing adapters forward clock
calls without sampling early or creating another clock/cache. The three legacy
compatibility handoffs for reset, physical probing, and terminal recovery retain
a saved async policy rather than replacing it with supplied fixed credentials.

The durable event and subscription-reset matrices now include a fifth async
provider mode, covering owned/direct attachment and saved recovery paths with
panic-on-use Basic token clocks. Async initial-admission cases cover acceptance,
lost acknowledgement, stale pre-POST authority, and restart without credential
acquisition or resubmission. A new retained-completion test drives both package
and loaded-JSON artifact publication through the async policy.

Review checked all seven dispatches, saved-policy preservation, shared provider
ownership, and unchanged durable guards. The existing 11-test durable-author
suite passed with the expanded event/reset matrices (218.61 seconds); the
subsequently extended admission test and new async retained-publication test
also passed. All 15 executor and seven existing conformance tests passed; those
existing conformance tests do not establish full concrete network conformance.
Daemon test compilation and diff whitespace checks passed. Startup ownership
and readiness installation remain task 7502 work after task 7501's outstanding
composed network/conformance audit. Task 7501 and the full goal remain incomplete.

### HPACK declared-length conformance follow-up

The IMS network-path review found that HPACK literal lengths exceeding the
encoded head budget were refused but lost their limit reason, becoming generic
transport failures. The integer/string/block layers now preserve the bounded
length refusal for the IMS assembler's existing head-limit mapping. Literal
length continuation bytes also reduce the enclosing budget: an impossible
remaining payload is refused immediately when its length becomes known, before
payload storage. Malformed Huffman syntax remains a distinct transport refusal.

New tests cover raw/Huffman name and value declarations, single/multiple length
octets, every input split, poisoned-state stability, exact-boundary valid values,
and invalid Huffman padding. A wire-level assembler test verifies that tiny
HEADERS/CONTINUATION frames declaring oversized literals produce the stable
IMS head-limit code without supplying those payloads. Review confirmed that the
shared author decoder remains bounded and existing valid inputs are retained.
All 129 connection unit tests, 28 provider tests, 12 cache tests, and 16 exchange
tests passed, as did daemon test compilation and diff whitespace checks.

This closes a concrete decoder gap, not the full task: composed fixed-host IMS
TLS/source/provider refresh and cancellation proof, the remaining Plan 0005
network conformance audit, and task 7501's completion gate remain outstanding.
The full goal remains active.

### Composed IMS TLS and HTTP proof

The concrete IMS client now shares a private connection-future composition
function with wire tests. Production supplies only its unchanged fixed-host
connector; a `cfg(test)` socket seam replaces physical TCP dialing while still
deriving TLS identity from the manifest. No production address, root, proxy, or
hostname override was added. Body preflight still precedes polling the connector
and the overall deadline still owns connection establishment and codec work.

New real-loopback tests validate TLS 1.2/1.3 with no ALPN, HTTP/1.1, and HTTP/2,
exact manifest SNI/HTTP authority/path, one form request and no second socket,
and request/receipt clock sampling. Separate trust and hostname failures stop
before HTTP/clock use. Oversized forms do not poll the connection. Cancellation
after a TLS HTTP/1.1 request drops the socket without a response receipt or retry.

A documented test-only CA/leaf chain names the exact IMS hostname and reuses
the already public fixture key. The initial self-signed leaf was correctly
rejected as a platform root; the fixture was corrected to a CA/leaf chain rather
than weakening trust validation. Review confirmed that production uses the same
TLS handshake and codec dispatch exercised here. All 133 connection unit tests,
28 provider tests, 12 cache tests, and 16 exchange tests passed, along with daemon
test compilation and diff whitespace checks.

These tests replace TCP resolution/dialing and use a fixed fixture form: they do
not yet prove a signed source/provider refresh through the composed network
path, nor cancellation during that composed refresh. Those and the broader
Plan 0005 network audit remain outstanding. Task 7501 and the goal are incomplete.

### Signed source/cache over composed IMS TLS

The real signed token source and asynchronous cache are now exercised through
the private composed-client TCP seam, preserving fixed IMS TLS identity and
the production HTTP/1.1 and HTTP/2 codecs. The peer independently checks ordered
form fields, credential fixture values, the first assertion's committed golden
vector, and the freshly sampled assertion for each subsequent exchange.
Successful refresh installs one replacement generation; stale and foreign
leases cause no additional network exchange.

A second test cancels a refresh after the signed request reaches the TLS peer.
A deterministically polled cache joiner receives cancellation, the peer observes
socket closure, and a later caller performs a new signed exchange instead of
receiving the rejected old token. Both HTTP protocols pass, including exact
exchange and assertion-sample counts. All 135 connection unit tests, 28 provider
tests, 12 cache tests, and 16 exchange tests passed; daemon test compilation and
diff whitespace checks also passed.

This proves signed-source/cache network composition, not normal provider-owned
client installation or production DNS resolution. Review also reconfirmed the
explicit Plan 0002 requirement for a process-random opaque cache identity: the
current async allocation-owned identity protects lease ownership but does not
meet that literal requirement. Its identity factory and initialization failure
handling need completion before closing task 7501. The broader conformance audit
and the full goal remain active and incomplete.

### Process-random async cache identity

Normal async provider construction now creates a 256-bit opaque
`AccessTokenCacheIdentity` with the transport crypto provider's secure random
source. The identity has no display or serialization and redacted debug output;
it remains unrelated to credentials. Allocation ownership is retained as an
additional lease boundary, so accidentally repeated injected identity bytes
still cannot make separately constructed caches accept each other's leases.

Cache construction is fallible and has no infallible `Default` or fallback
identity. Entropy failure maps to the existing coarse
`identity_management_transport_failed` initialization refusal, discarding the
dependency error and returning no provider. Explicit identity factories support
deterministic tests, while ordinary `new_async` selects secure randomness.
Legacy synchronous fixtures retain their explicitly supplied integer identity,
encoded into the opaque representation; they are not the runtime async path.

Review verified factory ownership, initialization failure propagation and all
cache callers. Tests cover random identity distinction, redacted debug output,
single factory invocation on failure, no provider on failed initialization, and
foreign-lease rejection even for identical deterministic identity bytes. Async
concurrency tests now use the injected factory. All 137 connection unit tests,
29 provider tests, 12 cache tests, and 16 exchange tests passed; daemon test
compilation and diff whitespace checks passed. This closes the audited async
identity requirement, not the remaining provider/network conformance audit.
Task 7501 and the full goal remain incomplete and active.

### Exact connection-phase deadline boundaries

The Plan 0005 phase audit confirms that its precise deadline definition gives
DNS resolution and TCP one shared connect budget; TLS begins afterward with an
independent budget. No separate DNS budget was invented from the broader
conformance shorthand. Author and IMS connectors now use one private bounded
phase helper that explicitly refuses a ready result at or beyond the deadline,
including dropping a late returned socket. This closes the edge where a ready
future can win Tokio's timeout polling order without a post-result clock check.

Deterministic virtual-time tests prove before/equal/after boundaries, destruction
of owned results/work, original early failure preservation, and an independent
next-phase budget. Existing real-socket author and composed IMS TLS tests remain
green. Review verified unchanged selected hosts, roots, ALPN, direct dialing,
phase-specific error mapping, and the outer IMS overall deadline. All 139
connection unit tests, 29 provider tests, 12 cache tests, and 16 exchange tests
passed; daemon test compilation and diff whitespace checks passed.

This closes the connector deadline edge only. Provider-owned network composition,
the outer development conformance/trap transcript, and the rest of task 7501's
complete conformance gate still require work. The goal remains active.

### Provider-owned IMS network composition

Normal provider authentication/refresh is now tested with its own selected
credentials, random-identity cache, and concrete IMS client. A compile-time-only
constructor/socket field replaces physical TCP dialing for these unit tests;
it does not exist in production builds. TLS identity still comes from the IMS
manifest and both protocols use the production codecs. No request-time source
injection is used by the new tests.

HTTP/1.1 and HTTP/2 cases prove signed first exchange, cached reuse, one signed
replacement, stale/foreign lease protection, and no additional socket. Publisher
target refusal precedes clock/assertion access. Cancellation of provider-owned
refresh after signed request delivery closes TLS, fails a joined authentication
waiter, and requires a new exchange rather than serving the rejected token.
A CA present only in the selected author extension cannot authenticate the
provider-owned IMS client; the peer receives no HTTP bytes or token response.

The fixture profile, credential and author-CA documents pass the real complete
integrity inventory. Review corrected the initial omitted credential inventory
entry and made the additional CA/source revision explicit instead of weakening
configuration validation. All 142 connection unit tests, 29 provider tests,
12 cache tests and 16 exchange tests passed, as did daemon test compilation and
diff whitespace checks; the final three provider cases passed again after the
fixture source-reference tightening.

This closes the focused provider-owned network proof. It is not the outer
development transcript with all simultaneously reserved author/publisher/proxy/
redirect traps, production DNS observation, or the complete Plan 0005 command
conformance audit. Those task 7501 requirements and the full goal remain open.

### Outer concrete Basic boundary transcript

The development crate now drives real profile loading, immutable snapshot
construction, the async provider, and concrete selected-author capability
discovery. Live author/publisher/proxy/redirect/IMS/hostile-IMS listeners remain
reserved together. Success, redirect, Basic 401, and truncated-response cases
verify the exact context-prefixed route and authorization bytes, selection and
publisher refusal before I/O, no extra author request, and no trap connection.

Each case also runs in an isolated child process with uppercase/lowercase HTTP,
HTTPS and ALL proxy variables pointing to the proxy trap and both NO_PROXY
variables empty. This does not mutate the parent's environment. The child uses
the same concrete provider path, its lifetime is cancellation-owned, and its
stdout/stderr join direct debug/result renderings in the secret scanner. Raw
Basic username, password and encoded authorization value remain absent from
those diagnostic transcripts; authorization is intentionally checked on the
allowed author wire rather than treated as a diagnostic leak.

Review confirmed no simulated AuthorPorts/token-source path, real trap accepts
instead of closed-port assertions, and a dev-only Tokio dependency with only
the corresponding lockfile edge added. The new transcript (four cases in each
process mode), six existing network-chaos tests, and four existing profile
boundary tests passed. The concrete transcript passed again with raw-principal
scanning enabled; diff whitespace checks passed.

This is the Basic/discovery portion of the outer proof, not the full gate.
Cloud/TLS composition, hostile-CA author success beside IMS refusal, command
execution/recovery transcripts and production-startup conformance remain open.
Task 7501 and the full goal remain active and incomplete.

### Outer owned-Cloud TLS boundary and conformance access

The existing provider-owned IMS loopback seam is now available to cross-crate
conformance tests through an explicit `test-support` Cargo feature, enabled only
by the development crate's dev-dependency. The public test constructor rejects
non-loopback and unspecified IPv4/IPv6 addresses before provider construction.
Normal constructors still supply no endpoint override. The seam retains fixed
IMS SNI/hostname verification, platform-only trust, protocol codecs, and bounded
exchange ownership. Review added the normal connection-phase deadline to its
loopback dial as well; the seam does not exercise production DNS resolution.

The outer development test now composes loaded Cloud profiles, immutable
snapshots, owned service credentials/cache/client, actual IMS TLS, and concrete
selected-author TLS capability discovery. Four cases cover success, complete
401 with exactly one token refresh and one identical read retry, redirect
refusal, and truncated-body refusal. The peer checks fixed IMS SNI, authority,
route, ordered form fields, complete signed assertion, context-prefixed author
route, and the expected bearer generation. Publisher/proxy/redirect/hostile-IMS
traps stay reserved and reject any unexpected connection; diagnostics are
scanned for tokens, client secret, raw principal components, and private-key
markers. These Cloud cases currently run directly with HTTP/1.1; the previously
recorded isolated ambient-proxy child matrix remains Basic-only.

A second valid loopback author leaf signed by the IMS fixture CA proves author
TLS succeeds with that CA installed only as an author extension, while the same
provider rejects the correctly named IMS leaf before sending HTTP credentials.
This uses real selected-author connection establishment, not certificate-set
membership as a substitute for TLS. The fixture uses only the existing public
test key and is documented as unsuitable for deployment.

Validation: normal no-default-feature library check passed; all 142 connection
unit tests passed; all five outer concrete boundary tests, six network-chaos
tests and four profile-boundary tests passed after review fixes. Whitespace
checks passed. No new permission or user choice is needed to continue. Cloud
ambient-proxy child coverage, the complete command/recovery conformance gate,
and subsequent startup composition remain unfinished; task 7501 and the full
goal remain active rather than being declared blocked or complete.

### Cloud ambient-proxy process isolation

The outer Cloud matrix now runs each case directly and in an isolated child
process. The child constructs its own loaded snapshot, owned provider/cache/IMS
client and selected-author transport. All six uppercase/lowercase HTTP, HTTPS
and ALL proxy variables point to the reserved proxy trap; both NO_PROXY forms
are empty. Only the existing compile-time loopback IMS seam changes dialing;
TLS identity and codecs remain unchanged. The parent still validates every
signed IMS exchange and author bearer generation and observes all trap sockets.
Child stdout/stderr enter the same secret scanner as direct diagnostics.

Review tightened the child outcome check to exactly one marker, kept child
lifetime cancellation-owned, and added complete 403 and truncated 401 cases.
Neither may refresh credentials or retry the author request. Six Cloud cases
in each process mode passed, alongside the Basic matrix, hostile-CA proof and
non-loopback refusal test. All six concrete-boundary tests, six network-chaos
tests and four profile-boundary tests passed; whitespace checks passed.

This closes the previously noted Cloud ambient-proxy child gap. Full command
and recovery conformance and subsequent startup composition remain open; no
task completion or broader goal completion is claimed.

### Existing provenance vectors through durable concrete admission

The daemon's selected-admission test previously exercised generation refusal
over a real socket while the seven shared provenance-drift vectors were proved
only against decoded capability objects. It now consumes those exact JSONL
vectors through the concrete discovery codec and durable initial-submission
coordinator, with both fixed authentication and the owned async provider.
Generation change and continuation-authority readiness refusal complete an
18-case matrix. Each case observes the context-prefixed capability request,
requires refusal, confirms no retained agent-operation record, and checks that
no second connection (CSRF or submission) follows. Existing successful/lost-ack,
stale-guard and restart checks continue after this matrix.

Review separated the immutable successful capability fixture from each mutated
copy, fixing a compile-time scope error caused by the new loop. The expanded
admission test passed, as did all seven existing simulated conformance tests
and 15 executor tests. This proves the existing provenance fixture family over
the concrete durable path; it does not relabel the other simulated fixture
families as concrete or claim the full task gate has passed.

### Event conformance review: exact replay versus changed state

Review of the shared event vectors found that the case named as an exact replay
actually retained a queued observation while receiving a progress event, which
describes running. Its integrity-conflict expectation was correct, but its name
misstated the evidence. The vector now names the changed-state conflict, and a
separate running-state vector explicitly expects exact replay. The conformance
driver accepts the explicit retained fixture state and refuses unknown state
spellings. All seven conformance tests passed with both cases present.

The concrete live-event matrix already distinguishes replay, changed progress,
unknown operation, stale sequence, sequence gap and conflicting cursors across
five transport/authentication modes and both owned/direct attachment paths. It
checks durable sequence, counters, retention, physical association and cursor
state; this review did not replace that socket-backed evidence with the pure
fixture test. All 12 selected-admission tests passed together in 238.14 seconds,
covering the accumulated admission, event, reset, artifact and recovery work.
Whitespace checks passed. This is a combined regression result, not evidence
that every remaining task 7501 conformance requirement or later plan is done.

### Authentication ownership checked at invocation construction

Runtime composition review found that `RetainedAuthorProtocol` validated local
command/database ownership while accepting a foreign authentication policy
until a later network preflight. It now requires the policy's immutable target
and revision to match the invocation before retaining any runtime context.
Fixed credentials expose only a credential-free binding check; synchronous
and asynchronous providers use their frozen selected connection identity.
No token acquisition, clock sampling, cache mutation or network I/O occurs in
this check. Per-request binding checks remain unchanged as a second boundary.

The concrete admission test covers foreign targets and revision-only drift
for all three policy variants. The revision-only case changes publisher
metadata while preserving the exact author target. All six foreign policies
refuse construction, preserve the retained agent-operation state, and open no
author connection. Matching policies still pass the existing admission and
restart matrix and async retained-artifact publication tests.

Review and validation: the expanded admission test passed; async retained
publication passed; all 29 environment-provider, 12 token-cache and 16 IMS
exchange integration tests passed; seven conformance and 15 executor tests
passed. Whitespace checks passed. This closes an eager runtime-binding gap,
not the entire task 7501 conformance gate or subsequent startup composition.

### Recovery entry points retain only selected authentication

Follow-up review extended eager authentication binding to durable submission,
lookup, event attachment, subscription reset, physical generation probing,
scheduled generation recovery and terminal-event capture. Each coordinator
now rejects a target/revision mismatch before returning retained evidence or
capturing a policy for later work, rather than relying solely on eventual wire
preflight. Terminal reconciliation also checks a compatibility caller's fixed
credential before its wait and failure accounting, so a local configuration
mismatch cannot consume an exchange-failure recovery budget.

Concrete regressions cover all three policy forms with target and revision-only
drift against an already retained integrity incident and a valid generation
reset. They require refusal while preserving subscription/local state. Another
case captures a real terminal event and supplies a fixed credential from the
same author under a different revision: refusal must preserve local, remote
and cursor records and open no lookup connection. Matching policies retain the
existing path through the full event and recovery matrices.

Validation: all 12 concrete selected-admission tests passed together in 228.13
seconds. After the terminal-reconciliation follow-up guard and its regression
case were added, the expanded live-event matrix passed again in 216.10 seconds.
All seven conformance and 15 executor tests passed; whitespace checks passed.
No recovery budget or retained evidence is now reachable through these entry
points with a mismatched policy. Broader task 7501 conformance and subsequent
runtime startup work remain open; the goal remains active.

### Concurrent Basic and Cloud isolation in the outer transcript

The outer concrete test now runs independently selected Basic and Cloud
providers together, each with its own context-prefixed author and publisher
metadata. Both publishers, proxy, redirect and hostile-IMS traps stay reserved.
The real fixed-identity IMS TLS peer receives and validates the signed form,
then withholds its token response until the Basic capability request has fully
completed. This deterministic handshake proves pending Cloud authentication
does not serialize Basic work; it does not infer concurrency from test-runner
scheduling or sleeps. Two Basic and two Cloud requests finish, with exactly one
IMS exchange and no additional author or trap connections.

The peers verify exact Basic/bearer authorization and reject the other scheme.
Both result/debug transcripts are scanned for credentials and raw principal
components. Review added explicit swapped-provider transport calls before the
concurrent work; they must reject without token clock sampling or network I/O,
as must authentication for the other provider's author endpoint.

All seven outer concrete tests, six network-chaos tests and four profile-boundary
tests passed after review; whitespace checks passed. This closes the concurrent
Basic/Cloud outer proof gap, not the remaining task 7501 command conformance or
later runtime composition requirements. The full goal remains active.

### Secret rotation preserves frozen providers and separates cache ownership

The outer Cloud snapshot helper now accepts explicit credential document bytes,
includes those exact bytes in the verified source inventory, and parses the
same document into the owned provider. A new test constructs one provider,
rotates only the client secret at the same source reference, then constructs
another provider. Target and semantic revision remain equal, as required for
secret-only rotation, while cache ownership remains distinct.

Real IMS TLS exchanges observe original, rotated, then original credentials:
the existing provider never reloads the rotated input, even on forced refresh.
The selected-author TLS peer observes five requests with token generations
old/new/old/refreshed-old/new. The rotated provider rejects a foreign old-provider
lease without exchanging, and refreshing the old provider neither evicts nor
replaces the rotated provider's cached token. All publisher/proxy/redirect/
hostile-IMS traps remain untouched; diagnostics pass secret/principal scanning.

Review added the final new-provider request to prove its cache survives the
old-provider refresh. All eight outer concrete tests, six network-chaos tests
and four profile-boundary tests passed after that addition; whitespace checks
passed. This verifies immutable in-process snapshot/provider reconstruction,
not a production daemon process-restart or startup-builder proof. Those and
the remaining task 7501 conformance requirements are still open.

### Task boundary audit and concrete physical-job acknowledgement fixtures

The completion audit separates task 7501 adapter conformance from task 7502's
durable runtime builder/readiness and task 7601's compiled-process-only proof.
Both later tasks explicitly depend on 7501; their production-startup and binary
restart deliverables must not be treated as prerequisites that prevent 7501
from ever closing. Their requirements remain mandatory for the overall goal.

One concrete adapter gap was identified: the shared physical-job set vectors
were covered by the pure submission interpreter, while the socket submission
test used one valid identifier. That real CSRF/POST test now consumes all eight
shared job-set fixtures through fixed credentials and the owned async provider.
Empty, duplicated, unsorted and empty-element acknowledgements remain unknown
after the POST; valid sorted/distinct sets are accepted. Existing media and
identity-refusal cases remain intact. Every variant checks for no extra request.
The expanded socket test passed, preserving the existing later HTTP/2 and TLS
phases of the same test rather than replacing them with the new HTTP/1 matrix.

The full connection-suite audit also found stale heartbeat fixtures: accepted
and progress events lacked physical-job/state fields and used a noncanonical
operation identifier. The fixtures now use the required wire fields and state
corresponding to their event kind; the strict production decoder is unchanged.
All eight heartbeat tests passed after the correction.

The complete `cargo test -p slingshot-agent-connection --offline --quiet` rerun
passed after both changes, including the full terminal-failure command-policy
matrix (263.10 seconds for that test binary) and all remaining integration/doc
tests. Whitespace checks passed. The initially failing full run is not counted
as success; only the corrected complete rerun establishes this result. Adapter
completion review remains open, with startup/compiled-process work assigned to
its actual dependent tasks rather than used as a circular prerequisite.

### Adapter completion review

Task 7501 is complete following the requirement-to-implementation and concrete
test mapping in [the adapter completion review](../author-port-conformance.md).
The previous open-status notes describe intermediate checkpoints, not the
final disposition. All four task steps and the existing agent conformance
families have concrete adapter evidence; identified follow-up issues were
fixed and verified before closing this task.

Production runtime construction/readiness and compiled-process-only proof
remain assigned to dependent tasks 7502 and 7601 respectively. This completion
does not mark those tasks, Plan 0020, or the overall plans 15–20 goal complete.
