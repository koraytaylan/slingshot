---
id: canonical-command-fingerprint
title: "Canonical Command Fingerprint"
workstream: "0015"
kind: task
depends_on:
  - daemon-runtime-contract
gated: false
touches:
  - crates/slingshot-domain/src/command_fingerprint.rs
  - crates/slingshot-domain/tests/command_fingerprint.rs
  - "crates/slingshot-domain/tests/fixtures/command_fingerprint/**"
status: done
merged_as: "da1bbf3dabb466c1a9721fcc68155b19b121c993"
---
# Canonical Command Fingerprint

An operation identifier is safe to retry only when the daemon can prove the repeated request describes the same target and command. This task defines that proof as a versioned canonical digest.

**Steps:**

1. Commit fingerprint vectors first for every Plan-0003 command family, reordered object fields, genuine same-principal credential/publisher rotations, changed canonical metascope, `VerifiedIdentityManagementTrustPolicyIdentity`, or `VerifiedAuthorTrustPolicyIdentity`, explicit-restart operating-system provider-policy or selected-additional-author-CA drift, changed opaque authentication-principal identities, profile-authentication-contract-only drift, Plan 0002's named Basic and Cloud organization/client/`integration.id`-backed technical-account vector outputs, and one-field command mutations. Consume those upstream expected typed outputs without duplicating their preimage construction.
2. Define the fingerprint input as the complete opaque Plan 0002 `AuthorTargetIdentity`, followed by exact `SelectedEnvironmentRevision`, command schema version, command kind, and typed command content. Use Plan 0002's `AuthorTargetIdentityDigest` hash output directly as the repository partition; never hash its lowercase rendering or reconstruct the target from deployment/address/principal/contract members.
3. Exclude publisher metadata beyond its revision contribution, raw principal values, credential secrets, request identifiers, wait cursors, timestamps, caller display data, source-generation evidence, and connection-local state. Do not exclude selected revision: the profile-authentication-contract digest, canonical metascopes, `VerifiedIdentityManagementTrustPolicyIdentity`, and `VerifiedAuthorTrustPolicyIdentity` enter only through Plan 0002's opaque exact values.
4. Hash the canonical bytes with SHA-256 and expose a fixed-size fingerprint type with lowercase hexadecimal rendering and strict parsing.

**Tests:**

- Each committed vector yields its independently calculated digest.
- Semantically identical values with different construction order have one fingerprint.
- Upstream vectors for author origin/context/deployment, Basic username, or each Cloud organization/client/`integration.id`-backed technical-account change supply a different opaque target and therefore a different fingerprint; Plan 0004 never parses those members. Profile/environment aliases are represented by the complete upstream typed identity rather than display names.
- An otherwise identical command preserves its fingerprint when Plan 0002 supplies equal opaque target and revision values under genuine same-principal rotation; any supplied target or revision change does not. This task neither reconstructs nor enumerates upstream principal/revision preimage fields.
- Changing only `SelectedEnvironmentRevision` changes the fingerprint, and that revision is also persisted and compared independently. Canonically equivalent route-typed root snapshots preserve `VerifiedIdentityManagementTrustPolicyIdentity` and `VerifiedAuthorTrustPolicyIdentity`; an explicit-restart platform-policy or selected-additional-author-CA change preserves the upstream target but changes the appropriate identity and revision/fingerprint, while a live root-source edit changes neither retained snapshot nor fingerprint.
- A profile-authentication-contract-only mutation changes both upstream target and revision, the repository partition is byte-for-byte the one upstream target hash (not a second hash), and no raw contract digest becomes a separately interpreted Plan 0004 field.
- Excluded transient fields do not affect the fingerprint.

- **Done when:** `cargo test -p slingshot-domain --test command_fingerprint` matches every committed vector and proves complete opaque target plus exact selected-revision inclusion, direct use of `AuthorTargetIdentityDigest` without a second hash, profile-contract/principal partition changes, genuine same-principal credential-rotation stability, live-root immutability versus restart-visible scope/trust revision changes, and publisher/raw-secret/source-generation exclusion boundaries for every command family, and all workspace gates succeed.
