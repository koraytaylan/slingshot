---
id: property-documents-are-bounded-and-unambiguous
title: "Property Documents Are Bounded And Unambiguous"
workstream: "0064"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-command-line/src/property_document.rs
  - crates/slingshot-command-line/tests/page_mutation_commands.rs
status: complete
merged_as: "pending"
---
# Property Documents Are Bounded And Unambiguous

The property-file reader promises bounds before construction and duplicate refusal, but reads the entire file and deserializes to `Value` first. Duplicate keys have already selected a winner and oversized whitespace has already consumed memory by the time validation begins.

**Steps:**

1. Apply the canonical maximum command-argument byte limit to metadata and a limit-plus-one read before allocating the complete document; handle growth between metadata and read conservatively.
2. Deserialize through a typed map visitor that counts entries, enforces nesting while reading, and rejects every semantic duplicate at the outer property and inner reserved-member levels.
3. Preserve the typed property-value validation and maximum mutation-property count without first materializing an unbounded generic tree.
4. Test exact and plus-one bytes, huge whitespace, too many members, first/last duplicate variants, escaped duplicate keys, nested reserved-key duplicates, and a growing file; prove every refusal sends nothing to the daemon.

- **Done when:** property input is byte/member/depth bounded and duplicate-free before request construction, no duplicate spelling chooses a mutation value, and all refusals have zero daemon submission.
