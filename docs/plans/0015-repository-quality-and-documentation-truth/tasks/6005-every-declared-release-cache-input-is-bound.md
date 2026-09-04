---
id: every-declared-release-cache-input-is-bound
title: "Every Declared Release Cache Input Is Bound"
workstream: "0060"
kind: task
depends_on: ["unreviewed-dependency-duplication-refuses", "hosted-tools-are-authenticated-before-execution"]
gated: false
touches:
  - .github/workflows/release.yml
  - scripts/prepare_locked_source_cache
  - scripts/release_acceptance
  - crates/slingshot-development/src/release_input_cache.rs
  - crates/slingshot-development/src/release_acceptance.rs
  - crates/slingshot-development/src/release_acceptance/evidence.rs
  - crates/slingshot-development/tests/release_input_cache.rs
  - crates/slingshot-development/tests/release_acceptance.rs
  - crates/slingshot-development/tests/fixtures/release-input-cache/cache-manifests.jsonl
status: planned
merged_as: ""
---
# Every Declared Release Cache Input Is Bound

The workflow says the cache is prepared once but prepares it independently in every matrix row and uploads only the coordinator copy. It also builds a verified coverage-fuzzing bundle in every row; preparation checks only that the directory exists, so changing that required input changes no cache output.

**Steps:**

1. Before adding behavior to the 995-line `release_acceptance.rs`, move its bounded evidence-tree and digest machinery into a focused submodule without changing the public interface or behavior; require the source-size policy and existing acceptance suite to remain green across the split.
2. Prepare one complete cache in a dedicated producer job, transfer that exact cache read-only to every build row, and record one cache-manifest digest in every row's evidence. Remove per-row networked cache preparation and discarded coverage-tool builds.
3. Put the verified coverage bundle and complete `fuzz/Cargo.lock` closure only in the coordinator member, under closed path and size/count contracts, and bind their trees plus source, lock, toolchain, and executable identities in the canonical manifest.
4. Verify both dependency closures and the bundle offline from the coordinator member without consulting `PATH`, ambient Cargo state, or preparation inputs. Give cache surveying the same no-link, regular-file, path, count, and byte rules as acceptance evidence. Run the bounded committed fuzz targets as an explicit acceptance gate and retain its report digest.
5. Add data-flow fixtures that mutate, omit, add to, rename, link, or substitute the bundle or fuzz closure. Prove one producer invocation and the identical cache identity in every row's evidence.

- **Done when:** acceptance evidence code remains within source-size policy after a behavior-preserving split, one prepared cache reaches every release build with one identity, its coverage executable and fuzz graph are provenance-bound and exercised offline, cache and acceptance survey identical trees under the same no-follow bounds, and any input/path/report mutation or per-row divergence refuses.
