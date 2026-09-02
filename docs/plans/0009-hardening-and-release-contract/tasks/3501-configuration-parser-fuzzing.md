---
id: configuration-parser-fuzzing
title: "Configuration Parser Fuzzing"
workstream: "0035"
kind: task
depends_on:
  - pinned-coverage-fuzzing-tool
gated: false
touches:
  - fuzz/Cargo.toml
  - fuzz/Cargo.lock
  - fuzz/fuzz_targets/configuration_document.rs
  - "fuzz/corpus/configuration_document/**"
  - scripts/run_fuzz_target
  - crates/slingshot-development/tests/configuration_fuzz_corpus.rs
status: done
merged_as: "80c8d5349287dbd581411ac5b31d4787dc48d4a3"
---
# Configuration Parser Fuzzing

Configuration and credential documents are the first untrusted bytes most runs consume. This task creates the fuzz side workspace and drives those bytes through the bounded production parsers without granting generated paths filesystem authority.

**Steps:**

1. Commit a seed corpus first containing valid and invalid profile, selection, and `configuration-snapshot.toml` documents; sorted/unsorted/duplicate/extra/missing source references; lowercase/malformed source digests; exact/next `MAXIMUM_CONFIGURATION_SOURCE_DOCUMENT_BYTES = 1,048,576` and `MAXIMUM_CONFIGURATION_GENERATION_SOURCE_BYTES = 16,777,216`; first/second per-file and first/second whole-generation attempt boundaries; parser-independent role-unknown documents and role-tagged inspector output; same-role deduplication, cross-role and fixed snapshot/selection/profile location collision; S2 change before selected parsing; Cloud service-credential JavaScript Object Notation; selected versus unselected JSON/PEM documents; duplicate keys; 31/32/33 distinct public-diagnostic candidates; `SensitiveConfigurationDocument` and `SecretValue` lifecycle canaries; boundary sizes; Unicode names; normalized base-address prefixes; ambiguous trailing paths; dot segments and encoded separators; colon-bearing Basic usernames; credential and additional certificate-authority paths; sampled UTC below/at/above `0..=253,402,300,799` and maximum `exp = 253,402,329,599`; relative `expires_in` millisecond values; secret sentinels; and truncations.
2. Create the isolated fuzz workspace against the exact dated nightly and pinned coverage-tool contract owned by `pinned-coverage-fuzzing-tool`. Add an argument-checked `scripts/run_fuzz_target` wrapper that requires `SLINGSHOT_COVERAGE_FUZZING_TOOL_BUNDLE=<verified-bundle>`, invokes only the manifest-resolved absolute executable after offline verification, and checks its source/binary/nightly identities before named defaults bound ordinary regression time; a named environment override enables deeper runs without selecting another toolchain or executable.
3. Implement the configuration target over the production stable-read/generation coordinator, profile/selection inventory inspector, selected JSON/PEM parsers, diagnostic mapper, sensitive containers, and deterministic credential-filesystem policy fake; never open a generated path on the host filesystem. Require exactly two named per-file attempts and two named whole-generation attempts. The coordinator, not a semantic parser, verifies generic source/private-digest/aggregate limits and role inventory, and it cannot invoke selected JSON/PEM parsing until S2 equals S1.
4. Add an ordinary integration test that runs every seed and retained crashing input on all supported platforms.
5. Assert bounded allocation, checked single-sample UTC/assertion and receipt-clock expiry conversion, deterministic acceptance/error category, canonical successful output, exact role-tagged discovered/transitively referenced inventory equality, cross-role refusal, S2-before-selected-parser ordering, and absence of secret input, source reference, source-derived ordering, private source digest, unknown key/value, parser excerpt, or dependency cause from diagnostics. Pin the inclusive 32-item rule: 31 and 32 distinct tuples have no marker; 33 yields the first 31 plus one fixed `configuration_diagnostics_truncated` marker with occurrence count two.

**Tests:**

- Every committed seed completes without panic or host filesystem access.
- The wrapper refuses an absent/unverified bundle, PATH/global executable, wrong source/tree/lock/binary/host identity, or unpinned nightly before compiling a target; it succeeds with an empty PATH and network denied when given the matching verified bundle.
- Repeating one input yields the same result and canonical output bytes.
- The 1,048,576-byte generic source and 16,777,216-byte aggregate boundaries succeed where class-specific limits permit; the next byte/source fails before retention or proportional allocation. Same-role reuse deduplicates, while cross-role/fixed-location reuse fails generation consistency.
- Exhausting exactly two per-file attempts returns only `configuration_file_changed_during_read`; exhausting exactly two complete S1/source-inventory/S2 attempts returns only `configuration_snapshot_inconsistent`. Selected credential JSON and certificate PEM parsers observe only role-tagged `SensitiveConfigurationDocument` after equal S2; unselected opaque documents are never semantically parsed.
- Source documents use only temporary redacted, nonserializable `SensitiveConfigurationDocument`; extracted long-lived password/private-key/client-secret/assertion/token bytes alone use redacted `SecretValue`. Owned mutable buffers are disposed under their exact scope without claiming dependency or operating-system zeroization.
- Sampled UTC below zero or above 253,402,300,799 fails before flooring/signing; accepted maximum plus 28,800 seconds yields exactly 253,402,329,599 without overflow. Zero, negative, overflowing, and out-of-policy relative token lifetimes fail without consulting a clock twice.
- Public 31/32/33 diagnostic vectors enforce the inclusive marker and contain no source reference or ordering oracle. Sentinel credential fragments never appear in errors, tracing, or retained crash names.

- **Done when:** `cargo test -p slingshot-development --test configuration_fuzz_corpus` and `SLINGSHOT_COVERAGE_FUZZING_TOOL_BUNDLE=<verified-bundle> scripts/run_fuzz_target configuration_document` pass the committed corpus with an offline manifest-resolved pinned executable, exact two-attempt/generation and source/aggregate bounds, parser-independent role/S2 ordering, inclusive source-free diagnostics, sensitive-document/secret lifecycles, exact UTC/exp bounds, deterministic outcomes, and no secret echo, and `scripts/quality` succeeds.
