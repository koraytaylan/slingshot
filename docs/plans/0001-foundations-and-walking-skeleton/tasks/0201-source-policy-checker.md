---
id: source-policy-checker
title: "Source Policy Checker"
workstream: "0002"
kind: task
depends_on:
  - dependency-direction-check
gated: false
touches:
  - policy/abbreviated-identifiers.txt
  - policy/external-interface-identifiers.toml
  - policy/documentation-rules.toml
  - policy/source-policy.toml
  - crates/slingshot-development/src/source_policy.rs
  - crates/slingshot-development/src/main.rs
  - crates/slingshot-development/tests/source_policy.rs
  - "crates/slingshot-development/tests/fixtures/source-policy/**"
status: done
merged_as: "02727756a9ea4efb2c4e6744d27f0b85df85b2d1"
---
# Source Policy Checker

Repository constraints become reviewable failures with file, symbol, and line evidence instead of conventions that drift as crates grow.

**Steps:**

1. Author accepted and rejected Rust, executable-script, workflow, and Structured Query Language fixtures for the exact code-file line boundary, declared-identifier tokens, the closed literal fully-qualified external-trait/platform-interface exception, alias/re-export/local-lookalike attempts, semantic/operational numeric-expression placement, structural allowances, exact cyclomatic ceiling, immutable action references, permissions including the closed release-attestation exception, checkout credential persistence, shell expression flow, placeholder bodies, missing exported-item documentation, every unsafe Rust syntax category, exact configured task-marker/placeholder tokens, and planning-only headings. The accepted Rust fixture contains the word `unsafe` only in a comment and string, proving syntax classification rather than token matching.
2. Store `MAXIMUM_CODE_FILE_LINES` as 1,000 and `MAXIMUM_CYCLOMATIC_COMPLEXITY` as 10 once in `policy/source-policy.toml`; define the full-word replacement table, exact machine-checkable documentation forms, and the separate semantic documentation/comment review checklist in their policy files. Store each exceptional exact leading-`::` interface path, locked standard-library/dependency identity, required signature item, and target condition in `policy/external-interface-identifiers.toml`; it is closed interface data, not a token allowlist. Plan 0009 reads these same values and never duplicates them.
3. Implement syntax-aware Rust, shell, and Structured Query Language checks for locally declared names, named semantic/operational numeric values or expressions, placeholder bodies, per-function/trigger/conditional complexity, physical line count, applicable documentation, and unsafe Rust syntax; reject every unsafe block, unsafe function (including an unsafe foreign-ABI function), unsafe trait, unsafe implementation, unsafe foreign block, and Rust 2024 unsafe attribute, with no allow/expect or documentation escape hatch. Add structural workflow plus scoped Markdown/Rust-documentation checks limited to exact configured tokens/headings and nonempty exported-item documentation. Do not infer whether prose is historically framed, complete, accurate, or merely narrates adjacent code; record those semantic questions only in the mandatory review checklist. Exempt an abbreviated Rust signature declaration only when the implementation header literally names one tabled leading-`::` fully qualified interface path, the item/signature and target condition match, and locked Cargo metadata proves the tabled external package identity. Reject aliases, renamed imports, re-exports, glob imports, inferred short paths, local lookalikes, inherent methods, and project-owned traits without attempting compiler HIR/name resolution; continue checking every project-controlled local in the body. Require full-commit references for nonlocal workflow actions, explicit least permissions, checkout credential persistence disabled, no workflow expression embedded in `run`, and no untrusted event/ref/title/body expression passed through shell-reaching environment values. Permit `id-token: write` and `attestations: write` only together on the exact owner-authorized release binary-provenance job, alongside `contents: read`, explicit file subjects, full-commit-pinned `actions/attest`, and no artifact-storage, package, or other write permission; reject either permission in every other workflow/job. Classify intrinsic identity/index operations, protocol structure, parsed fixture/version grammar, compiler-required syntax, dependency schema-version declarations, and independent fixture data structurally rather than by line suppression.
4. Produce deterministic diagnostics ordered by repository path, line, rule, and declared symbol, and fail the command if any violation exists.
5. Extend the exhaustive development-binary dispatcher with `source-policy`, preserving the existing metadata and `dependency-direction` branches and rejecting unknown commands.

**Tests:**

- Rust, executable-script, workflow, and Structured Query Language files at 1,000 physical lines pass and adjacent 1,001-line fixtures fail.
- Single-character names and each maintained abbreviation fail in applicable locally declared Rust, script, and migration identifiers, while dependency symbols, standardized workflow keys, external inputs, command/wire strings, and required SQLite keywords remain accepted.
- Exact `::core::fmt::Display::fmt` and tabled leading-`::` Serde visitor item names pass only in their literal fully qualified external trait implementations with matching locked dependency identity. An alias, re-export, inferred short path, local lookalike, inherent method, project-owned trait method, or abbreviated project-controlled local inside an accepted external implementation fails; no free-standing spelling allowlist or compiler-HIR inference can make either case pass.
- Numeric values carrying domain, size, duration, retry, capacity, exit, or operational meaning pass only through named Rust constants, readonly script values, or named migration policy/schema declarations and fail inline otherwise. Intrinsic zero/one identity/index operations, protocol structure, parsed fixture/version grammar, and independent fixture values pass their structural fixtures without a meaningless constant.
- Rust/script functions and migration triggers/conditional expressions at complexity 10 pass; computed complexity 11 fails with the exact shared-policy diagnostic.
- `todo!`, `unimplemented!`, placeholder panics, exact configured task markers/placeholders, and planning-only headings fail in product scope.
- Exported interfaces without nonempty documentation fail. Separate fixtures reject unsafe blocks, functions, foreign-ABI functions, traits, implementations, foreign blocks, and unsafe attributes at their exact source locations; comments and string literals containing the word remain accepted, and no lint attribute or documentation section can exempt unsafe syntax. Fixtures also prove that the checker does not pretend to classify semantic completeness, historical framing, or narrating comments; those cases appear in the separately asserted review-checklist inventory.
- Planning documents containing prospective language are outside the product scan, while exact configured forbidden tokens/headings in root-product prose fail.
- A local action and a full-commit-pinned nonlocal action with explicit read-only permissions and checkout credential persistence disabled pass. The exact release binary-provenance fixture passes only with `contents: read`, `id-token: write`, `attestations: write`, explicit file subjects, and artifact-storage disabled; either write permission outside that job, `artifact-metadata`, `packages`, or any additional write permission, a tag/branch/short digest, omitted permission, persisted checkout credential, direct shell expression, or untrusted shell-reaching environment expression fails with the exact workflow location.

- **Done when:** `cargo test -p slingshot-development --test source_policy` proves workspace-wide unsafe-syntax rejection plus every other falsifiable Rust/script/workflow/migration/documentation acceptance and rejection boundary at 1,000 lines and complexity 10, proves the semantic documentation/comment checklist is review-only, and the source-policy command reports no repository violation using the single policy data source.
