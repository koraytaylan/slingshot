---
id: property-values
title: "Property Values"
workstream: "0009"
kind: task
depends_on:
  - command-module-scaffold
  - repository-path
gated: false
touches:
  - crates/slingshot-domain/src/command/property_value.rs
  - crates/slingshot-domain/tests/fixtures/commands/property-values.jsonl
  - crates/slingshot-domain/tests/property_values.rs
status: done
merged_as: ""
---
# Property Values

Repository predicates and authoring commands need one lossless closed JCR value model that cannot be confused with the distinct Open Service Gateway Initiative observation model.

**Steps:**

1. Commit valid/invalid fixtures for qualified/unqualified direct PropertyName and cross-role/SNS rejection, each PropertyScalarValue/JCR mapping including absolute/relative RepositoryPropertyPath, scalar PropertyValue, and homogeneous nonempty multi-value PropertyValue.
2. Implement PropertyName as one RepositoryName without same-name-sibling syntax, retaining its independent direct-property role and named bound.
3. Implement PropertyScalarValue with exact JCR String, Boolean, signed 64-bit Long, scale-preserving plain Decimal, millisecond-precision UTC Date, and Path mappings; implement PropertyValue as Scalar or nonempty same-discriminator ordered Scalars.
4. Pin the exact architecture grammars: minimal Integer; Decimal without plus/exponent/leading integer zero/negative numeric zero while preserving fractional presence and trailing-zero scale; Date with `Z`, required seconds, absent zero milliseconds or exactly three nonzero-millisecond digits. Bound scalar strings, decimal integer/fraction/total bytes, and multi-value count through named constants.
5. Serialize the exact architecture grammar: scalar `type`/`value`; PropertyValue `cardinality` equal to `single` with `value` or `multiple` with homogeneous `values`. Export no observation/redaction type from this JCR module; task 1002 owns the independent Open Service Gateway Initiative value and observation grammar.

**Tests:**

- Every scalar and homogeneous multi-value variant has exact canonical request and JCR mapping fixtures.
- Signed 64-bit Long limits/minimal spelling, Decimal plus/zero/leading-zero/fraction/trailing-zero/exponent boundaries, Date calendar/year/offset/leap-second/zero-and-nonzero-millisecond canonical forms, and list count are asserted on both sides of every boundary.
- RepositoryPropertyPath values reuse the exact absolute-or-relative path-property grammar and remain distinct from command-address RepositoryPath.
- Deserialization refuses unknown discriminators, unknown fields, and invalid nested values.
- Serialization round trips preserve list ordering, Decimal scale/trailing zeros, and canonical Date spelling; integer and Decimal comparison vectors distinguish lexical representation from their pinned mathematical equality.
- Mutation PropertyValue cannot represent redaction, Open Service Gateway Initiative carriers/types, or metatype evidence.
- Empty/nested/mixed lists, null, and deletion intent are unrepresentable and rejected during deserialization.
- Unknown discriminator, wrong literal spelling, and every missing/surplus/operator-incompatible field fail against independently authored canonical fixtures.

- **Done when:** cargo test -p slingshot-domain --test property_values passes exact minimal-Integer, scale-preserving-Decimal, millisecond-Date, scalar-to-JCR mapping, homogeneous multi-value, direct-name, JCR/OSGi model separation, boundary, null/deletion refusal, and lossless round-trip inventory.
