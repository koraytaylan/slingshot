---
id: search-predicates
title: "Search Predicates"
workstream: "0009"
kind: task
depends_on:
  - command-module-scaffold
  - property-values
gated: false
touches:
  - crates/slingshot-domain/src/command/search_predicate.rs
  - crates/slingshot-domain/tests/fixtures/commands/search-predicates.jsonl
  - crates/slingshot-domain/tests/search_predicates.rs
status: done
merged_as: ""
---
# Search Predicates

Path, page, and asset searches need a structured predicate language whose meaning is explicit without accepting raw repository query text.

**Steps:**

1. Commit fixtures for direct and nested RelativePropertyPath plus Exists, Equals, NotEquals, ScalarIn, ListContainsAny, ListContainsAll, LessThan, LessThanOrEqual, GreaterThan, and GreaterThanOrEqual before implementation.
2. Implement RelativePropertyPath as zero or more RepositoryPathSegment child addresses plus one final PropertyName, rejecting absolute/cross-role, traversal, malformed namespace/SNS/property, reserved punctuation, non-NFC, and over-bound forms.
3. Implement PropertyPredicate with RelativePropertyPath and exact operator fields, plus scalar-only OrderedScalarPropertyValue for comparisons.
4. Validate shapes: Exists has no value; equality has one PropertyValue; membership has nonempty ordered unique same-discriminator PropertyScalarValue values; ordered comparison has one ordered scalar; no type is inferred from a JSON token shape. Use exact spelling equality for String/RepositoryPropertyPath, value equality for Boolean, mathematical equality for Integer/Decimal, instant equality for DateTime, and elementwise repository order for lists.
5. Bound path segments/bytes, predicate count, and membership count with named constants.
6. Serialize the exact architecture literals `exists`, `equals`, `not_equals`, `scalar_in`, `list_contains_any`, `list_contains_all`, `less_than`, `less_than_or_equal`, `greater_than`, and `greater_than_or_equal`, using only `operator`, `property_path`, and the operator-specific `value` or `values` field.

**Tests:**

- Each operator has an exact canonical fixture and round trip.
- Missing, surplus, and operator-incompatible fields are rejected.
- Empty, mixed-discriminator, and over-bound predicate/membership collections are rejected.
- Integer and Decimal equality/order are mathematical without precision loss, including scale-distinct Decimal spellings; DateTime equality/order uses instants; String order uses Unicode scalar values; and unlike discriminators never compare.
- RepositoryPropertyPath and list values are rejected for ordered comparisons while ordered lists retain exact element order under Equals and NotEquals.
- Nested relative paths resolve exact child segments from each candidate node; mutation PropertyName does not deserialize as a query path and predicates perform no descendant/name fallback.
- Independently authored canonical fixtures pin every literal and field; unknown operator, alternate spelling, and missing/surplus/operator-incompatible fields fail.
- Arbitrary strings resembling JCR-SQL2 remain ordinary values and never become executable query syntax.

- **Done when:** cargo test -p slingshot-domain --test search_predicates passes all relative-property resolution, exact operator-shape, homogeneous typed comparison/membership, bound, canonicalization, and raw-query noninterpretation cases.
