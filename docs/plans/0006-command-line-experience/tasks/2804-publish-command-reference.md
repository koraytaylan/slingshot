---
id: publish-command-reference
title: "Publish Command Reference"
workstream: "0028"
kind: chore
depends_on:
  - scan-command-output-for-secrets
gated: false
touches:
  - crates/slingshot-command-line/tests/command_reference.rs
  - docs/COMMANDS.md
status: done
merged_as: ""
---
# Publish Command Reference

Publish the tested command grammar, daemon behavior, output contract, exit taxonomy, detachment semantics, and representative workflows from the same metadata used by the parser.

**Steps:**

1. Commit a reference snapshot checklist covering every command leaf/option, typed predicate, byte-preserved SearchPhrase rule, canonical asset-set ordering, opaque token bound, exact daemon-runtime and author-agent-transport digest provenance, exact `slingshot.command-canonical-json/1` artifact/annotation binding and raw-byte/schema/typed order, unchanged five-field registry contract, exact closed Plan 0002 configuration diagnostics, every revised closed semantic failure, machine-envelope field including all four interruption local-error variants, terminal disposition, operation-artifact and maintenance-result URIs, maintenance metadata/read authentication, and exit class.
2. Render `docs/COMMANDS.md` from command metadata and add concise examples for configuration check, content, package, queries, assets, required non-idempotent operation keys, safe same-identifier rerun after lost response or pre-receipt admission-unknown interrupt, post-receipt observation detachment, pre-publication interrupted/resumed operation-artifact or maintenance-result transfer, post-publication receipt-backed success re-rendering without collision, page mutation, detachment, bounded target-partition list/result, current-target expected-revision/category recovery resume and its receipt lifetime, secure artifact retrieval, and current or explicitly selected historical-partition complete maintenance preview followed by digest-bound applied/replayed receipt and exact target-qualified `maintenance result` retrieval. Explain caller-digest comparison against target-and-identifier metadata, full start revalidation, the sole apply ownership transition, and inline versus operation-free-associated maintenance including supersession/retirement, without implying the complete maximum manifest fits below 4096 bytes or that the identifier reveals its digest.
3. Generate failure tables directly from registry metadata, retaining exact literals and registered fields without aliases. Add documentation tests for generated sections, links, examples, stream claims, exact phase-specific human/JSON interruption templates, atomic publication as success, no false pre-receipt durability, interrupt-does-not-cancel behavior, and present-state-only language.

**Tests:**

- `command_reference` compares generated reference sections and the complete coverage checklist.
- Example argument vectors parse to the exact requests pinned by the command tests.
- A text-policy case rejects developer commentary, TODO markers, and past/future implementation narration while excluding `docs/plans` from the product-document scan.

- **Done when:** `cargo test -p slingshot-command-line --test command_reference` proves the reference and executable grammar describe the same authenticated runtime/transport/canonical-contract/annotation and exact five-field registry identity, canonical phrase/asset/token contract, closed configuration diagnostics, lossless failure inventory, both pre-publication interruptions plus receipt-backed publication success, inline/operation-free-associated complete maintenance metadata/read behavior and exact URI, terminal evidence, target guidance, and machine envelope.
