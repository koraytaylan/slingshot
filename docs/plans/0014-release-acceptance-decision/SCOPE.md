# Plan 0014 — Release Acceptance Decision

> Make the decision that says a revision is releasable something that runs, instead of something the release asks for and never receives.

## Why this plan

Plan 0009 built the evidence a release rests on and the verifier that reads it. It left one thing out, and nothing noticed until a release ran: the command that runs the gates inside the isolated container and records what each decided does not exist. `scripts/run_acceptance_gates` invokes `run-release-acceptance`; the executable has no such command; the acceptance module has parsers and verifiers and nothing that produces a manifest. So `verify-release-acceptance` checks a document nobody writes, and the last job of every release refuses.

Everything around it works and was proved by running it. The container starts with no network, no capabilities, a read-only source, and a writable root the gates own; the image travels in the verified cache and is loaded rather than fetched; the dependencies resolve offline from that same cache; the whole workspace compiles inside the boundary. What is missing is the part that decides, and the release cannot claim to have decided anything until it exists.

The manifest names what the decision has to say: which row coordinated it, every gate in order with what each concluded, the digests of the isolation contract and the platform evidence and the review record every input is bound to, the exact revision and tree, the provider run that produced it, and whether the revision is releasable. Several of those are facts about the run rather than about the container, and the container is told nothing about the run today - the environment it admits is closed, deliberately, and each addition to it has to be worth what it costs.

## In scope

- **0058 — The decision, inside the boundary.** One command that runs each gate the acceptance covers, in order, records what each concluded, and writes the manifest `verify-release-acceptance` already reads. A gate that refuses makes the revision unreleasable and the manifest says which one and why; a gate that could not run at all is not a gate that held. The command runs where the gates run, with what the container is given and nothing else.
- **0059 — What the decision is told about its run.** The manifest binds the revision, the tree, and the provider run, and none of them can be discovered inside a container that is deliberately told nothing. Each is admitted explicitly, through the closed environment the contract declares, and the contract records why every one of them is there. A value the decision cannot obtain honestly is a value the manifest does not carry.

## Out of scope

The isolation contract itself is not reopened: the network stays absent, the capabilities stay dropped, the source stays read-only, and the writable roots stay the two the contract names. Which gates constitute acceptance beyond those Plan 0009 already names is not decided here. Publishing is unchanged and remains an operator's act.
