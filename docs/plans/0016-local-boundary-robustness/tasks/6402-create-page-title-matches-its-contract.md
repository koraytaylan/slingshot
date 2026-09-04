---
id: create-page-title-matches-its-contract
title: "Create Page Title Matches Its Contract"
workstream: "0064"
kind: task
depends_on: ["property-documents-are-bounded-and-unambiguous"]
gated: false
touches:
  - crates/slingshot-domain/src/command/create_page.rs
  - crates/slingshot-domain/src/command/schema.rs
  - crates/slingshot-command-line/src/commands/page_mutation.rs
  - crates/slingshot-command-line/tests/page_mutation_commands.rs
  - crates/slingshot-domain/tests/command_contract_limits.rs
status: planned
merged_as: ""
---
# Create Page Title Matches Its Contract

`create_page.title` is a plain deserializable `String`. Its schema uses the property-string limit while other page-title commands use the smaller typed `PageTitle` bound, and the CLI performs no constructor validation.

**Steps:**

1. Use the shared `PageTitle` value object for `CreatePageCommand.title` and construct it at every CLI, serde, fixture, and adapter boundary.
2. Change the generated schema to the canonical `maximum_page_title_bytes` authority and prove generated schema, typed constructor, and wire deserialization agree.
3. Add exact-byte and plus-one Unicode-aware cases through direct serde and the real CLI, with zero submission for the refusal.
4. Add a registry-wide assertion that every schema string bound maps to the same runtime value constructor or an explicit independently tested raw-byte boundary.

- **Done when:** no CLI- or serde-produced `create_page` request can exceed the title bound its catalog publishes, exact-bound titles pass, plus-one titles refuse before submission, and the cross-command schema/runtime parity assertion holds.
