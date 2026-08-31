---
id: operational-contract-limits
title: "Operational Contract Limits"
workstream: "0040"
kind: task
depends_on: []
gated: false
touches:
  - schemas/command-contract-limits-1.json
  - crates/slingshot-domain/tests/command_contract_limits.rs
status: planned
merged_as: ""
---
# Operational Contract Limits

Every bound the new families enforce has to exist before the first type reads one, and it has to exist exactly once. This task extends the normative manifest with the identity, value, collection, budget, and result limits the architecture names, and the fifty-two new command versions, without touching a value the twelve existing commands already read.

**Steps:**

1. Add every limit the architecture's three tables name to the `limits` map of `schemas/command-contract-limits-1.json`, in the canonical ascending key order the format requires, as unsigned base-ten integers with durations in milliseconds.
2. Add the fifty-two new wire names to `command_semantic_contract_versions`, each with exact initial version `1.0.0`, keeping the map ascending.
3. Change no existing limit value and remove none. The manifest digest changes because the document changed; every consumer reads the digest rather than restating it, so nothing else records the old one.
4. Extend the manifest assertions so the new keys are covered by the same rules as the old ones: every limit is read by name, no second declaration of a limit value exists anywhere in the command family, and the document a reader regenerates is byte-identical to the committed one.

**Tests:**

- The committed manifest parses, is canonical, and round-trips byte-identically.
- Every new limit is reachable through `CommandContract::limit` by its exact name, and an unknown name still panics as a defect rather than returning a default.
- The version map holds exactly sixty-four entries, every value is `1.0.0`, and every key matches a wire name the schema inventory will publish.
- No new limit value is written down a second time inside `crates/slingshot-domain/src/command`, under the existing redeclared-bound scan and its name-based rather than value-based rule.
- Duration limits remain milliseconds, and the declared duration unit is unchanged.

- **Done when:** `cargo test -p slingshot-domain --test command_contract_limits` passes with the extended manifest, sixty-four versions, no redeclared bound, and no change to any value the twelve existing commands read.
