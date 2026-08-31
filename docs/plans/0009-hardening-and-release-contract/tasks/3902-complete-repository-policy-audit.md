---
id: complete-repository-policy-audit
title: "Complete Repository Policy Audit"
workstream: "0039"
kind: chore
depends_on:
  - release-artifact-contract
  - live-adobe-experience-manager-harness
gated: false
touches:
  - policy/abbreviated-identifiers.txt
  - policy/documentation-rules.toml
  - policy/source-policy.toml
  - docs/DOCUMENTATION_REVIEW.md
  - scripts/prepare_locked_source_cache
  - crates/slingshot-development/src/lib.rs
  - crates/slingshot-development/src/script_policy.rs
  - crates/slingshot-development/src/source_policy.rs
  - crates/slingshot-development/src/workflow_policy.rs
  - crates/slingshot-development/tests/source_policy.rs
  - crates/slingshot-development/tests/fixtures/workspace-module-map/module-ownership.txt
  - "crates/slingshot-development/tests/fixtures/source-policy/**"
status: done
merged_as: "5232f2ba08500dbeb1dafd90e58e502df76d8461"
---
# Complete Repository Policy Audit

Plan 0001 establishes policy on the walking skeleton. This task expands its corpus to every declaration and document shape introduced by the complete workspace and proves no later plan weakened the rules.

**Steps:**

1. Add accepted and rejected Rust, executable-script, and workflow fixtures first for command, configuration, authentication, daemon, agent transport, storage, command-line, Model Context Protocol, workflow, fuzz, and release code shapes.
2. Extend the maintained abbreviation table with every shortening discovered in locally declared Rust or script symbols, reject single-character names including generic parameters and explicit lifetimes, and keep standardized dependency identifiers, workflow keys, external action inputs, command names, and wire strings structurally outside the check.
3. Exercise the named-semantic-number rule across Rust and script asynchronous timing, protocol limits, status codes, retry schedules, collection bounds, and test iteration values, with accepted structural fixtures for identity elements, indices, version grammar, protocol syntax, and parsed external fixtures. Reject any adapter, fuzz harness, compatibility writer, or release command that redeclares a Plan 0003 wire-visible command limit/version/canonical-JSON/failure list instead of consuming the exact canonical limits, `slingshot.command-canonical-json/1`, role-schema registry, and separately tagged digest roles.
4. Exercise physical code-file length at and beyond 1,000 lines and cyclomatic complexity at and above the configured ceiling for each relevant Rust and script syntax shape.
5. Extend only the falsifiable documentation fixtures: configured placeholder/task tokens, planning-only headings outside `docs/plans/**`, per-line suppression markers, missing/nonempty exported-item documentation, and required structural forms. Keep semantic accuracy, completeness, present-versus-historical meaning, contract usefulness, and narrating-comment quality in the mandatory review checklist; fixtures prove the checker does not infer those properties.
6. Run the syntactic checker over the complete repository and split any violating source file or function instead of raising a limit. Complete the semantic documentation/comment review checklist separately, while automation validates only that the closed checklist inventory and required review record are present.

**Tests:**

- Every applicable locally declared Rust or script identifier uses the full-word vocabulary; dependency-owned, workflow-standardized, command, and serialized external spellings remain accepted only in their structural positions.
- Semantic domain and operational numeric expressions occur through named Rust or readonly script constants; intrinsic identity, index, protocol-structural, and parsed fixture/version literals remain idiomatic; every repository-owned code file is at most 1,000 physical lines.
- Every Rust or script function stays within the configured complexity ceiling and contains no placeholder body.
- Automation rejects only the configured falsifiable documentation tokens, headings, suppressions, absent exported-item documentation, and structural forms. Accepted/rejected fixtures prove it does not classify semantic completeness, accuracy, historical framing, or narrating comments.
- The mandatory review checklist names public contract/failure coverage, non-obvious invariant comments, non-narration, and present factual Rust/product prose once; its completion is review evidence rather than a source-policy inference. The complete repository produces no syntactic policy diagnostic.
- The complete command surface has one version/limits/canonical-JSON/schema/failure authority; CLI, Model Context Protocol, fake-agent, fuzz, compatibility, and release code contain no ad-hoc public constant, alias table, trim/normalization workaround, alternate validation order, or duplicate failure inventory.

- **Done when:** `cargo test -p slingshot-development --test source_policy` proves the expanded falsifiable corpus and review-checklist inventory, `cargo run --locked -p slingshot-development -- source-policy` reports zero syntactic violations across the complete repository while structurally excluding `docs/plans/**` from planning-heading rules, and the separate semantic documentation/comment checklist is completed without claiming automation judged its answers.
