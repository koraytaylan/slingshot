---
id: the-child-listings
title: "The Child Listings"
workstream: "0077"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-domain/src/command/list_child_nodes.rs
  - crates/slingshot-domain/src/command/list_content_fragment_models.rs
  - crates/slingshot-domain/src/command/list_page_templates.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/src/command/catalog.rs
  - crates/slingshot-domain/src/command/classification.rs
  - crates/slingshot-domain/src/command/classification_authoring.rs
  - crates/slingshot-domain/src/command/result_context.rs
  - crates/slingshot-domain/src/command/schema.rs
  - crates/slingshot-domain/src/command/schema_authoring.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - "crates/slingshot-domain/tests/fixtures/commands/**"
  - crates/slingshot-domain/tests/command_catalog.rs
  - crates/slingshot-domain/tests/command_schemas.rs
  - crates/slingshot-daemon/src/operation/durable_author_lookup/reconciliation/failure.rs
  - crates/slingshot-command-line/src/commands/page_lifecycle.rs
  - crates/slingshot-command-line/tests/model_context_protocol_tool_arguments.rs
  - crates/slingshot-command-line/tests/model_context_protocol_tool_catalog.rs
  - crates/slingshot-command-line/tests/operation_submission.rs
  - crates/slingshot-command-line/tests/live_adobe_experience_manager.rs
  - "crates/slingshot-command-line/tests/fixtures/live-adobe-experience-manager/registry-rows.jsonl"
  - "crates/slingshot-test-support/fixtures/command-golden-sessions/**"
  - "crates/slingshot-test-support/fixtures/model-context-protocol/**"
  - crates/slingshot-agent-protocol/tests/fixtures/identity-and-wire-schema/manifest.json
  - "crates/slingshot-development/tests/fixtures/protocol-compatibility/snapshot.json"
  - "schemas/commands/**"
  - schemas/command-canonical-json-1.json
  - docs/COMMANDS.md
  - docs/MODEL_CONTEXT_PROTOCOL.md
  - docs/DOCUMENTATION_REVIEW.md
  - policy/source-policy-baseline.tsv
status: done
merged_as: ""
---
# The Child Listings

A listing of pages answers one question and drops every other child of the same
anchor. The general read - every immediate child, and the immediate children of
one primary type - is what lets an operator see the shape of a tree, and a page
listing is the typed read with `cq:Page` as the type.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `ListChildNodesCommand` and `ListChildNodesByTypeCommand` with `root_path` and an optional `result_window`, and a typed request that also carries `primary_node_type`, naming the anchor `root_path` so its anchor failure is the one every other rooted search already reports.
3. Define a match as a resource that is an immediate child of the anchor and, for a typed listing, carries exactly the named primary type. A grandchild does not match, and a child of another type does not match the typed listing.
4. Report each match as its repository path, its primary node type, and an optional title, reusing the strict ascending repository-path order rather than defining a second ordering.
5. Allow the shared discovery failures and the shared root-anchor failures, preflight the anchor before enumeration begins, and supply request-context validation that refuses a match outside the anchor or of the wrong type.
6. Make `list_child_pages` the typed listing whose type is `cq:Page`, keeping its own wire name and result shape so an existing reader is unaffected.
7. Regenerate the committed schemas, schema manifest, catalog, projected-schema digests, protocol compatibility snapshot, command reference, and protocol reference from the declarations, and update every count that named the old surface.

**Tests:**

- An empty listing, a one-row listing, and a strictly ascending listing round-trip byte-identically for both new commands.
- A grandchild and a child of another primary type are proved not to match, against fixtures that contain both.
- Default and explicit result windows round-trip, and a continuation window beside an offset is refused.
- Missing and inaccessible anchors return only their closed failure with exactly `failure` and `root_path`, no matches, and no continuation token.
- Every one of the five shared computation budgets returns the closed budget failure with no partial page and no token.
- The registry, the catalog, and the schema inventory name the same commands, and every committed schema regenerates byte-identically to what the producer writes.
- Every projected tool has a golden session, every tool's minimal schema-legal call builds its own command, and the live-author set admits both new reads.

- **Done when:** `cargo test -p slingshot-domain --test command_catalog -p slingshot-domain --test command_schemas -p slingshot-command-line --test model_context_protocol_tool_catalog --test model_context_protocol_tool_arguments` proves immediate-child-only matching, the shared ordering and window rules, the exact anchor failures, every budget boundary, and that both new listings are advertised and reachable as commands without executing a search.
