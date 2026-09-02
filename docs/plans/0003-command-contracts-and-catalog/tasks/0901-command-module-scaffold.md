---
id: command-module-scaffold
title: "Command Module Scaffold"
workstream: "0009"
kind: chore
depends_on: []
gated: false
touches:
  - crates/slingshot-domain/src/command/**
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - schemas/command-contract-limits-1.json
  - crates/slingshot-domain/tests/command_module_inventory.rs
  - crates/slingshot-domain/tests/command_contract_limits.rs
status: done
merged_as: "188d9253c29def2b42ee47103cb6eb9043ae1228"
---
# Command Module Scaffold

Adopt Plan 0001's structural command root and own the one exact command-leaf inventory that lets every later command task own one source file.

**Steps:**

1. Assert that Plan 0001 already exported the empty `slingshot-domain::command` structural root and that its structural fixture contains no command-specific leaf list.
2. Write `crates/slingshot-domain/tests/fixtures/command-module-inventory.txt` first with the exact foundation, command, catalog, and schema leaf names specified by this plan architecture; this is the sole exact command-leaf authority.
3. Commit canonical `schemas/command-contract-limits-1.json` with format `slingshot.command-contract-limits/1`, every exact public constant and unit from the architecture tables, and the exact twelve-command-to-`1.0.0` map. Canonicalize it with `slingshot.schema-canonical/1`; reject missing, additional, renamed, duplicate, differently valued, or ad-hoc public constants. This file, not a Rust module, schema, catalog, or external-agent implementation, is the sole normative limits/version authority.
4. Adopt and update the existing `crates/slingshot-domain/src/command/mod.rs`, then create one documented, compiling leaf module file for every inventory entry without changing the crate root. All command-specific leaves remain empty; the foundation `command_identity.rs` leaf alone implements CommandSemanticContractVersion and typed access to the limits manifest before command modules need them.
5. Define CommandSemanticContractVersion from the architecture's exact ASCII ABNF. Charge the identifier bound as three core identifiers plus every prerelease and build identifier. Apply the ten-digit bound separately to each core numeric and all-digit prerelease identifier, never to build identifiers. Reject leading zeros only in core and numeric prerelease identifiers; preserve build spelling so `1.0.0+01` is legal while `1.0.0-01` is illegal. Enforce the complete-byte and identifier bounds before schema-URN construction. Keep every other leaf factual and limited to the present empty module boundary; do not add placeholders, future remarks, or unimplemented macros.
6. Add an inventory test that compares declarations, leaf files, and fixture entries exactly and rejects undeclared source files or a duplicate command-leaf inventory in Plan 0001's structural fixture. Add an independent limits test for canonical bytes/digest, all below/at/above boundaries, exact units/equations, largest-envelope fit, the zero-through-signed-64-bit AssetByteLength domain, the exact 262,144-byte canonical-loaded-document Inline boundary, and the complete exact `1.0.0` map. Add independent version vectors for release/prerelease/build spellings, legal numeric build identifiers including `+01`, illegal core/numeric-prerelease leading zeros including `-01`, identifiers whose ten-digit charge is exact/over, total identifier count exact/over across prerelease and build, Unicode/control/reserved-URI/complete-byte cases, precedence-independent exact compatibility equality, and schema-URN insertion.

**Tests:**

- The test fails if Plan 0001's structural root/export is absent or if its fixture enumerates command leaves.
- The inventory test fails when a fixture entry has no source file.
- The inventory test fails when a source module has no fixture entry.
- The limits manifest round-trips byte-for-byte, exposes exactly the architecture inventory and units, and rejects any absent, extra, renamed, duplicated, revalued, or separately declared public command constant.
- Boundary vectors cover every limit below/at/above where applicable, including AssetByteLength zero/maximum/next/negative/overflow and canonical loaded-document Inline exact/next disposition, checked equations never wrap, and complete-envelope vectors prove maximum configuration observations, discovery matches/tokens, load documents/artifacts, package filters/manifests/artifacts, and mutation results fit their governing outer bounds.
- The command-version inventory is exactly the twelve stable wire names, each at `1.0.0`; a command module cannot choose or redeclare another version.
- The crate compiles with every planned module present and no unused placeholder value.
- Canonical bounded Semantic Versioning vectors round-trip exactly. Core and numeric prerelease leading zeros fail, numeric build leading zeros round-trip, each numeric digit charge and the combined three-core-plus-prerelease-plus-build identifier charge passes at and fails above its boundary, and slash, colon, question mark, number sign, whitespace, controls, Unicode, or empty identifiers fail before schema identity construction.
- The repository documentation-policy test accepts every new module document.

- **Done when:** cargo test -p slingshot-domain --test command_module_inventory and cargo test -p slingshot-domain --test command_contract_limits pass with Plan 0001's structural command root adopted, an exact command-leaf bijection, one canonical versioned limits/version authority, complete boundary/envelope-fit proofs, and the workspace formatting and lint gates succeed.
