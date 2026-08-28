---
id: repository-path
title: "Repository Path"
workstream: "0009"
kind: task
depends_on:
  - command-module-scaffold
gated: false
touches:
  - crates/slingshot-domain/src/command/repository_path.rs
  - crates/slingshot-domain/src/command/component_resource_type.rs
  - crates/slingshot-domain/tests/fixtures/commands/repository-path.jsonl
  - crates/slingshot-domain/tests/repository_path.rs
status: planned
merged_as: ""
---
# Repository Path

Every command addresses repository content, so one validated path type must reject ambiguous or traversal-shaped input before any transport sees it.

**Steps:**

1. Commit language-neutral accepted/rejected vectors for RepositoryName, RepositoryPathSegment, absolute RepositoryPath, RepositoryRelativePath, RepositoryPropertyPath, and distinct ComponentResourceType, including Unicode 15.1 NFC/non-NFC, valid/malformed/multiple namespace colons, reserved punctuation/wildcard/pipe, root, omitted-first-sibling suffix, rejected `[1]`, same-name-sibling zero/leading-zero/maximum/overflow, repeated/trailing slash, dot/parent, controls, absolute/relative resource types, and every named byte/segment bound.
2. Implement the exact closed architecture grammar with distinct name, segment, absolute, relative, and property-path wrappers; require already-NFC input, represent the first same-name sibling without a suffix, and accept a suffix only for canonical indexes from two through the named maximum. Implement ComponentResourceType in its separate module under its Sling segment grammar without JCR namespace or same-name-sibling interpretation.
3. Provide explicit parent, address-child, and creatable-child operations that return validated role-specific values and never concatenate unchecked text.
4. Preserve valid input spelling exactly, serialize each path value as one JSON string, and reject cross-role construction rather than coercing it.
5. Implement Display without logging or diagnostic decoration so presentation layers decide how paths appear.

**Tests:**

- Every accepted fixture constructs and round-trips through canonical JSON.
- Every rejected fixture returns the expected error kind and field.
- Parent/address-child/creatable-child operations preserve root behavior and refuse the wrong segment/name role.
- A generated path corpus proves successful values obey NFC, namespace, reserved-character, same-name-sibling, traversal, empty-segment, control, and trailing-slash rules.
- Boundary cases immediately below, at, and above the named byte limit are asserted.

- **Done when:** cargo test -p slingshot-domain --test repository_path passes the complete language-neutral name/path/property-path/same-name-sibling/resource-type corpus and generated role invariants, and no public constructor can create an invalid or cross-role value.
