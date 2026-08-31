# Workflows

How the pinned external executor drives a Slingshot command, what identifies
the work it starts, and what each way of ending means for the workflow that
started it.

The commands it calls are the ones in [COMMANDS.md](COMMANDS.md); the server it
calls them through is the one in [MODEL_CONTEXT_PROTOCOL.md](MODEL_CONTEXT_PROTOCOL.md).

## What is pinned

The integration boundary is executable behaviour rather than a shared library,
so what this repository is compatible with is one exact external commit and two
exact upstream contracts, recorded in `compatibility/finite-state-machine.toml`
and read from there by everything that checks them.

<!-- generated: pin -->

| What | Value |
|---|---|
| Repository | `https://github.com/koraytaylan/fsm` |
| Commit | `7d183e4d7a6b130343ea7d88897e0d029f604813` |
| Protocol revision | `2025-06-18` |
| Handler format | `fsm.handlers/1` |
| Daemon runtime contract | `slingshot.daemon-runtime-contract/1` at `165033f6049fc0b6cdbd67002b21950b3c38be6bfadee7ddee07427edda48901` |
| Author-agent transport contract | `slingshot.author-agent-transport-contract/1` at `295fc1bdf0b88ecb5cbd45898d9a29d0dae1bada76d6c6fced1e99e7cdb2b9f8` |

<!-- end generated: pin -->

Both contract digests are recomputed from the bytes they are recorded against
before anything runs. Comparing one recorded string with another would prove
only that somebody wrote the same thing twice.

## How the processes fit together

The executor starts one protocol server process for one effect attempt,
negotiates its revision, calls one fixed tool, and maps the result into a
durable acknowledgement. The server connects that short-lived process to the
daemon that owns the target. Nothing is found: every executable is supplied by
path, and a missing one refuses the scenario rather than falling back to
whatever is on the machine.

## Which handler does what

A handler names the tool it calls, the arguments it calls it with, its own
deadline, and its retry policy. The executor owns whether the table is a valid
table; this product owns whether the tool exists, whether the identity carried
is the kind that tool has, and whether the executable named can be run here.

Nothing is defaulted on this side. A handler this product acts on spells out
all four retry members and, for every advance, its payload and its stamps -
because the alternative is two places deciding what an omission means.

A wait time from a handler is refused: the executor owns the handler deadline,
and a second timer would make one of the two a lie.

## How one command effect is named

<!-- generated: operation-key -->

The preimage is one object declaring `slingshot.workflow-effect-operation-key/1`, with no whitespace, its members in byte order, and the occurrence in minimal base ten. Its digest, prefixed with `slingshot-workflow-effect-1-`, is the key.

- Each input is a nonempty valid-UTF-8 string of at most 128 bytes, carrying no control code point.
- The only suffixes are the empty one and `-backup-restore`.
- A key is at most 107 bytes.

<!-- end generated: operation-key -->

This is what makes retrying safe: the same intended occurrence always derives
the same key, so a retry attaches to work that already exists. Two deliberate occurrences, or two stores with
their own namespaces, derive different keys and start different work. Nothing
is normalized: a composed and a decomposed spelling are two names.

A maintenance control carries no key at all. It is identified by its target and
its reviewed digest, and a key would invent an operation identity for something
that has none.

## What a workflow journals

An acknowledgement is the durable record a later decision is made from, so the
structured value is the envelope exactly as the daemon answered rather than a
summary of it. An answer inside the cap travels whole; one past it travels as a
nested prefix and the digest of the whole thing.

Two externalized branches stay disjoint. A command's own result is published as
the deterministic operation artifact and addressed through the operation that
produced it. A maintenance result is published as an association of a target
and addressed by that target and its identifier.

## What entitles a workflow to undo something

Two gates, in order, and both required. The first is evidence: the daemon said
the work provably ran and failed. The second is a person: a separate approval
event the machine waits for and cannot produce for itself. A failure category,
an error flag, or a message that reads like a failure establishes nothing -
those are descriptions, and compensation acts on the world.

Undoing something is a third operation rather than a repeat of either of the
first two, because the compensating effect carries its own key suffix.

## Retries and restarts

A handler deadline elapsing ends the call and not the work. The retry that
follows carries the same key and reaches the same operation. A nonterminal
state is not a failure, and the number of physical records the far side holds
is not the number of operations: bounded duplicates are permitted and every one
past the first no-ops.

A daemon restart is transparent inside the handler deadline. The key comes from
the occurrence, the operation is durable, and the effect fence is part of that
durable state.

## The compatibility gate

One command, provider-neutral and repository-local. It is given a checkout of
the pinned source and, optionally, one bounded directory to seed a private
Cargo home with:

```sh
scripts/check_finite_state_machine_compatibility \
  --finite-state-machine-source <checkout> \
  [--cargo-home-seed <directory>]
```

It validates the manifest before it builds anything against it, builds the
pinned executable and this product, and runs every target in
`compatibility/finite-state-machine-test-targets.toml` exactly once. It refuses
rather than skips: an absent source, an absent executable, and an empty
inventory are refusals, because a gate reporting success having run nothing is
the failure the arrangement exists to prevent.

A supplied seed forces frozen, offline resolution and is bounded and digested
without receiving any provenance claim. Without one, network-capable
acquisition is recorded rather than assumed away.

## Examples

<!-- generated: examples -->

- [`artifact.machine.json`](../examples/finite-state-machine/artifact.machine.json)
- [`compensation.machine.json`](../examples/finite-state-machine/compensation.machine.json)
- [`daemon-restart.machine.json`](../examples/finite-state-machine/daemon-restart.machine.json)
- [`failure.machine.json`](../examples/finite-state-machine/failure.machine.json)
- [`operation-key.machine.json`](../examples/finite-state-machine/operation-key.machine.json)
- [`retry.machine.json`](../examples/finite-state-machine/retry.machine.json)
- [`slingshot.handlers.template.json`](../examples/finite-state-machine/slingshot.handlers.template.json)
- [`success.machine.json`](../examples/finite-state-machine/success.machine.json)

<!-- end generated: examples -->

## What is not here

This document describes what is integrated and what is checked locally. It
establishes no hosted-provider, repository-identity, workflow, runner,
hostile-build-isolation, complete-provenance, or reproducibility claim, and an
externally supplied machine definition or event remains outside this product's
enforcement boundary.
