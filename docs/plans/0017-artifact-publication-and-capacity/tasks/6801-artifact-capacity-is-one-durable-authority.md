---
id: artifact-capacity-is-one-durable-authority
title: "Artifact Capacity Is One Durable Authority"
workstream: "0068"
kind: task
depends_on: ["the-artifact-store-publishes-verified-bytes"]
gated: false
touches:
  - crates/slingshot-storage/src/persistent_capacity.rs
  - crates/slingshot-storage/src/artifact_store.rs
  - crates/slingshot-storage/src/operation_repository.rs
  - crates/slingshot-storage/migrations/**
  - crates/slingshot-daemon/src/operation/artifact_completion.rs
  - crates/slingshot-storage/tests/persistent_capacity/**
  - crates/slingshot-storage/tests/artifact_store/**
status: planned
merged_as: ""
---
# Artifact Capacity Is One Durable Authority

Aggregate artifact accounting lives in a mutex on one `PersistentCapacity` object, while stores and repositories can construct independent objects and the product completion trait has no implementation. Nothing reserves capacity before `ArtifactStore::install` streams bytes.

**Steps:**

1. Persist namespace-scoped committed and reserved artifact bytes under the storage transaction authority so independent processes and objects observe one serialized total.
2. Reserve the declared maximum before accepting a stream, enforce per-artifact and aggregate exact/plus-one bounds with checked arithmetic, and convert only the actual verified size into a committed blob/association in the same durable transition.
3. Charge a verified digest once across associations, release reservations on refusal/cancellation, and reconcile abandoned reservations and store blobs after restart without deleting referenced content.
4. Implement the daemon `ArtifactSink` over this authority and the hardened store, so no later product adapter can bypass reservation by calling raw install.
5. Test concurrent independent instances, oversized first input, limit-minus-one crossing, integer overflow, duplicate digest, interrupted stream, commit failure, and restart; compare accounting to the verified store and database inventory.

- **Done when:** committed plus reserved artifact bytes never exceed the namespace contract across concurrent processes or restart, deduplicated bytes are charged once, abandoned reservations reconcile, and the production completion adapter cannot stream before reservation.
