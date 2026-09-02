---
id: command-registry-rows
title: "Command Registry Rows"
workstream: "0051"
kind: task
depends_on:
  - retry-replication-queue-entry
gated: false
touches:
  - crates/slingshot-domain/src/command/catalog.rs
  - crates/slingshot-domain/src/command/classification.rs
  - crates/slingshot-domain/src/command/classification_foundation.rs
  - crates/slingshot-domain/src/command/classification_authoring.rs
  - crates/slingshot-domain/src/command/classification_platform.rs
  - crates/slingshot-domain/src/command/classification_process.rs
  - crates/slingshot-domain/src/command/classification_administration.rs
  - crates/slingshot-domain/src/command/result_context.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/command_catalog.rs
  - crates/slingshot-domain/tests/fixtures/commands/catalog.json
status: done
merged_as: "0a993d08aeb2c62f324e47cf179194d5f5c3d0ec"
---
# Command Registry Rows

Fifty-two contracts exist and nothing publishes them. This task widens what access means, writes the row every new command is classified by, and keeps one ascending table and one pair of parallel enumerations - under a size rule that a single sixty-four-row file cannot meet.

**Steps:**

1. Widen the documented meaning of `Read`, `Write`, and `Destructive` from repository and replicated content to any state the author retains after the command returns, and say what retained state includes. Change no classification of the twelve existing rows, because the widening changes no answer for them.
2. Move `ClassificationRow`, the shared failure-category constants, and the ordered table into `classification.rs`, and the rows themselves into five family leaves as one named constant per command. The table stays one array in one ascending order whose entries name those constants.
3. Move every `AnswersCommand` implementation into `result_context.rs`, one per command and nothing else in the file.
4. Add sixty-four entries to the family macro so `Command` and `CommandResult` stay parallel and `validate_result_for_command` stays one rule rather than sixty-four matches.
5. Declare the artifact slots each new command has, which is none: a command that declares no slot forbids one, and fifty-two empty declarations are fifty-two statements rather than fifty-two omissions.
6. Regenerate the committed catalog fixture from the registry rather than editing it, and keep every path other crates already import exactly where it is.

**Tests:**

- The catalog serializes byte-for-byte as the committed fixture.
- Sixty-four commands appear exactly once each, in ascending wire-name order, and the catalog and the schema inventory name the same set.
- Every row's access, destructive, and idempotency classification equals what the architecture's table says, checked row by row rather than in aggregate.
- Every `Write` requires an operation key and every `Read` refuses one, taken from the idempotency column alone.
- The twelve existing rows are unchanged, proved against the values they published before this plan.
- Every enumeration variant maps to exactly one descriptor and back, and a result of another command is refused with a variant mismatch while a result of the same command answering another request is refused with a request mismatch.
- No file this task touches exceeds the source policy's line limit, and no function exceeds its complexity limit.

- **Done when:** `cargo test -p slingshot-domain --test command_catalog` passes with sixty-four ascending rows, the widened definitions, the unchanged twelve, and a committed fixture regenerated from the registry, and the source policy accepts every file the split produced.
