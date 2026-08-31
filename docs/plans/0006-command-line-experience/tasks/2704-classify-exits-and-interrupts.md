---
id: classify-exits-and-interrupts
title: "Classify Exits And Interrupts"
workstream: "0027"
kind: task
depends_on:
  - render-human-and-machine-output
gated: false
touches:
  - crates/slingshot-command-line/src/exit_classification.rs
  - crates/slingshot-command-line/src/interrupt.rs
  - crates/slingshot-command-line/tests/exits_and_interrupts.rs
status: done
merged_as: ""
---
# Classify Exits And Interrupts

Apply the public exit taxonomy and make interruption report its exact submission/observation/artifact phase without cancelling a Sling Job or fabricating durable admission.

**Steps:**

1. Commit a complete error-to-exit table including every legal terminal kind/disposition pair, both recovery-evidence branches, every revised continuation/configuration/FileVault/add-component semantic failure object, every nonterminal/control/local error, and exact human/JSON interrupt transcripts before receipt, after accepted/replayed receipt during wait/result, before local operation-artifact publication, and before operation-free maintenance-result publication. Include independent signal races immediately before/after receipt validation, immediately before/at/after each atomic publication kind, and immediately before/during/after final output commit.
2. Implement named exit constants and classify only by Plan 0005's authoritative terminal disposition, never by matching a failure-name prefix. Map authoritative nonexecution to agent rejection, authoritative remote failure to terminal remote failure, fail-closed indeterminate—including an outcome-unknown semantic category only when paired with that disposition—to unavailable/indeterminate, and `ResultUnavailable`/AuthoritativeRemoteSuccess to unavailable. Preserve the full semantic failure object independently in every rendered output; no exit choice aliases or erases it.
3. Linearize interrupt selection against durable-receipt validation, atomic result publication, and final renderer commit. Before receipt, preserve the generated/caller key only as `retry_operation_identifier` and select closed `submission_interrupted_before_receipt` with literal admission unknown and no durable operation/state/revision. After receipt during wait/result, select closed `operation_observation_interrupted` with the validated accepted/replayed admission, revision, and durable identifier. Before operation-artifact publication, select closed `artifact_transfer_interrupted` with durable operation/artifact identifiers and no destination/staging path. Before maintenance-result publication, select closed `maintenance_result_transfer_interrupted` with only target digest/maintenance-result identifier and no operation/slot/path. Use the exact architecture human stderr templates with empty stdout, or exactly one canonical JSON envelope on stdout; all four exit `130`.
4. Make successful atomic operation-artifact or maintenance-result publication itself win as exit-`0` success over a signal at/after that commit. Defer handled signals through its bounded final success-rendering section; if forced process/output loss leaves rendering incomplete, preserve the exact receipt so the next identical invocation authenticates the destination and re-renders success without transfer or collision. Make any other completely committed final rendering win over a later signal and otherwise emit exactly one applicable pre-commit phase outcome. Assert no interrupt path sends a daemon/agent cancellation, changes durable operation/maintenance state, or invents terminal evidence.

**Tests:**

- `exits_and_interrupts` covers every exit value and both output modes, proving rejection/nonexecution, remote failure, fail-closed lost/retired/integrity, post-success recovery, terminal result-unavailable, and all four interruption outcomes remain distinct.
- Exhaustive rows prove every revised semantic category is accepted only with its Plan 0005-authorized disposition, retains all registered fields, and cannot choose an exit merely from its literal spelling.
- Pre-receipt transcripts prove admission is always reported unknown, the retry identifier survives exactly, and no field claims durable admission. Post-receipt wait/result transcripts preserve exact receipt facts and durable observability. Pre-publication operation-artifact transcripts name only operation/artifact identities; maintenance-result transcripts name only target/result identity. Neither publishes a destination or exposes a path. At/post-publication races never emit interruption or republish; interrupted-output cases retain and consume the exact success receipt. Every interrupt leaves remote cancellation counts at zero.

- **Done when:** `cargo test -p slingshot-command-line --test exits_and_interrupts` proves every revised failure has one disposition-derived stable exit without category alias/loss, indeterminate and post-success-unavailable outcomes remain truthful, each pre-receipt/post-receipt/operation-artifact/maintenance-result pre-publication interruption has one exact non-authoritative rendering, and either atomic publication can only commit truthful success with receipt-backed re-rendering and never remote cancellation.
