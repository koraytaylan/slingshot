---
id: add-component
title: "Add a Component"
workstream: "0012"
kind: task
depends_on:
  - command-module-scaffold
  - repository-path
  - property-values
gated: false
touches:
  - crates/slingshot-domain/src/command/add_component.rs
  - crates/slingshot-domain/tests/fixtures/commands/add_component/**
  - crates/slingshot-domain/tests/add_component.rs
status: done
merged_as: "0617a202596e3e1f0ebde114f0f2b9dc5e70b19a"
---
# Add a Component

Represent adding one named component under a page-relative parent resource with an explicit resource type and typed properties.

**Steps:**

1. Commit canonical command/result fixtures and language-neutral agent conformance scenarios for content-root-parent, nested-parent, exact computed-path, duplicate `jcr:content` prefix, property-rich, forbidden `sling:resourceType` property, invalid page/relative path/name/resource type, duplicate property, orderable/non-orderable parent, append-last order, existing-resource, property/order/save failure, NotStarted/InFlight/Committed restart, matching/missing/mismatched/unreadable operation receipt, later target edit/reorder/move/deletion, accepted-result replay, no-effect conflict, and outcome-unknown cases before implementation.
2. Implement PageContentParent as ContentRoot or Descendant(RepositoryRelativePath); descendant paths use address segments including valid same-name-sibling suffixes, reject every absolute/traversal/malformed lexical form, and reject any segment whose RepositoryName is `jcr:content` regardless of sibling suffix.
3. Implement ComponentName as its own creatable-child wrapper using the same lexical subset as but not the type PageName; reuse the separate bounded ComponentResourceType grammar.
4. Implement AddComponentCommand with a page RepositoryPath identifying `cq:Page`, PageContentParent relative to its `jcr:content`, component name, component resource type, and ordered mutation PropertyValue properties; reject `sling:resourceType` in the property map.
5. Compute `page_path/jcr:content[/descendant]/component_name`; before InFlight or mutation, resolve the exact parent and require its effective primary node type to report orderable child nodes. Return authoritative-no-effect `parent_not_orderable` when it does not; never infer or promise append order for a non-orderable parent. Only then atomically create the resource, set resource type/properties, append it last in deterministic child order, and save it plus an agent-private operation receipt carrying semantic-version, argument, target/effect digests, and bounded canonical success-result bytes in the same JCR transaction.
6. Implement `page_not_found`, `page_invalid`, `parent_not_found`, `parent_access_denied`, `parent_not_orderable`, `target_already_exists`, `property_rejected`, and `repository_commit_failed` as the eight closed computed-target failures emitted only with authoritative no-content-or-order-effect evidence. Add closed `mutation_outcome_unknown` for an InFlight target without a matching receipt or an unreadable/conflicting receipt; it makes no no-effect assertion and forbids automatic replay.
7. Pin NotStarted/InFlight/Committed execution detail. Persist InFlight before the save and Committed after it. On physical retry/restart, a readable operation/argument/effect-matching receipt is authoritative commit proof and replays its validated bounded canonical success result without mutation even when the current target was later edited, reordered, moved, or deleted. Absence of both receipt and target after InFlight proves no commit and permits one retry; a preflight target before InFlight is conflict; a target without a matching receipt after InFlight or an unreadable/conflicting receipt is outcome unknown. Never compare current target content/order to overturn a matching receipt. `repository_commit_failed` requires post-failure proof that receipt and target are both absent.
8. Supply request-context validation that recomputes the page-content-relative target and rejects every success/failure carrying another target before persistence.

**Tests:**

- PageContentParent accepts ContentRoot, while Descendant refuses every absolute, traversal-shaped, and duplicate-`jcr:content` fixture.
- Component name and resource type boundaries are asserted independently.
- Page paths reuse RepositoryPath validation.
- Properties preserve order and type while rejecting duplicates and excess count.
- Redacted observed values are invalid mutation input; omitted properties are not cleared.
- Independently authored scenarios prove the parent orderability check occurs before InFlight/mutation; a non-orderable parent yields only `parent_not_orderable` with no target, receipt, content, or ordering effect. Orderable parents require exact computed path, append-last order, no overwrite/deletion, atomic target-plus-receipt commit, bounded result/effect evidence, and authoritative committed/no-effect/unknown recovery.
- Result path, disposition, canonical round trip, and unknown-field rejection are asserted.
- Every registered failure literal/target path round-trips exactly; the eight authoritative categories, including `parent_not_orderable`, assert no content or ordering effect, `mutation_outcome_unknown` explicitly does not, and every unknown or surplus failure field is rejected.
- Physical retry vectors replay a matching receipt after current-target edit/reorder/move/deletion, never repeat a committed mutation, retry once only after proof of no receipt/target, and fail closed on a target without a matching receipt or an unreadable/conflicting receipt.
- A structurally valid result copied from another add-component command fails request-context target validation.

- **Done when:** cargo test -p slingshot-domain --test add_component validates parent orderability before mutation, all eight authoritative-no-effect failures, name/resource-type/property/result invariants, and the complete pinned page-content-relative append-last atomic-creation agent-conformance inventory without claiming Rust mutates a repository.
