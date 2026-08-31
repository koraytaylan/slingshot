---
id: author-agent-transport-contract
title: "Author-Agent Transport Contract"
workstream: "0019"
kind: task
depends_on: []
gated: false
touches:
  - policy/author-agent-transport-contract-1.json
  - policy/author-agent-transport-contract-1.sha256
  - crates/slingshot-development/tests/fixtures/workspace-module-map/module-ownership.txt
  - crates/slingshot-domain/src/lib.rs
  - crates/slingshot-domain/src/author_agent_transport_contract.rs
  - crates/slingshot-domain/tests/author_agent_transport_contract.rs
  - crates/slingshot-domain/tests/fixtures/author-agent-transport-contract/**
status: done
merged_as: ""
---
# Author-Agent Transport Contract

Plan 0005 needs one canonical versioned source for every public wire, timeout, retry, retention, key-authority, handoff, and hard-cap value before any schema or simulator can use those values.

**Steps:**

1. Independently author exact canonical `policy/author-agent-transport-contract-1.json` and SHA-256 sidecar from the architecture tables, including every unit, formula, protocol version, hard cap, distinct DNS/TCP-versus-TLS timeout, and the independently charged 431-byte maximum continuation key ring. Pin the existing author header-limit interpretation: `MAXIMUM_AUTHOR_RESPONSE_HEADER_BYTES` bounds one decoded field name plus value, `MAXIMUM_AUTHOR_RESPONSE_HEADER_COUNT` bounds decoded fields, and `MAXIMUM_AUTHOR_RESPONSE_HEAD_BYTES` independently bounds the raw HTTP/1.1 head, encoded HTTP/2 header block, and checked decoded status/field/separator aggregate; no fourth framing or compression constant exists.
2. Adopt the existing domain crate root, declare `author_agent_transport_contract` exactly once, and parse repository and embedded bytes into one immutable typed `AuthorAgentTransportContract`; reject missing/additional/reordered/differently valued members, digest mismatch, arithmetic overflow, formula failure, or a local override/default.
3. Inventory every Plan-0005 schema, fixture, transport, key, store, FakeAuthor, conformance, and documentation consumer. Require each public value to resolve through this typed contract and fail a source scan for ad-hoc aliases or literals.
4. Expose `AuthorAgentTransportContractDigest` for capability, operation, storage, and recovery provenance without exposing any secret or making a mismatched operation executable.

**Tests:**

- Independent byte/digest/formula vectors accept the exact manifest, reconstruct the 431-byte key ring member by member, and reject every one-member/value/order/whitespace/newline mutation.
- Exact maxima fit and the next unit refuses for every dimension, including each independent HTTP/1.1 raw/decoded and HTTP/2 encoded/decoded application of the three author-head limits; no unchecked multiplication or duration addition exists.
- Regeneration in a temporary directory is byte-identical, and the inventory proves every consumer uses the typed source.

- **Done when:** `cargo test -p slingshot-domain --test author_agent_transport_contract` proves canonical bytes, digest, formulas, boundaries, complete consumers, and no ad-hoc public value, and all workspace gates succeed.
