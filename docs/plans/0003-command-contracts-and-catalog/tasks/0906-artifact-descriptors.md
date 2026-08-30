---
id: artifact-descriptors
title: "Artifact Descriptors"
workstream: "0009"
kind: task
depends_on:
  - command-module-scaffold
gated: false
touches:
  - crates/slingshot-domain/src/command/artifact.rs
  - crates/slingshot-domain/tests/fixtures/commands/artifacts.jsonl
  - crates/slingshot-domain/tests/artifacts.rs
status: done
merged_as: ""
---
# Artifact Descriptors

Every artifact-capable logical result needs shared stable identity, declared slot, integrity, size, media, and presentation metadata before individual command tasks use it.

**Steps:**

1. Commit valid/invalid fixtures for ArtifactIdentifier, ArtifactSlot, ArtifactRequirement, ArtifactDigest, metadata, OptionalAlternative `loaded_content_json` with `application/json`, exact `loaded-content.json` suggested file name, and its named maximum byte length, Required `content_package` with `application/zip`, exact `<package_name>.zip` suggested file name, and its named maximum byte length, plus forbidden/empty manifests before implementation.
2. Implement bounded ArtifactIdentifier/ArtifactSlot and closed OptionalAlternative/Required ArtifactRequirement; declare each command slot/requirement/maximum byte length once and reserve no server-supplied location.
3. Implement ArtifactDescriptor with ArtifactIdentifier, ArtifactSlot, media type, exact byte length, lowercase SHA-256 digest, and suggested file name as distinct validated fields.
4. Keep suggested file name presentation-only and reject separators, traversal forms, controls, NUL, empty, and over-bound values without using it as identity or a filesystem path.
5. Serialize a closed canonical object with no inline bytes, remote location, local path, or credential-bearing field.

**Tests:**

- Every field accepts values at its named boundary and rejects values immediately outside it.
- ArtifactIdentifier and ArtifactSlot are separately present and never interchangeable; changing suggested file name cannot change either identity.
- Digest accepts exactly lowercase SHA-256 hexadecimal bytes, and length rejects malformed or overflowing values.
- `loaded_content_json` with `application/json`/`loaded-content.json`/its maximum and `content_package` with `application/zip`/`<package_name>.zip`/its maximum are distinct stable slots with exact fixtures; an exact result length above its declared maximum is invalid.
- Load requirement is OptionalAlternative, package is Required, other commands are empty/forbidden, and generic `structured_result` cannot be declared remotely.
- Unknown, inline-byte, remote-location, and local-path fields are rejected.

- **Done when:** `cargo test -p slingshot-domain --test artifacts` passes shared artifact identity/slot, metadata, boundary, canonical-shape, and forbidden-location/bytes fixtures, and both artifact-capable command tasks can depend on one foundation type.
