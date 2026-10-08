# Documentation review

Review renewed on 2026-09-29 for `CONTRIBUTING.md`, `docs/DAEMON.md`, and
`docs/MODEL_CONTEXT_PROTOCOL.md`, with a further review of `docs/COMMANDS.md` for
the incremental discovery migration and renewed on 2026-10-02 for its page-search
contracts. The unchanged document identities below retain
their previous reviews; this update makes no fresh workspace-wide review claim.

The source-policy checker enforces structural documentation rules and rejects
new findings against the reviewed baseline. It does not decide whether prose
is accurate, complete, or useful. Those remain a reviewer's judgement. The four subjects below are the inventory in
[documentation rules](../policy/documentation-rules.toml).

On 2026-10-06, reviewed capability timing observations against the fixed numeric parser,
finite-response admission boundary, saturating atomic counters and independent submission
observer. The protocol document distinguishes absent, rejected and parsed headers, exact byte
and duration limits, quiescent attribution and unsupported causal or behavioral claims.
The preserved grammar boundary cases exercise both sides of every numeric limit and prevent
remote descriptions or header text from entering observations. This source review does not
claim a complete gate, deployment, compatibility or live latency repair.

On 2026-10-06, renewed the complete protocol document review against the finite
HTTP/1 head-only preflight and final body/connection-close collector, the HTTP/2
END_STREAM collector, and both unchanged numeric observers. Real loopback endpoint
assertions cover complete, absent, unusable, truncated, surplus and malformed
responses in a dedicated process. The prose states the observation boundary
without changing response acceptance or claiming remote repair or full-gate proof.
The same review now covers HTTP/1 artifact head-only validation against the HTTP/2
artifact head boundary. Original staging, integrity, refusal, cancellation and
deadline assertions remain; the artifact workflow publishes neither finite-response
observer. An artifact head is represented by the artifact head type rather than
an empty completed finite response. No command, body or retry acceptance is widened.

## Public contract and failure coverage

*Every contract, invariant, side effect, and bound that applies is stated.*

Reviewed the changed interfaces and their prose together. MCP documents bounded
workers, response ownership, identifier reuse, cancellation, pressure, output
failure and EOF. Catalogue metadata exposes the compiled identities in both
revisions and distinguishes shipped contracts from a deployment’s active handlers. The daemon document describes automatic scheduling and retry
reconstruction, durable producer rotation, admission limits, and the version-2
producer identity boundary. The contributing guide
names the full gate and the narrower developer checks, offline dependency
behavior, timing output, and the baseline exception mechanism. Platform and
release evidence remain separate from these implementation descriptions.

Reviewed discovery's explicit completeness, empty partial pages, retained initial
limit and offset, fixed cursor lifetime, replay window, exact roots and provider
order against the agent implementation and client result validator. The command
reference states the live-content consistency limits and cooperative cancellation
boundary. It makes no claim of a snapshot or a complete catalogue from a partial page.

On 2026-10-01, reviewed `query_paths` version two against the same registry and
its strict client result boundary. The command reference includes evaluation of
the root itself; provider order replaces the previous ascending array rule in
the authenticated canonical contract. Results require explicit, consistent
progress and unique paths. Schema annotations and published identities follow
that coordinated change. The 71 other command identities receive patch increments
for the shared metadata change, with their shapes and numeric limits unchanged.
This review describes source behavior; fresh full
gates, candidate-pair and deployment evidence are independent requirements.

On 2026-10-01, reviewed the continuation state and signature against both Java
and Rust implementations and the independently calculated shared signature
vector. `docs/AGENT_PROTOCOL.md` now describes the authenticated original limit,
its contract bounds, fixed expiry, version-two closed schemas and rejection of
old tokens without that claim. The review distinguishes opaque token migration
from unchanged outer command schemas and does not infer deployment evidence.

On 2026-10-02, reviewed both page searches against their runtime-owned traversal,
current-request row and component-witness validation, strict native progress
parsers, and the byte-identical shared schemas. The command reference states
exact case-sensitive phrase matching without excerpts, nearest-page ownership,
scoped `any`/`all` witnesses across pages, nested-page independence, and the
containing-page result allowed for an anchor inside page content. Provider order,
empty partial results, and bounded witness checks replace the old complete-tree
collection. The two result contracts have major version `2.0.0`; the other 70
identities receive patch increments for the shared metadata change. Numeric
limits and the other command shapes are unchanged. This is a review of source
contracts, independent of full-gate, candidate-pair and deployment evidence.

