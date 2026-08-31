# The daemon

One daemon serves one profile and environment. This document says what it
guarantees, and what it deliberately does not.

## What a daemon is for

A command against a remote system can take minutes, and the process that asked
for it may not survive that long. The daemon outlives its clients: it accepts
work, records it durably, runs it, and can still answer about it afterwards —
including after the client that asked has gone, and after the daemon itself has
been restarted.

## One target, one daemon, one owner

A runtime namespace is named by a profile and an environment, and nothing else.
Rotating a secret or selecting another security context does not move a daemon
to a different endpoint, because the process that owns a profile and environment
owns it whoever it turns out to be talking to. What those values partition is
the durable data, one layer down, through the opaque author-target digest.

Exactly one process owns a namespace at a time, and ownership is the
operating-system lock it holds. A readiness record and a numeric process
identifier are diagnostics. The identifier in particular proves nothing: the
operating system reuses them, so a record whose identifier matches a running
program may be naming something unrelated. Nothing looks one up, checks it, or
signals it. Deciding whether an apparent owner is alive means reaching its
endpoint and comparing the nonce that answers with the one its record claims.

## Two roots

Endpoints, locks, and readiness records live under an ephemeral per-user runtime
root and are expected to vanish with a login session. Databases, artifacts, and
diagnostics live under a persistent per-user state root and must not. Replacing
the runtime root is a new login; everything durable is still there afterwards.

## Reaching readiness

Startup runs in one order and fails closed. Ownership, then the environment
snapshot, then the installation comparison, then the database migration, then
the cross-partition audit, then the listener, then hello, then readiness. A
client that can see readiness may assume every earlier stage held.

The audit is worth stating on its own. A daemon serves one author target at one
environment revision, and the state it opens may hold work an earlier daemon
admitted under a different identity. Finished work is history and stays
queryable. Unfinished work under another target or revision refuses startup,
unchanged and unreconciled, because adopting it would mean executing against a
security context nobody chose.

## What execution does in this build

A product build installs the author-backed operation executor. Startup composes
it after the storage is open and the cross-partition audit has passed, and it is
installed rather than chosen: no setting names an executor, so no deployment can
end up running the one that runs nothing. That one stays reachable through an
explicit constructor in the test-support crate, which no product crate depends
on.

An execution goes through it in one order. The submission is derived from what
this build has and sent; the filtered event stream for that target partition
supervises it; a snapshot reconciles it when the stream is not enough; and the
artifacts its result declares are fetched and published. Only the last of those
may report success.

Everything unresolved along the way is outstanding work rather than an ending. A
submission whose fate is unclear, a stream that dropped, an artifact that is not
there yet: each carries a recovery category and a certainty, because settling an
operation on this daemon's own difficulty would be reporting a local problem as
a remote fact. Two answers do end an execution without the agent finishing it -
work the agent no longer holds, and two accounts of it that disagree - and both
fail closed rather than guessing.

## Facts an operation can be in

An operation is named by its target partition and its identifier together. A
repeat is the same work only when the environment revision and the command
fingerprint also match; anything else wearing that name is a conflict, and a
conflict changes nothing. The daemon derives the fingerprint itself rather than
believing one it was handed.

Three outcomes are not interchangeable, and the daemon never blurs them.

- **Succeeded** — the work happened, and the row says where its result went.
- **Failed** — the work ended, carrying a kind and exactly the one disposition
  that kind admits.
- **Recovery required** — the work has not ended. This is neither an ending nor
  a success, and reading it as either is the mistake the types exist to prevent.

The case that most needs care is a remote that provably succeeded whose result
cannot be stored. The operation stays nonterminal under persistent-capacity
unavailability carrying authoritative remote success, publishes no result slot,
and keeps its fingerprint — so resuming retries the local half and never the
remote one. Inventing an execution uncertainty there would rewrite proven work
as work that might not have happened.

## Waiting, listing, and reading

Watching is free. A slow waiter, a waiter that stopped reading, a disconnected
waiter, and the maximum number of waiters at once all leave an operation exactly
as it would have been with nobody watching. A waiter that is already behind is
told the current state at once rather than left blocked on an event that already
happened. A queue under pressure drops superseded progress and never drops a
recovery, a resume, or an ending.

Listing pages by arrival sequence rather than by time, because a timestamp is
not a position: two operations can share one and a clock can move. Rows admitted
during a walk appear above where the walk already is.

Reading an artifact is a start, some chunks, and an end, and the end is the only
place success is reported — after the whole second pass agrees with the recorded
digest and length and the handle is still the same file. A resumed transfer
reads and hashes the prefix it is skipping rather than seeking over it.

## Resuming and maintaining

Resuming makes a durable row eligible for the scheduler again and does nothing
else: no identifier is allocated, no executor is invoked, nothing is submitted
remotely, and the command fingerprint is untouched. Every resume is answered
from a durable receipt, because whether a resume took effect cannot be
reconstructed from current state.

Maintenance is the only path by which durable state shrinks, and nothing
triggers it but a person. There is no age policy and no automatic pruning. A
preview says exactly what would go and changes nothing; an apply quotes the
digest of what was previewed and does exactly that, or refuses. Only terminal
operations are ever selected.

## Stopping

Stopping refuses new work, lets what is running finish, and only then withdraws
readiness and closes the listener. Stopping is not cancelling. It is authorized
by the current instance nonce alone, so a nonce a previous instance published
cannot stop its replacement.

## Diagnostics

A detached daemon has nobody watching its terminal, so what it records is the
only account of what it did. Records are redacted before they are written,
because a file outlives the moment that produced it. Diagnostics are bounded
separately from operations and artifacts, so neither can exhaust the other.

## What is not here

- No operation executor in a product build, so no remote work runs.
- No maintenance-result associations; the manifest, preview, apply, and receipt
  are here, and the association ownership transfer is not.
- No automatic retry timer. Retry eligibility is computed, and a scheduler is
  asked; nothing here advances remote work on its own.
- No claim about any platform beyond the one a check actually ran on.
