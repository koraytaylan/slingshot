---
id: expose-configuration-and-page-query-commands
title: "Expose Configuration And Page Query Commands"
workstream: "0026"
kind: task
depends_on:
  - define-command-invocations
gated: false
touches:
  - crates/slingshot-command-line/src/commands/configuration.rs
  - crates/slingshot-command-line/src/commands/page_query.rs
  - crates/slingshot-command-line/src/commands/path_query.rs
  - crates/slingshot-command-line/src/predicate_arguments.rs
  - crates/slingshot-command-line/tests/configuration_and_page_query_commands.rs
status: planned
merged_as: ""
---
# Expose Configuration And Page Query Commands

Expose Open Service Gateway Initiative configuration inspection plus structured path, phrase, template, and component-usage queries as typed read operations.

**Steps:**

1. Commit invocation/request pairs for persistent identifiers; path roots and optional node types; required page roots; byte-preserved content phrases including internal and rejected leading/trailing Unicode whitespace; template paths; non-empty component sets; and exact registry version/limits/schema identities.
2. Import the exact `property_path`, `operator`, `type`, `cardinality`, `value`, and `values` spellings plus all ten operator tags and six scalar type tags from Plan 0003 fixtures rather than declaring a second encoding.
3. Implement one `--property-predicate <canonical-json-object>` parser that enforces no value for `exists`; one PropertyValue for `equals|not_equals`; non-empty ordered-unique homogeneous scalars for `scalar_in|list_contains_any|list_contains_all`; and one ordered string/integer/decimal/date-time scalar for the four comparison tags.
4. Reject unknown/surplus fields, noncanonical integer/decimal/date-time strings, heterogeneous/mixed cardinality, unsupported boolean/repository-path/list comparisons, and raw query text before daemon access.
5. Pass the exact argument bytes directly to SearchPhrase validation without trimming or normalization. Preserve every accepted byte/internal whitespace scalar and surface Plan 0003's noncanonical leading/trailing Unicode 15.1 White_Space rejection before daemon access.
6. Expose `--offset` and `--limit`, or mutually exclusive `--continuation-token`, on every path and page discovery leaf. Validate the token only through the manifest-owned Plan 0003 opaque token type and exact byte bound, then pass it unchanged without decode, trim, normalization, log, or synthesis.
7. Implement distinct command variants and registry mappings with fully named Rust declarations. Snapshot exact help/request payloads, require exact `1.0.0` and canonical limits/role digests, assert every operation is read-only, and pin the complete configuration lookup/value/result plus continuation failure metadata without CLI aliases.

**Tests:**

- `configuration_and_page_query_commands` pins all five query shapes, required roots, component match modes, result windows, continuation tokens, every Plan 0003 type/discriminator tag, all ten predicate variants, exact arity/domain rules, and diagnostics.
- Continuation fixtures prove `--continuation-token` conflicts independently with `--offset` and `--limit` and survives request construction byte-for-byte.
- SearchPhrase fixtures prove accepted leading/trailing bytes are never trimmed into another phrase: valid internal Unicode whitespace remains exact, while every leading/trailing Unicode White_Space case fails rather than being rewritten.
- Configuration descriptor fixtures expose all and only the revised lookup, unsupported, malformed, value-budget, and result-budget objects/reasons/literals for exact version `1.0.0`; page/path descriptors expose all five closed continuation failures.
- Registry assertions prove no query variant resolves to a mutation descriptor.

- **Done when:** `cargo test -p slingshot-command-line --test configuration_and_page_query_commands` proves predicates, byte-preserved SearchPhrase, opaque bounded continuation, exact registry identity/failures, and every pagination form map to their exact read requests while noncanonical or malformed inputs fail before daemon access.
