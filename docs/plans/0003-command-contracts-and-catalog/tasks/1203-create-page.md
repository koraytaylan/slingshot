---
id: create-page
title: "Create a Page"
workstream: "0012"
kind: task
depends_on:
  - command-module-scaffold
  - repository-path
  - property-values
gated: false
touches:
  - crates/slingshot-domain/src/command/create_page.rs
  - crates/slingshot-domain/tests/fixtures/commands/create_page/**
  - crates/slingshot-domain/tests/create_page.rs
status: done
merged_as: "0d5062d317baca6c5775abdea00143dff852e8fb"
---
# Create a Page

Represent creation of one named page below a parent path from a template with a title and typed initial properties.

**Steps:**

1. Commit canonical command/result fixtures and language-neutral agent conformance scenarios for minimal, property-rich, Unicode-title, exact `jcr:content` target, forbidden `jcr:title` property, invalid-name/path, duplicate-property, existing-page, template/property/save failure, NotStarted/InFlight/Committed restart, matching/missing/mismatched/unreadable operation receipt, later target edit/move/deletion, accepted-result replay, no-effect conflict, and outcome-unknown cases before implementation.
2. Implement PageName as its own bounded creatable-child wrapper over one unqualified RepositoryName, rejecting namespace colon, same-name-sibling syntax, reserved punctuation, non-NFC, and every path/property/resource-type role.
3. Implement PageTitle as bounded non-empty text and CreatePageCommand with parent path, page name, title, template path, and ordered initial mutation PropertyValue properties.
4. Reject duplicate property names and a `jcr:title` override and bound the property count.
5. Define exact atomic creation semantics: create one `cq:Page` at `parent/page_name` from the template; apply title and supplied initial properties only to its `jcr:content`; preserve omitted template properties; and atomically save that content plus an agent-private operation receipt carrying semantic-version, argument, target/effect digests, and bounded canonical success-result bytes in the same JCR transaction.
6. Implement `target_already_exists`, `parent_not_found`, `parent_access_denied`, `template_not_found`, `template_invalid`, `property_rejected`, and `repository_commit_failed` as closed computed-target failures emitted only with authoritative no-effect evidence. Add closed `mutation_outcome_unknown` with computed target for an InFlight target without a matching receipt or an unreadable/conflicting receipt; it makes no no-effect assertion and forbids automatic replay.
7. Pin NotStarted/InFlight/Committed execution detail. Persist InFlight before the save and Committed after it. On physical retry/restart, a readable operation/argument/effect-matching receipt is authoritative commit proof and replays its validated bounded canonical success result without mutation even when the current target was later changed, moved, or deleted. Absence of both receipt and target after InFlight proves no commit and permits one retry; a preflight existing target before InFlight is conflict; a target without a matching receipt after InFlight or an unreadable/conflicting receipt is outcome unknown. Never compare current target content to overturn a matching receipt. `repository_commit_failed` requires post-failure proof that receipt and target are both absent.
8. Supply request-context validation that recomputes `parent/page_name` and rejects every success/failure carrying another target before persistence.

**Tests:**

- Valid and invalid page names exercise every segment rule.
- Parent and template validation report distinct fields.
- Empty, over-bound, and Unicode titles behave according to fixtures.
- Initial properties preserve ordering and typed values while rejecting duplicates and excess count.
- Redacted observed values are invalid mutation input; omitted properties remain governed by the template and are not cleared.
- Independently authored scenarios require exact created/content-property targets, no overwrite, atomic target-plus-receipt commit, bounded result/effect evidence, and authoritative committed/no-effect/unknown recovery; Rust validates their bounded command/result/checkpoint shapes without mutating a repository or executing the external Java handler.
- Result paths and disposition values reject impossible shapes and unknown fields.
- Every registered failure literal/target path round-trips exactly; the seven authoritative categories assert no effect, `mutation_outcome_unknown` explicitly does not, and every unknown or surplus failure field is rejected.
- Physical retry vectors replay a matching receipt after current-target edit/move/deletion, never repeat a committed mutation, retry once only after proof of no receipt/target, and fail closed on a target without a matching receipt or an unreadable/conflicting receipt.
- A structurally valid result copied from another page command fails request-context target validation.

- **Done when:** cargo test -p slingshot-domain --test create_page validates name/title/path/property/result invariants and the complete pinned `cq:Page`/`jcr:content` atomic-creation agent-conformance inventory without claiming repository mutation.
