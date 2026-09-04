---
id: product-prose-is-bound-to-current-facts
title: "Product Prose Is Bound To Current Facts"
workstream: "0061"
kind: task
depends_on: ["every-supported-row-gates-the-change", "unreviewed-dependency-duplication-refuses", "publication-requires-one-closed-release"]
gated: false
touches:
  - README.md
  - ARCHITECTURE.md
  - docs/DOCUMENTATION_REVIEW.md
  - .github/workflows/platform-runtime.yml
  - crates/slingshot-development/tests/product_documentation.rs
status: complete
merged_as: "pending"
---
# Product Prose Is Bound To Current Facts

Root product documents currently add an unsupported Windows row and deny package metadata, profile loading, and release machinery that are present. The test accepts a subset of platform rows and explicitly requires some stale phrases, so its green result preserves the mismatch.

**Steps:**

1. Rewrite current-state sections of `README.md` and `ARCHITECTURE.md` from the workspace manifests, exact platform matrix, profile-loading composition, automation authority, and release interfaces. Remove the stale Windows comment from the native workflow.
2. Make supported rows an exact set comparison, including rejection of prose-only extras, and compare package publish/license/repository facts structurally for every workspace package.
3. Bind profile-consumption and release-availability statements to explicit source/workflow authorities without requiring one obsolete sentence as a proxy.
4. Bind `DOCUMENTATION_REVIEW.md` to the full commit or complete document digests it reviewed, or stop treating it as ongoing evidence. Add mutations for an extra platform, missing metadata, and inverted profile/release claims.

- **Done when:** root product prose and workflow commentary state the exact current platform, package, profile, and release facts, every one-fact mutation fails the documentation contract, and an older review record cannot certify changed documents.
