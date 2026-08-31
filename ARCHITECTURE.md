# Slingshot architecture

What this commit contains, and how its pieces reach each other.

## The crate graph

Dependencies point inward, toward the contracts that change least.

| Crate | Depends on |
|---|---|
| `slingshot-domain` | nothing in this workspace |
| `slingshot-configuration` | `slingshot-domain` |
| `slingshot-agent-protocol` | `slingshot-domain` |
| `slingshot-local-protocol` | `slingshot-domain` |
| `slingshot-agent-connection` | `slingshot-configuration`, `slingshot-agent-protocol`, `slingshot-domain` |
| `slingshot-storage` | `slingshot-domain` |
| `slingshot-daemon` | every inward product adapter except the command line |
| `slingshot-command-line` | `slingshot-local-protocol`, `slingshot-configuration`, `slingshot-daemon` |
| `slingshot-test-support` | `slingshot-domain`, `slingshot-agent-protocol`, `slingshot-local-protocol`, `slingshot-storage` |
| `slingshot-development` | inward on the product crates and on test support |

Two executables exist. `slingshot` lives in `slingshot-command-line` and is the
product. `slingshot-development` is the repository-command executable that
checks the dependency direction, the source policy, and the advisory pin.

Durable remote-work vocabulary is domain vocabulary. The agent job identifier,
its state, its event sequence, and the event stream cursor belong to
`slingshot-domain`; `slingshot-agent-protocol` converts their wire encodings to
those domain values, and `slingshot-storage` persists the domain values without
an edge to the wire crate.

Reusable process harnesses and path-only executable values belong to
`slingshot-test-support`, which accepts paths and plain process inputs and
names no type from the command line, the daemon, configuration, the author
transport, or the repository tooling.

## One target, one daemon

A target is a `(profile, environment)` pair. Its runtime namespace is named by
a digest taken over both names with their lengths, so two targets that would
read the same once joined by a delimiter still name different namespaces.

The namespace has four objects: an endpoint, a lock a daemon holds for its
whole lifetime, a separate lock an electing client holds while it decides, and
a readiness record. The two locks are separate operating-system objects with
separate Rust types; neither substitutes for the other, and no client lends
either to a child.

Ownership is the owner lock and nothing else. A readiness record and a process
identifier are diagnostics. Records a departed owner left behind are recovered
only once the recovering process holds the lock, so a forged record cannot
displace a live owner.

## Starting and stopping

`daemon start` is a convergence protocol. A caller connects first, contends for
the election lock, rechecks after winning it, and only then, only once, and
only after the owner lock proves absence, creates one detached child from its
own absolute executable. It holds the election through a responsive probe or a
terminal failure. Callers that lose the election wait, retry the connection,
and retry the election under the same absolute deadline, so every caller
returns the same live nonce.

`daemon ping` is an existing-owner probe. It never contends, never creates, and
never waits.

A daemon draws one random readiness nonce when it takes ownership. That nonce
is the only thing that authorizes a cooperative stop: a stop carrying any other
value is refused as a stale instance with no state change, so a caller holding
a nonce from a daemon that has since been replaced can never stop the
replacement. Orderly shutdown removes the endpoint object and the record
carrying that nonce, and leaves the persistent lock file for the next owner.

A test cleans up a daemon it started through the handle its supervisor kept.
The supervisor retains the exact child, unreaped, until one disposition:
observing that it already exited, or terminating it through that same handle
and waiting. Nothing looks a numeric process identifier up and signals it, so a
replacement that reuses one can never be reached.

## The local request path

A frame is a fixed-width unsigned payload length in network byte order followed
by exactly one document. Reading is pure over byte slices: it reports whether a
buffer holds nothing, part of the prefix, part of the payload, or a whole
frame, so a server applies a deadline without reading the same bytes twice.

The server binds only while ownership is held and serves at most the declared
connection capacity. Four deadlines apply, all from the contract: one for a
connection's first control frame, one between two reads while a frame is
incomplete, one for completing a frame however slowly it arrives, and one for
writing a response. A connection that finished a frame and went quiet has no
incomplete-frame deadline, because nothing is incomplete.

The retained control surface is one version, one framing, `daemon.ping`, and
`daemon.stop`. A request naming another control version is refused before any
method is read.

## Platforms

`support/platforms.toml` declares three abstract rows and the capabilities each
must provide. A Unix row uses a Unix domain socket, advisory locks, an
owner-only runtime directory, atomic readiness, and session-independent
detachment. The Windows row uses a named pipe created with remote clients
rejected on every path, exclusive locks, the current user's access control, and
detached creation.

Every row is evaluated through deterministic observations that decide ownership
and readiness from observed facts alone, so all three are checked from one
machine. Real behavior runs only for the row the machine matches, and its
result is one report labelled `untrusted_current_native_observation`, shaped by
`support/platform-runtime-evidence.schema.json`. Mapping every row to an
owner-approved environment and attesting its evidence is release work that has
not happened.

## Limits

`support/foundation-contract.toml` is the only place a Plan 0001 wire,
namespace, endpoint, startup, or process-harness value is written. It is
embedded into `slingshot-local-protocol` and read through one typed interface,
and an assertion refuses a second copy of any of those values in the crates
that consume them.

## How the rules are enforced

`slingshot-development` parses repository Rust into a syntax tree and
classifies unchecked syntax by what it is rather than by matching text, so a
file that names the keyword only in a comment and a string passes. It parses
executable scripts into a shell syntax tree, migrations into statements, and
workflows into their real structure. It reports a file, a line, a rule, and a
symbol, ordered deterministically.

The checker decides only what it can decide. Whether prose is accurate,
complete, historically framed, or narrating adjacent code is recorded as a
review checklist in `policy/documentation-rules.toml`, and an assertion proves
the checker does not pretend otherwise.

## The daemon in detail

The ownership, startup ordering, operation facts, waiting, listing, artifact
reading, resuming, maintenance, and stopping rules are set out in
[docs/DAEMON.md](docs/DAEMON.md).

## What is not here

No Adobe Experience Manager operation, no profile document, no credential, no
durable operation storage, no remote job submission, no artifact transfer, and
no Model Context Protocol behavior. No aggregate proof across platform rows,
and no release artifact: the packages are unpublished and carry no owner-
supplied legal or repository metadata.
