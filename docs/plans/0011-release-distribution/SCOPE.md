# Plan 0011 — Release Distribution

> Make the release something a stranger can obtain and authenticate, and hold every claim on that path to an assertion.

## Why this plan

Plan 0009 built a release that proves itself: two builds compared byte for byte, provider attestation over each archive, an acceptance matrix that decides releasability offline. It put hosted release publication out of scope, and it was right to. A job that can create a release can also push to a branch, because the provider has no permission that separates the two, and `workflow_policy` refuses that class outright.

What no plan supplied is the step after the evidence. Task 3804 opens by saying a release is the archive an operator receives. Until this plan lands, no operator receives one: the archives sit as workflow artifacts that expire, need an account to reach, and no document mentions.

The path from evidence to a download was built outside any task footprint, because it was built while finding out that none of it had ever run. The tag is now held to the version the workspace declares, the notes are assembled from the history that produced them, each attestation is kept beside the archive it attests, and an operator-run publisher authenticates every archive before it uploads anything. Each of those is a claim, and this repository's first rule is that a claim comes with the assertion that proves it. They have none. This plan supplies them and adopts the work rather than leaving commits the repository's own discipline calls a defect.

The hosted gate is part of the same gap. It verifies three pinned executables before it runs a check and nothing installed them, so every hosted run refused at its first stage and the adapter proved nothing. That is now fixed and equally unasserted.

## In scope

- **0052 — The distribution path's own refusals.** Every refusal the path already makes, proved by fixtures rather than by having been watched once. The version agreement refuses a tag naming a version the workspace does not declare and a tag without the release prefix, and reports the declared version when no tag names the run. The notes drop plan bookkeeping by scope while keeping a documentation commit that publishes a product reference, against a synthetic history so the assertion cannot drift with this branch. The publisher performs no upload when an archive's attestation does not authenticate, when the notes are absent, or when the run holds no archive - proved by the absence of an upload rather than by the wording of a message. The workflow keeps each attestation bundle beside its archive, so a downloaded run is sufficient for the offline verification the verifier's own header promises.
- **0053 — What the hosted gate is given.** The gate runs on a machine that starts with none of its pinned tools, so something installs them and the installer is held to the manifest: every version read from it rather than repeated, every tool verified afterwards by the same check the gate uses, and every tool the manifest names one the installer handles. A tool added to the manifest and not to the installer is refused rather than discovered on a runner.
- **0054 — What a stranger receives.** Product documentation naming the archive, the attestation bundle beside it, and the exact command that authenticates one offline, held to the verifier's real option set so the documented command cannot describe an interface that does not exist.

## Out of scope

Hosted release publication stays out of scope and this plan does not revisit it: publishing remains an operator's act taken with an operator's credential, and no job gains a permission beyond reading content. Advancing the released branch, signing keys and their custody, package-manager distribution, and any change to the attestation trust root are outside this plan. The changelog grouping is configuration rather than a contract; what is proved is which commits reach it and which do not.
