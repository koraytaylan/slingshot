# Plan 0014 — Release Acceptance Decision

## Architectural boundary

The command belongs to the tooling crate, beside the verifier that reads what it writes. It runs only inside the acceptance container and depends on nothing the container is not given, which is what makes its conclusion about the revision rather than about the machine.

## What proves what

A decision is proved by what it refuses. A gate that refuses must make the revision unreleasable, and a manifest that says otherwise must itself be refused - which the existing verifier already does, so the new command is held to a document that was specified before it existed.

The values the manifest binds are proved by their absence: a run told nothing about its revision cannot state one, so each admitted value is admitted explicitly and the contract says why. Nothing is inferred from the container's surroundings, because a container that could infer its run could be told a different one.

## What stays outside

No gate is added or removed here, and no isolation is relaxed to make a gate pass. If a gate cannot run inside the boundary as the contract draws it, that is a fact about the gate or the contract and belongs in the plan that owns it.
