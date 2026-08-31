---
id: authorizable-identity
title: "Authorizable Identity"
workstream: "0040"
kind: task
depends_on:
  - operational-contract-limits
gated: false
touches:
  - crates/slingshot-domain/src/command/authorizable_identity.rs
  - crates/slingshot-domain/src/command/repository_path.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/authorizable_identity.rs
status: planned
merged_as: ""
---
# Authorizable Identity

Eight commands address a user or a group, and none of them can borrow a repository path to do it: an authorizable is addressed by the identifier it was created under, and where it lives in the repository is the author's answer rather than the caller's. This task lands that identifier, the kind it belongs to, and the intermediate path a creation may ask for.

**Steps:**

1. Make the validated-address shape the repository paths already share reachable from the rest of the family, and route its own constructions through the one constructor that shape now declares, rather than copying the shape into this leaf and into each of the three that follow it.
2. Implement `AuthorizableIdentifier` as a validated wrapper: non-empty, at most `MAXIMUM_AUTHORIZABLE_IDENTIFIER_BYTES`, already in normalization form C, and refusing a solidus, any control, a leading or trailing space, and the reserved single and double full-stop forms.
3. Implement `AuthorizableKind` as the closed `User` or `Group`, serialized in snake case.
4. Implement `AuthorizableIntermediatePath` as a bounded relative path of validated repository names joined by a solidus, at most `MAXIMUM_AUTHORIZABLE_INTERMEDIATE_PATH_BYTES`, refusing an absolute form, an empty interior segment, a trailing solidus, and every traversal form the repository path grammar refuses.
5. Give each failure a variant that names the invalid field and the violated bound and never echoes the whole value.

**Tests:**

- The identifier accepts ordinary spellings and refuses empty, oversized by one byte, non-normalized, solidus-bearing, control-bearing, space-edged, and reserved forms.
- The exact byte bound is accepted and the next byte is refused, for both the identifier and the intermediate path.
- The kind round-trips through canonical JSON as `user` and `group` and refuses any other spelling.
- An intermediate path with an absolute prefix, an empty segment, a trailing separator, or a traversal segment is refused with the field named.
- Every error message names the field and the bound and contains no part of an oversized value.

- **Done when:** `cargo test -p slingshot-domain --test authorizable_identity` proves both bounds at and one past the limit, the closed kind, and every refused form, with no command depending on it yet.
