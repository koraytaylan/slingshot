---
id: command-registry
title: "Command Registry"
workstream: "0013"
kind: task
depends_on:
  - load-content-as-javascript-object-notation
  - inspect-open-service-gateway-initiative-configuration
  - query-paths
  - find-pages-containing-phrase
  - find-pages-by-template
  - find-pages-using-components
  - find-assets-by-metadata
  - find-assets-referenced-by-page
  - replicate-content
  - download-content-package
  - create-page
  - add-component
  - command-schemas
gated: false
touches:
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/src/command/catalog.rs
  - crates/slingshot-domain/tests/fixtures/commands/catalog.json
  - crates/slingshot-domain/tests/command_catalog.rs
status: planned
merged_as: ""
---
# Command Registry

Publish one ordered descriptor registry so every presentation and transport layer discovers the same command names and behavioral metadata.

**Steps:**

1. Commit the canonical catalog fixture with every wire name, exact semantic version `1.0.0`, exact `slingshot.command-contract-limits/1` digest, separate committed argument/result schema digests, the architecture's exact AccessClassification/DestructiveClassification/IntrinsicIdempotencyClassification row, ordered artifact slot/requirement/media/maximum-length/suggested-name policy, command result-envelope maxima, allowed structured-failure categories/fields/dispositions, required agent-conformance inventory, and every descriptor field.
2. Implement closed Command and CommandResult enums in command/mod.rs, each with one command-specific typed variant, then implement all descriptor fields from the closed twelve-row classification authority, applicable named complete canonical success-result maxima with their exact charge rule, ordered stable structured-failure categories, and ordered OptionalAlternative/Required/empty remote artifact declarations. Load and package are Read/NonDestructive but NotIntrinsicallyIdempotent because remote-artifact publication requires a stable caller operation key; replication is Write/Destructive, while create/add are Write/NonDestructive and all three are NotIntrinsicallyIdempotent.
3. Register every Command/CommandResult pair exactly once in stable wire-name order, consuming its wire name, `1.0.0`, limits digest, and role digests from the already committed authorities rather than defining a parallel identity/constant table.
4. Make duplicate wire names, any version other than the exact manifest value, a missing/malformed limits digest, missing/malformed or role-swapped schema digests, ad-hoc public limits, and a missing Command variant fail catalog construction; the wire name is definitionally the agent capability name and no separate alias exists. Identical bytes may legitimately yield the same digest without conflating digest roles.
5. Expose no presentation-specific flags, endpoint paths, credentials, or transport values from the registry.
6. Implement `validate_result_for_command(&Command, &CommandResult)` once in the domain registry: require variant identity and compare every checkable echoed/derived invariant, but do not decode continuation tokens or claim to re-execute repository semantics. Commit substitution fixtures that distinguish domain-detectable path/name/identifier/count mismatches from same-variant cases that only Plan 0005's authenticated canonical submitted-command digest can bind.
7. Walk the committed schema, canonical-byte pointer, command/result fixture, and independently authored agent-conformance inventories and require one exact entry for every enum pair, descriptor/wire capability name, semantic version, canonical-contract-bound schema role/digest, ordered-collection rule, and scenario set.

**Tests:**

- The catalog serializes byte-for-byte as the committed fixture.
- Every Command and CommandResult variant maps to exactly one descriptor and back.
- Stable names are unique and sorted.
- The exact twelve AccessClassification/DestructiveClassification/IntrinsicIdempotencyClassification rows byte-match architecture. Read-only/live admission consumes only AccessClassification/DestructiveClassification; Model Context Protocol `readOnlyHint` derives exclusively from AccessClassification, and `destructiveHint` derives exclusively from DestructiveClassification. Model Context Protocol `idempotentHint` and operation-key policy both derive exclusively from the complete seven-`IntrinsicallyIdempotent`/five-`NotIntrinsicallyIdempotent` column: the hint is true exactly for the former, while a caller operation key is required exactly for the latter. Load and package remain Read/NonDestructive while requiring keys and exposing a false `idempotentHint`; replication is the sole Destructive row; create/add are Write/NonDestructive.
- Load declares exactly OptionalAlternative `loaded_content_json`/`application/json`/`loaded-content.json`/its maximum length, package exactly Required `content_package`/`application/zip`/`<package_name>.zip`/MAXIMUM_PACKAGE_OUTPUT_BYTES, all other remote manifests are empty, and `structured_result` is absent.
- Package exposes exact `pattern_rejected`, `filevault_profile_unsupported`, `filevault_filter_unrepresentable`, anchor/read/build/staging/publication failures, and `evaluation_budget_exceeded`, whose budget is restricted to `candidate_paths|pattern_evaluations|selected_paths|filter_document_bytes|package_manifest_bytes|archive_entries|uncompressed_input_bytes|package_output_bytes`; a duration literal or missing, duplicate, or undeclared failure metadata fails registry construction.
- Query plus the four rooted discovery entries expose exact `root_not_found|root_access_denied` with `root_path`; referenced-asset discovery exposes exact `page_not_found|page_access_denied|page_invalid` with `page_path`. All six additionally expose exact `discovery_budget_exceeded` and five continuation-token categories. Configuration inspection exposes the four lookup failures, unsupported/malformed/value-budget failures with their exact closed reasons/literals, and result-budget failure; load exposes its four closed failures; replication exposes its four preflight and three admission categories with exact zero/partial/unknown dispositions; create exposes seven and add exposes eight authoritative-no-effect categories, including `parent_not_orderable`, plus their outcome-unknown category without a no-effect assertion.
- MAXIMUM_DISCOVERY_RESULT_BYTES fits each command's maximum legal single match plus complete envelope/token, and lowering it by one below any computed requirement fails the registry proof.
- MAXIMUM_INSPECTED_CONFIGURATION_RESULT_BYTES fits the largest legal single observed property plus the complete success envelope; property count and canonical bytes are descriptor/schema bounds, and lowering the byte maximum by one below that computed requirement fails the registry proof.
- Every descriptor's one non-empty wire name is its agent capability identity and has no independently variable capability alias; every descriptor has exact `1.0.0`, one limits digest, two distinct schema-digest roles whose schema bytes bind the exact canonical-JSON-contract digest, a complete canonical-array-pointer inventory, and present-state descriptive text.
- Compatibility equality is true only when wire-name capability, exact semantic version, limits digest, argument digest, and result digest all match.
- Request-context fixtures reject every variant mismatch and checkable echoed/derived mismatch. Same-variant substitutions without a distinguishing result fact are explicitly deferred to Plan 0005's authenticated submitted-command-digest check and are not presented as proof of repository semantic re-execution.
- Every repository-dependent command has a complete independently authored agent-conformance scenario inventory, and the registry rejects a missing or extra scenario without claiming that Rust executes it.

- **Done when:** cargo test -p slingshot-domain --test command_catalog passes the exact snapshot and proves the full enum/descriptor-as-wire-capability/exact-`1.0.0`/limits-digest/canonical-contract-bound-schema-role-digest/canonical-pointer/fixture/agent-scenario bijection, exact twelve-row safety/effect classifications, exclusive classification-to-Model-Context-Protocol-hint mapping, seven/five intrinsic-idempotency/operation-key policy, exact result-context invariants, exact anchor/configuration/package/mutation failure metadata, and every per-command envelope-fit bound.
