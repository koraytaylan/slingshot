---
id: owner-confirmed-github-automation
title: "Owner-Confirmed GitHub Automation"
workstream: "0038"
kind: task
depends_on:
  - pinned-coverage-fuzzing-tool
  - minimum-rust-and-dependency-gates
  - daemon-process-chaos
gated: true
touches:
  - support/github-automation-authority.toml
  - .github/workflows/quality.yml
  - .github/workflows/platform-runtime.yml
  - crates/slingshot-development/src/github_automation_authority.rs
  - crates/slingshot-development/src/lib.rs
  - crates/slingshot-development/src/main.rs
  - crates/slingshot-development/tests/github_automation_authority.rs
  - crates/slingshot-development/tests/github_workflow_contract.rs
  - "crates/slingshot-development/tests/fixtures/github-automation-authority/**"
status: planned
merged_as: ""
---
# Owner-Confirmed GitHub Automation

Hosted quality, native multi-platform automation, and signed release provenance require a repository/build authority that cannot be inferred from an unconfigured Git remote or another project. This release-plan gate adds GitHub-specific adapters only after the provider decision; Plans 0001–0008 remain executable through repository-local commands and explicitly supplied native hosts without it.

**Steps:**

1. Keep this task gated until the repository owner confirms GitHub Actions, canonical/immutable repository identity, one exact available native environment for each abstract Plan 0001 target/capability row, one OCI-capable coordinator, one probed FSM-compatible row, and one protected release environment with immutable identifier and required-owner-review policy for the exact RustSec pin. Plan 0001 supplies no concrete runner, linker, software-development kit, or aggregate proof. Sequential Phase R must already have integrated Plan 0008.
2. For every native row, require the owner to select and probe one closed source-protection claim: `operating_system_enforced` names the exact different-principal/read-only-filesystem primitive, coordinator/build identities, denied ownership/access-control/remount privileges, and active write/chmod/rename/replace probe; `digest_observation_only` records read-only permission bits plus before/after digests but explicitly disclaims malicious same-principal isolation. Also name and probe the row's exact build-subprocess non-loopback-network denial primitive. An unavailable or failed declared primitive makes the row ineligible for release evidence rather than triggering an operating-system-family fallback.
3. Commit accepted and rejected fixtures for exact repository identity, concrete target-to-environment mappings and probes, Windows remote named-pipe client capability, protected RustSec-review environment/reviewer policy, per-release exact-pin review records, self-hosted/cross-compile/duplicate/placeholder cases, coordinator/FSM selection, source-protection modes, network denial, and exact RustSec/FSM handoffs before implementation.
4. Define closed `support/github-automation-authority.toml` with repository identity, workflow root, exact abstract-target-to-native-selector/host/architecture/observed-image/toolchain/linker/system-root-or-software-development-kit mapping, endpoint capabilities including real Windows remote-client probing, source-protection/network-denial probes, coordinator/FSM rows, and protected RustSec-review environment/reviewer policy. Store no credential, mutable label, inferred support, or generic unprobed mechanism.
5. Implement a development validator that accepts only explicit document input, compares provider-supplied repository identifiers and workflow path with the closed authority, and refuses an absent, renamed, forked, self-hosted, cross-target, placeholder, ambient-remote, unprobed source-protection, or ineffective network-denial identity before a hosted job contributes evidence.
6. Create full-commit-pinned workflows that validate authority and invoke repository-local commands. Ordinary quality authenticates Plan 0001's exact RustSec snapshot without freshness. Each release run has a non-skippable protected-environment `rustsec-owner-review` job: after required owner approval it verifies the exact source commit/tree and RustSec origin/full-commit/tree and emits one canonical record binding those digests, environment/reviewer-policy digest, workflow/run/attempt identity, with no author timestamp or reusable `fresh` flag. Cache preparation consumes only that record in the same run. Derive one native job per abstract row and map it to the exact owner-approved environment; run real capability/runtime/walking and threat suites, including real Windows local-success/remote-client-refusal evidence, and label Plan 0001's earlier current-native observation untrusted.
7. In the quality workflow, add one required non-skippable `pinned-fsm-compatibility` job only after authority and probe-record validation. Run it solely on the separately owner-declared compatible row, check out `https://github.com/koraytaylan/fsm` at `7d183e4d7a6b130343ea7d88897e0d029f604813` into a job-local path with disabled credential persistence, and invoke Plan 0008's unchanged `scripts/check_finite_state_machine_compatibility --finite-state-machine-source <path>` command. Emit a canonical source/row/gate-manifest/report digest for later same-row hosted release evidence. This network-enabled quality report retains Plan 0008's source-pin compatibility scope; it is not the cache-closed release build. The adapter neither expands the job to unproven Slingshot rows nor duplicates the gate's test inventory nor substitutes a narrower test, installed executable, synthetic client, source-discovery path, skip, or fallback.
8. Make the compatibility and release workflows depend on the same validated authority and exact RustSec preparation contract. Task `release-input-cache` separately prepares each cold offline row closure plus the Plan-0008-manifest-verified selected-row/coordinator Cargo-home seed projections, and task `owner-confirmed-native-evidence-trust` fixes the attestation authority before task `release-artifact-contract` adds the privileged provenance job and reruns/binds the single-row FSM report through the unchanged gate's exact `--cargo-home-seed <verified-private-seed>` path with network denied. Release acceptance later repeats that exact gate against the coordinator projection; this task owns neither projection nor rerun.

**Tests:**

- Exact owner/repository identifiers, all three target-to-runner mappings, each row's exact source-protection claim/probe and network-denial mechanism/probe, one OCI-capable coordinator row, and one distinct single-row pinned-FSM compatibility selection/probe digest are required once and match the canonical HTTPS address and supported matrix.
- A matching display name with a different immutable identifier, renamed/forked repository, self-hosted runner, mutable branch identity, missing target, duplicate selector assignment, and cross-compile-only mapping fail closed.
- The validator neither reads an ambient Git remote nor accepts a process-environment value as repository authority.
- Credential-shaped fields and private signing material are rejected from the document and all diagnostics remain bounded and secret-free.
- Immutable actions, disabled credential persistence, least privileges, and shell-flow rules hold. Quality proves only the exact RustSec snapshot. Every release run requires a new protected-environment owner-review record matching its source/pin/run; a copied/prior-run/self-timestamped record fails. The native matrix has one exact owner-mapped environment per abstract row, proves source/network modes and platform suites, and the Windows row proves `PIPE_REJECT_REMOTE_CLIENTS` through a real remote client. No cross-compile/family/current-native Plan 0001 report substitutes. The FSM adapter remains single-row and exact.

- **Done when:** `cargo test -p slingshot-development --test github_automation_authority --test github_workflow_contract` proves exact owner-mapped/probed native environments, real Windows remote-pipe rejection, per-row source/network claims, one protected per-release RustSec owner-review record with no self-authored time, coordinator/FSM authority, and closed immutable workflows before any all-row or release claim.
