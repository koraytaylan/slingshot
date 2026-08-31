---
id: content-fragment-elements
title: "Content Fragment Element Values"
workstream: "0043"
kind: task
depends_on:
  - operational-contract-limits
gated: false
touches:
  - crates/slingshot-domain/src/command/content_fragment_element.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/content_fragment_element.rs
  - "crates/slingshot-domain/tests/fixtures/commands/content_fragment_element/**"
status: done
merged_as: "37b2e8e9db8503286b3ba6df14ef0aeeb0015c67"
---
# Content Fragment Element Values

Three of the four fragment commands carry the same thing: a set of element names, each holding either one text value or an ordered list of them. Landing it once means the create and the update cannot disagree about what an element is.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `ContentFragmentElementName` as a bounded non-empty name at `MAXIMUM_CONTENT_FRAGMENT_ELEMENT_NAME_BYTES`, refusing controls, edge spaces, and a solidus.
3. Implement `ContentFragmentElementValue` as a closed single text value or an ordered list of at most `MAXIMUM_CONTENT_FRAGMENT_ELEMENT_VALUES` text values, each bounded by `MAXIMUM_PROPERTY_STRING_BYTES`.
4. Implement `ContentFragmentElementValues` as an ascending set of at most `MAXIMUM_CONTENT_FRAGMENT_ELEMENTS` element names, refusing a repeat and refusing an unordered set rather than sorting it.
5. Implement `ContentFragmentVariationName` as a bounded non-empty name at `MAXIMUM_CONTENT_FRAGMENT_VARIATION_NAME_BYTES`; an absent variation means the master variation, and that is stated where the value is declared rather than in each command.

**Tests:**

- Each bound is accepted exactly and refused one past it, for the element name, the value, the value list, and the element set.
- A repeated element name and a descending set are both refused.
- Both value forms round-trip byte-identically and neither accepts the other's members.
- An empty value list is refused, and a single value is never rewritten as a one-item list.
- The variation name is proved at its bound on both sides.

- **Done when:** `cargo test -p slingshot-domain --test content_fragment_element` proves every bound on both sides, the ascending distinct element set, and both closed value forms.
