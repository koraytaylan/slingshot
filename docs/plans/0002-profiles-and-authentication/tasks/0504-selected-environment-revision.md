---
id: selected-environment-revision
title: "Selected Environment Revision"
workstream: "0005"
kind: task
depends_on:
  - validate-profile-selection
gated: false
touches:
  - crates/slingshot-domain/src/selected_environment_revision.rs
  - crates/slingshot-configuration/src/profile_selection.rs
  - crates/slingshot-configuration/tests/selected_environment_revision.rs
  - "crates/slingshot-configuration/tests/fixtures/selected-environment-revision/**"
status: planned
merged_as: ""
---
# Selected Environment Revision

Give clients and daemons constructible opaque nonsecret principal, target, route-specific trust, and revision digests that detect changed authentication, authorization scope, or either route's effective trust without treating genuine same-principal credential rotation as a new target.

**Steps:**

1. Commit independently calculated canonical-preimage/digest vectors for manifest digest; Basic username and Cloud organization/client/technical-account tuples; root/context author; deployment; names and normalized optional sources; publisher; transport policy; insecure opt-in; empty/reordered metascopes; reordered/equivalent and changed platform-only identity-management DER sets; reordered/equivalent and changed author platform-plus-selected-additional DER sets after provider-policy eligibility; same-principal secret/key/certificate/token rotation; each included-field mutation; absent/present option encoding; checked lengths; timestamps; and excluded metadata.
2. Implement the exact common `u64be(name length) || name || presence || [u64be(value length) || value]` frame. Define `AuthenticationPrincipalIdentity` under `slingshot.authentication-principal/1` with Basic `authentication_method,user_name` or Cloud `authentication_method,organization_identifier,technical_account_client_identifier,technical_account_identifier`, where the last is `integration.id`. Reject absent/empty/oversized/noncanonical inputs and checked-length overflow; perform no Unicode normalization.
3. Define `AuthorTargetIdentity` under `slingshot.author-target/1` with raw contract digest, deployment, complete normalized author base, and raw principal digest. Define its SHA-256 output as the `AuthorTargetIdentityDigest` itself rather than a second hash. Define `CanonicalMetascopeSet`, platform-only `VerifiedIdentityManagementTrustPolicyIdentity`, and platform-plus-selected-additional `VerifiedAuthorTrustPolicyIdentity` with the exact distinct domains and count/length encodings from architecture.
4. Define `SelectedEnvironmentRevision` under `slingshot.selected-environment-revision/1` with the exact ordered required/optional fields from architecture, including both raw route-specific trust identities, and SHA-256 rendering. Exclude configuration-generation/source-content digests, secrets, public-certificate contents, raw principals, timestamps, file identity, permissions, and transient metadata. Expose only fixed-size lowercase-digest parsing/rendering; snapshot construction supplies the exact typed values.

**Tests:**

- Every vector matches an independently calculated digest and reordered TOML yields the same revision.
- Changing any included normalized metadata changes the revision.
- Replacing password, private-key, client-secret, matched public-certificate, or access-token bytes in a newly committed generation while preserving the principal tuple, canonical metascopes, and effective verified server-authentication roots preserves principal identity, target identity, and revision.
- Reordering metascopes or semantically identical eligible DER within either route preserves revision. Adding, removing, or changing a platform root changes both trust identities and the revision; changing only an additional author root changes only the author trust identity and the revision. Each case preserves target identity.
- Changing a Basic username or any one Cloud organization/client/technical-account member changes principal identity, target identity, and revision even at the same credential source; author origin, author prefix, deployment, or contract digest change also changes target identity and revision.
- Changing only insecure-author opt-in changes revision, while its warning is a deterministic projection rather than an additional digest input.
- Public target/wire/debug fixtures use only the opaque principal digest and never a raw Basic username or Cloud principal tuple; closed `ConfigurationDiagnostic` fixtures contain neither raw principal fields nor the digest.

- **Done when:** `cargo test -p slingshot-configuration --test selected_environment_revision` matches every independently calculated contract/principal/target/identity-management-trust/author-trust/revision preimage and digest, proves `AuthorTargetIdentityDigest` has no second hash, and proves genuine same-principal rotation stability, route-specific security-policy revision, principal partitioning, and exact inclusion/exclusion boundaries.
