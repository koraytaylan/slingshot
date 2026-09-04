# Plan 0015 — Repository Quality, Release Integrity, and Documentation Truth

## Architectural boundary

Provider workflows remain adapters over repository-local checks. The native check is an argument-free repository script whose scope is explicit and runnable by a developer; the workflow supplies only the mapped runner identity. Dependency policy consumes one authenticated advisory snapshot for two independently locked graphs.

Release assurance stays in repository-local tooling too. Network-acquired archives remain untrusted until a manifest-owned immutable identity verifies their still-archived bytes. Cache production has one producer and one canonical manifest consumed read-only by every build row. Acceptance production and verification are separate authorities: the producer records observations, while the verifier receives independent expectations and hashes the exact retained inputs and reports. Publication crosses the provider boundary only after one complete preflight pins the clean checkout, local and remote tag, accepted decision, supported-row set, archives, and attestations to the same commit.

Product documentation has machine-checkable factual islands rather than a test that guesses at prose. Exact target rows come from `support/platforms.toml`, package publication/legal/source facts from workspace manifests, profile behavior from the product composition boundary, and release claims from the committed release interface. Prose may explain those facts, but may not add rows or negate them.

## What proves what

A pull-request check proves a supported row only when the row compiles all targets/features and runs the declared OS-sensitive test inventory on that native runner. A one-test smoke job is not evidence for cfg-gated code elsewhere.

Each lockfile is passed independently through the same offline policy with the same reviewed configuration and advisory snapshot. A fixture reachable only from the fuzz graph proves the second invocation cannot disappear unnoticed.

A version string proves compatibility, not provenance. Corrupt, substituted, missing-identity, wrong-platform, and counterfeit-ambient tool fixtures must execute and install nothing. Cache mutation fixtures prove every declared input contributes bytes and provenance to the canonical cache identity and that every row consumes that same identity.

An acceptance manifest cannot be its own expected value. Verification closes the set of report names, reads bounded regular files without following links, recomputes their digests and all retained input identities, and derives the overall decision from those verified outcomes. A recording provider client proves every publication refusal occurs before create, edit, or upload rather than between mutations.

Documentation checks compare exact sets and structured facts. Mutation fixtures add an obsolete platform, remove inherited metadata, toggle a release/profile fact, and require the contract to fail. A human review record names the exact commit or document digest it reviewed and expires as present-factual evidence when that identity changes.

## What stays outside

Hosted execution does not replace local quality, and documentation tests do not enforce tone or wording. Provider behavior and signing-key custody are not simulated. Historical plans retain the facts and claims of their time.
