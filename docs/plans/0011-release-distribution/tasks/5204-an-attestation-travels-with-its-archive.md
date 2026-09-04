---
id: an-attestation-travels-with-its-archive
title: "An Attestation Travels With Its Archive"
workstream: "0052"
kind: task
depends_on: ["publish-refuses-unverified-bytes"]
gated: false
touches:
  - crates/slingshot-development/tests/release_workflow_evidence.rs
  - crates/slingshot-development/tests/fixtures/release-workflow-evidence/workflows.jsonl
status: planned
merged_as: ""
---
# An Attestation Travels With Its Archive

The verifier promises to authenticate an archive without reaching the network. That promise holds only if the bundle is in the same uploaded directory as the archive it attests; left where the attesting action wrote it, it is reachable only by asking the provider for it.

**Steps:**

1. Commit one accepted workflow that places its attestation beside the archive it attests, and rejected ones that attest without keeping the bundle, that keep it outside the uploaded directory, and that upload the archive alone.
2. Parse each workflow rather than matching its text, and require the attesting job to place a bundle inside the directory the same job uploads.
3. Require the committed release workflow to be accepted by the same check.

- **Done when:** a workflow whose attesting job does not place the bundle beside its archive is refused, and the committed workflow passes.