On 2026-10-02, renewed the scoped command-reference review for asset discovery
version `2.0.0`. Compared its provider-order progress envelope, root inclusion,
asset-descendant pruning, original binary length, metadata format fallback,
unknown-size filtering, all-tags default and canonical UTF-8 tag order with the
Java producer and typed native consumer. The original numeric limits and every
other payload shape remain unchanged; 71 other command identities receive the
shared-metadata patch increments. Retained cursor ownership, current authority,
latest-continuation replay and cooperative bounds remain explicit. This source
review does not establish full-gate, candidate-pair or deployment success.

## Non-obvious invariant comments

*A comment exists wherever a constraint is not visible from the code.*

Reviewed the reasons for retaining an identifier through delivery or waiter
detachment, keeping durable work independent of local cancellation, reserving
response capacity for workers, and enforcing deadlines outside blocked stream
threads. Retry comments distinguish monotonic durations from persisted wall
observations and leases. The gate documents why the default-feature build and
all-feature Clippy cover distinct configurations.

## Non-narration

*No comment narrates syntax the types and control flow already show.*

Reviewed the changed prose for repeated implementation narration. It explains
ownership, scheduling and evidence limits rather than restating the request
loop. The timer and gate comments explain failure propagation and measurement
scope. This is a review of these changes, not a claim that existing baseline
debt or every older comment has been retired.

## Present factual prose

*The documentation describes the code in this commit, not a plan for it.*

Reviewed against the coordinator, real executable cancellation test, interactive
transcripts, SQLite retry selector, source-policy checker and gate scripts.
Removed the obsolete no-automatic-retry statement and unsupported progress
forwarding claim. The baseline is acknowledged as existing debt. MCP makes no
claim that the local tests establish remote effect counts or native-platform
coverage. The gate documentation describes its actual commands; successful
full-gate or release validation is not inferred from this review.

## Reviewed document identities

This review applies only to the complete product documents named below. Each
identity is the SHA-256 digest of the document bytes reviewed; a changed byte
cannot inherit this review without an explicit renewed record.

| Document | SHA-256 |
|---|---|
| `README.md` | `8a69bafa9663c68fec56e78c38833710a693b232af2f865f9bee32774721468c` |
| `CONTRIBUTING.md` | `5ef483f3b3bfef920a65e432b9c17bcbf92b6b07a69b3a8b8a1653b14ebb2f7b` |
| `ARCHITECTURE.md` | `7b5fe4d75b11e63bae6c7d4de6c2426009025fc9b03c23d906e899996c5534cb` |
| `docs/AGENT_PROTOCOL.md` | `66ea30008f51815184794e06d54bfca835ab02e9770b3e806bfe6225beae5b4f` |
| `docs/COMMANDS.md` | `384e2c23616c0c2f13cc990a600ac9346d3cfaca91d949563f58536e73e2ec55` |
| `docs/CONFIGURATION.md` | `775ce5363790d1b44a91fdb7e7b2015d538cce224de5030f5e13edec87b089b3` |
| `docs/DAEMON.md` | `6d7b1961643b90e8b19212e836fa87973ae81274b195986f7cf78957af6c4f92` |
| `docs/MODEL_CONTEXT_PROTOCOL.md` | `a9ef98d045f8ffc127f8c7952cd5c581fb32a673ece618abb533043b8939914b` |
| `docs/RELEASES.md` | `c25a1800a6e20515d29569eec77003a267184a35e88d2a1c21d24630f0501d6a` |
| `docs/WORKFLOWS.md` | `0bd107a373f258069f2e9472f5c17bea2ccb675aa070ccc581d49ce11c9f3f6e` |

The 2026-10-05 submission timing observation review covers the new module's
numeric-only snapshots and its call after all common finite-response checks.
The exported-item descriptions state cumulative saturation, independent atomic
sampling and the required quiescent sequential observation boundary. Invariant
comments distinguish parsed diagnostic durations from validated acknowledgements
and terminal outcomes. The prose explains what those measurements exclude rather
than narrating the parser. It describes the implemented optional header handling
and makes no live measurement, attribution or release-validation claim.


On 2026-10-07, reviewed retryable submission error handling against the existing
status interpreter and the concrete finite transport. The protocol records JSON
media and decimal retry-header checks, lookup-first reconciliation, absence of
acceptance or nonexecution certainty, and the prohibition on an automatic second
POST. Real loopback assertions cover all six retryable statuses with generic,
malformed and empty JSON bodies in both selected authentication paths, plus four
nonretryable status controls. Existing acknowledgement, identity, argument and
deadline assertions remain. The public submission interfaces now document their
pre-send refusals. This review describes source behavior and does not settle old
remote operations or claim live capacity repair.
