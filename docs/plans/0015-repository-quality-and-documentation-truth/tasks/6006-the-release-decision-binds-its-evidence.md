---
id: the-release-decision-binds-its-evidence
title: "The Release Decision Binds Its Evidence"
workstream: "0060"
kind: task
depends_on: ["every-declared-release-cache-input-is-bound"]
gated: false
touches:
  - crates/slingshot-development/src/release_acceptance.rs
  - crates/slingshot-development/src/release_acceptance/evidence.rs
  - crates/slingshot-development/src/main.rs
  - crates/slingshot-development/tests/release_acceptance.rs
  - crates/slingshot-development/tests/fixtures/release-acceptance/decisions.jsonl
status: complete
merged_as: "pending"
---
# The Release Decision Binds Its Evidence

The acceptance manifest carries identities and digests for its run, inputs, and gate reports, but the verifier checks only the source commit and closed list of gate names and outcomes. Every other field can be replaced without changing its answer.

**Steps:**

1. Give `verify-release-acceptance` independent expected commit, tree, provider-reported workflow identity, coordinator row, isolation contract, platform evidence, review record, and acceptance-directory inputs. Derive values from committed authorities where possible instead of copying the manifest.
2. Recompute every input and report digest from bounded retained regular files without following links. Require exactly one report per gate and reject missing, extra, renamed, linked, special, or over-bound entries.
3. Validate canonical commit, tree, digest, and provider shapes; require the authority-designated coordinator; and derive the overall result from the independently verified gate set instead of trusting the manifest word.
4. Add one-field mutation fixtures for every manifest member and report, including two reports exchanged under each other's names, and prove each refuses before publication can consume it.

- **Done when:** changing any run identity, input digest, report byte/name/digest, gate outcome, or overall outcome fails verification against independent expectations, while the unmodified bounded acceptance directory verifies offline for exactly its intended commit and tree.
