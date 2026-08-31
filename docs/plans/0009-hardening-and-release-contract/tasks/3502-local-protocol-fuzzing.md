---
id: local-protocol-fuzzing
title: "Local Protocol Fuzzing"
workstream: "0035"
kind: task
depends_on:
  - configuration-parser-fuzzing
gated: false
touches:
  - fuzz/Cargo.toml
  - fuzz/fuzz_targets/local_protocol_frame.rs
  - "fuzz/corpus/local_protocol_frame/**"
  - crates/slingshot-development/tests/local_protocol_fuzz_corpus.rs
status: done
merged_as: ""
---
# Local Protocol Fuzzing

The target-scoped endpoint accepts arbitrary same-user bytes, including concatenated and fragmented frames. This task fuzzes the exact codec and envelope path before daemon dispatch.

**Steps:**

1. Commit seeds first for every retained control and versioned operation request and response; exact five-field command identity of wire name, semantic version `1.0.0`, canonical `slingshot.command-contract-limits/1` digest, argument-schema digest, and result-schema digest plus separate canonical `slingshot.command-canonical-json/1` artifact/digest/dual annotations; missing/stale/extra/role-swapped/one-bit-different provenance; independent canonical-contract/limits/role-schema drift; exact `1.0.0+01` accepted and `1.0.0-01` rejected version vectors; canonical and noncanonical SearchPhrase, FindAssets set, exact `AssetByteLength` zero/maximum/next/negative/fraction/exponent/nonminimal/overflow and inverted-range cases, and opaque continuation inputs; every exact registered revised semantic failure; expected target revisions; both `RecoveryExecutionEvidence` variants; every legal terminal kind/disposition pair including `ResultUnavailable`/`AuthoritativeRemoteSuccess`; illegal missing/duplicate/cross-branch certainty shapes; inline and `structured_result_artifact_access` command dispositions; inline maintenance values and operation-free `maintenance_result_access` descriptors; `MaintenanceResultMetadata` requests/responses keyed only by target and identifier with exact metadata or closed unreadable refusal; `MaintenanceResultRead` requests/responses with exact target/identifier/expected-digest-from-metadata/offset/chunk fields and no operation or artifact slot; artifact-chunk offsets/byte lengths/encoded bytes; list cursors; boundary frames; duplicate fields; unsupported versions; concatenation; fragmentation descriptors; invalid Unicode; truncation; and oversized prefixes.
2. Register the `local_protocol_frame` target in the fuzz workspace and pass input through the production framing and envelope parsers.
3. For accepted input, canonicalize, parse again, and require semantic identity; for rejected input, require a bounded stable error class.
4. Track requested allocation and consumed bytes so the target asserts frame and manifest-owned command limits before daemon or storage code can run; route accepted inputs through a sentinel dispatcher that separately records control and operation access. Reserve operation dispatch only after the exact five-field command identity and separate canonical-contract artifact/digest/dual-annotation provenance validate, raw bytes and typed array order pass `slingshot.command-canonical-json/1`, the decoded tree passes its Draft 2020-12 role schema, and typed/cross-field construction, closed result/failure shape, and applicable inline-versus-`structured_result_artifact_access` disposition validate in that order.
5. Add the seed-corpus integration test for normal continuous integration.

**Tests:**

- Arbitrary seeds and retained findings never panic, over-allocate, or reach a dispatch sentinel after rejection.
- Accepted frames are canonicalization-idempotent; recovery-required frames carry exactly one legal evidence variant, and terminal result-unavailable/authoritative-remote-success frames carry no certainty while every illegal pairing is rejected.
- Concatenated input consumes exactly one declared frame at a time.
- Fragmentation at every byte boundary yields the same result as one complete read.
- Accepted operation-artifact and maintenance-result chunks stay within decoded-chunk and encoded-frame limits and round-trip exact bytes. Maintenance metadata returns the authenticated expected digest before a read, and missing, superseded, retired, or mismatched associations return one closed unreadable refusal without a start. Maintenance-result frames preserve their distinct start/chunk/end variants and never acquire an operation identifier or artifact slot.
- Metadata/read sequences accept unchanged identity-bound metadata and the sole current-preview-to-application-receipt owner/revision transition before read start; reject every other owner/revision/content transition; and preserve an already acquired same handle when the exact ownership transfer occurs after read start.
- Any missing or unequal command-identity field or separate canonical-contract provenance role, unverified schema bytes, noncanonical phrase/set/asset-length spelling, invalid or inverted asset range, decoded or rewritten token, alias/surplus/missing failure field, cross-command failure, or over-bound inline result reaches neither operation nor storage dispatch. Maximum registered failure envelopes remain inline; the first over-inline canonical command success is accepted only through the daemon-created `structured_result_artifact_access` disposition for its `structured_result` slot with exact digest and bytes.
- Unsupported operation versions and target mismatches never dispatch an operation, while every retained hello, status, ping, and stop fixture remains usable.

- **Done when:** `cargo test -p slingshot-development --test local_protocol_fuzz_corpus` and `scripts/run_fuzz_target local_protocol_frame` prove bounded parsing, exact consumption, pre-dispatch five-field identity plus separate canonical-contract provenance, independent provenance-role drift, raw-canonical-before-schema-before-typed enforcement, exact `+01`/`-01` version outcomes, lossless closed failures, shared inline/externalized result disposition, canonical conditional recovery/terminal evidence, and round trips for the committed corpus, and `scripts/quality` succeeds.
