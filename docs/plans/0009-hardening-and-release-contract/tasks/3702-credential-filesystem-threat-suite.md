---
id: credential-filesystem-threat-suite
title: "Credential Filesystem Threat Suite"
workstream: "0037"
kind: task
depends_on:
  - credential-exposure-threat-suite
gated: false
touches:
  - crates/slingshot-configuration/src/credential_filesystem.rs
  - crates/slingshot-configuration/src/configuration_generation.rs
  - crates/slingshot-configuration/src/testing/credential_filesystem.rs
  - crates/slingshot-development/tests/credential_filesystem_threats.rs
status: planned
merged_as: ""
---
# Credential Filesystem Threat Suite

Credential and additional certificate-authority path validation and reading must remain one contained operation even when directory entries change concurrently or platform path grammars disagree.

**Steps:**

1. Add deterministic schedules first for stable equality; in-place same-length and length-changing mutation before, during, and between reads and metadata samples; atomic replacement before and during an attempt; first-attempt instability followed by stability; named first/second per-file attempt and exhaustion; links at each component; ownership and permission change; special files; mixed separators; encoded traversal; exact/next 1,048,576-byte generic source and 16,777,216-byte aggregate generation; repeated open; and configuration-root containment for configuration snapshot, profile, selection, credential, and additional certificate-authority files. Include sorted/unsorted/duplicate inventories, private digest match/mismatch, role-unknown sources, exact role-tagged profile/selection/transitive inventory, same-role reuse, every cross-role and fixed snapshot/selection/profile collision, S1/source/S2 publication cuts, selected versus unselected JSON/PEM parser probes, old/new/mixed generations, named first/second whole-generation attempt and exhaustion, 31/32/33 public-diagnostic candidates, `SensitiveConfigurationDocument` lifetime probes, and a complete manifest-last replacement.
2. Drive every schedule through Plan 0002's exact safe-file/root policy. Each of exactly `MAXIMUM_CONFIGURATION_STABLE_READ_ATTEMPTS = 2` named per-file attempts performs one root-relative no-follow open to one verified final-file handle, captures pre identity/mutation metadata, reads bounded A from offset zero through the recorded exact length and end of file, captures middle metadata, rewinds the same handle, independently reads bounded B from offset zero through its newly recorded exact length and end of file, and captures post metadata. Acceptance requires byte-equal pre/middle/post platform evidence, A equal to B, both reads at their recorded lengths and end of file, and a still-valid same-handle policy. Any mismatch closes that attempt; second-attempt instability returns only `configuration_file_changed_during_read` and no bytes.
3. Exercise exactly `MAXIMUM_CONFIGURATION_GENERATION_ATTEMPTS = 2` named whole-generation attempts through the parser-independent coordinator. Each stable-reads/parses S1, applies `MAXIMUM_CONFIGURATION_SOURCE_DOCUMENT_BYTES = 1,048,576`, privately verifies every listed digest, checked-adds retained source lengths no higher than `MAXIMUM_CONFIGURATION_GENERATION_SOURCE_BYTES = 16,777,216`, and invokes a bounded inspector that parses only profile/optional-selection TOML and returns typed inspection plus exact role-tagged inventory. Require manifest/discovered/transitive role equality and class limits; same-role reuse deduplicates, while cross-role or fixed-location reuse refuses. Stable-read S2 and return opaque role-tagged `SensitiveConfigurationDocument` values only on S2=S1. Only afterward may selection call the chosen JSON/PEM parsers; no unselected credential/certificate document is parsed. Exhaustion returns only `configuration_snapshot_inconsistent`. Writers synchronize and replace every safe source first and publish the synchronized snapshot manifest last.
4. Treat Plan 0002's all-row deterministic policy fakes and at-most-one explicitly untrusted current-native report as untrusted inputs. Rerun every available stable-read/link/permissions/ownership/replacement/root-containment branch on every exact Plan 0009 owner-mapped native row and emit one canonical per-row report that binds the real platform metadata tuple, exact two-per-file/two-whole-generation trace, generic/aggregate/source-role/S2 outcomes, and real-versus-explicitly-unsupported policy inventory. The report states that an actively malicious same-account writer able to alter the same open object and manipulate metadata is inside the operating-system-account trust boundary and that the protocol makes no isolation claim against it.
5. Harden production only where the new schedules expose a gap, preserving one opened-handle identity through both bounded reads, parser-independent generation coordination, and temporary role-tagged sensitive-document ownership.
6. Assert public `ConfigurationDiagnostic` output contains only `source_class|stage|structural_location|code|occurrences`, never a source reference/name/private digest/unknown key/value/parser excerpt/dependency cause or source-ordering oracle. Enforce the inclusive 32-item marker with exact 31/32/33 vectors.
7. Run the suite concurrently against separate temporary configuration roots to detect shared global path state.

**Tests:**

- No link, traversal, replacement, wrong-owner, permissive, nonregular, or oversized object supplies credential or trust-root bytes.
- A valid file is read twice from the same verified handle and accepted exactly once only after identical pre/middle/post evidence, byte sequences, exact lengths, end-of-file observations, and policy checks.
- In-place mutation, metadata drift, a short/growing read, or unequal A/B bytes closes the named per-file attempt; atomic replacement yields one complete old or new file on the second attempt or refusal, and two unstable attempts yield only `configuration_file_changed_during_read` with no bytes. Cross-file old/new hybrids, inventory/role/collision mismatch, aggregate overflow, or changed S2 starts the second complete S1/source/S2 attempt and then yields only `configuration_snapshot_inconsistent`; one complete manifest-last generation succeeds without exposing a private digest or source reference.
- A source at 1,048,576 bytes and aggregate at 16,777,216 bytes succeed when the role-specific limit also permits; the next byte/source refuses before retention. The coordinator has no JSON/PEM/trust/key dependency, S2 equality precedes selected JSON/PEM parsing, and unselected sensitive documents remain opaque and are disposed.
- Unsupported ownership enforcement fails explicitly rather than silently weakening policy.
- Concurrent roots and replacement schedules cannot cross-read or escape their roots.
- Every supported native row emits the exact real-versus-policy inventory and stable-read trace for configuration roots, credentials, and additional trust files; task `release-artifact-contract` reruns and binds that complete report into authenticated row evidence. No Plan 0002 policy fake or explicitly untrusted current-native report can satisfy or aggregate a release row, and neither suite claims isolation from an actively malicious same-account writer able to manipulate the same open object and its metadata.

- **Done when:** `cargo test -p slingshot-development --test credential_filesystem_threats` passes stable equality, in-place mutation, atomic replacement, first-attempt recovery, exhausted instability, and complete/mixed committed-generation schedules plus every supported all-row real-platform configuration-root/credential/trust-file attack with exactly two named per-file/two-read/three-metadata attempts, exactly two named whole-generation attempts, exact generic/aggregate bounds, parser-independent role/collision/S2 ordering, temporary sensitive-document ownership, source-reference-free inclusive diagnostics, root containment, explicit platform policy and same-account boundary; emits canonical untrusted-input-separated release-evidence inputs owned solely by Plan 0009; and `scripts/quality` succeeds.
