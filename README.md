# Slingshot

Slingshot is a command line and a local daemon for driving Adobe Experience
Manager from one place. This commit contains the workspace, its enforceable
engineering contract, and one proven local process boundary. No Adobe
Experience Manager behavior exists here yet.

Version 0.1.0.

## What this commit does

One daemon owns one `(profile, environment)` target. Several clients that
address the same target converge on that one daemon; different targets have
independent daemons, endpoints, locks, and state. A target name is a bounded
opaque value here, and nothing reads a profile document yet.

```sh
# Reach the daemon that owns a target, creating it if nobody has.
slingshot --profile local --environment author daemon start

# Report whether a daemon already owns that target. This never creates one.
slingshot --profile local --environment author daemon ping
```

Both write one line to the result stream and every diagnostic to the diagnostic
stream: an action and what it found or did, such as `daemon-start: created` or
`daemon-ping: absent`. Adding `--machine` writes the same outcome as one
tagged object instead. Neither form carries the daemon's readiness nonce, which
authorizes a cooperative stop and therefore stays in the runtime state its
daemon owns. `--runtime-root` places the target's runtime objects somewhere
other than this user's own runtime directory.

[docs/COMMANDS.md](docs/COMMANDS.md) is the reference for every leaf, option,
answer, failure category, and exit this executable has, and
[docs/MODEL_CONTEXT_PROTOCOL.md](docs/MODEL_CONTEXT_PROTOCOL.md) is the one for
the protocol server it can hand its streams to.
[docs/WORKFLOWS.md](docs/WORKFLOWS.md) records what the pinned external workflow
executor is integrated with and what that integration guarantees.

## Crates

| Crate | Responsibility |
|---|---|
| `slingshot-domain` | Value objects, operation and durable agent-job vocabulary, execution ports, errors, and limits |
| `slingshot-configuration` | Profile documents, target resolution, and credential references |
| `slingshot-agent-protocol` | Language-neutral author-agent messages, schemas, and wire conversions |
| `slingshot-local-protocol` | Daemon request, response, event, and framing contracts |
| `slingshot-agent-connection` | Authentication and Author network transport |
| `slingshot-storage` | Operation ledger and artifact persistence |
| `slingshot-daemon` | Target-scoped application service and local server |
| `slingshot-command-line` | The `slingshot` executable, its command-line adapter, and its daemon client |
| `slingshot-test-support` | Fake services, temporary roots, path-only executable values, and process harnesses |
| `slingshot-development` | Repository policy, orchestration, compatibility, and release commands |

The first eight form the product graph. The last two exist for tests and
repository policy, and no product crate reaches either through a library or
build dependency.

Every package is unpublished. No package declares a license, a license file, or
a repository, because no owner has supplied those values, and none is inferred
from anywhere else. A release artifact stays refused until they are supplied.

## Supported targets

`support/platforms.toml` is the only abstract supported-target authority. It
declares three rows and their release artifact layout:

| Target | Executable | Archive | Native smoke |
|---|---|---|---|
| `x86_64-unknown-linux-gnu` | `slingshot` | `tar.gz` | `direct` |
| `aarch64-apple-darwin` | `slingshot` | `tar.gz` | `direct` |
| `x86_64-pc-windows-msvc` | `slingshot.exe` | `zip` | `direct` |

Each row also names the capabilities the target must provide: the
provider-record trust decisions a store must not flatten, the endpoint, the two
separate locks, the current-user protection, atomic readiness, detachment,
stable supervised cleanup, the filesystem evidence a credential check reads,
and the deterministic build-policy requirements. The Windows row requires every
named-pipe server creation to reject remote clients.

A row is a declaration, not evidence. Every row is evaluated here through
deterministic observations, and real behavior runs only for the single row that
matches the machine the run is on. That result is one report labelled
`untrusted_current_native_observation`: it describes one machine nobody has
attested. This commit makes no aggregate claim across rows, names no
continuous-integration provider, runner image, linker, or system root, and
claims no release readiness.

## Limits

`support/foundation-contract.toml` is the only place a wire bound, a namespace
bound, an endpoint bound, a startup deadline, or a process-harness value is
written. Those bytes are embedded into `slingshot-local-protocol` and read
through one typed interface, and an assertion refuses a second copy of any of
them anywhere in the crates that consume them.

## Verifying against a real author

The hermetic suite against the fake author is the gate. To watch the same read
path run against an actual author, ask for it explicitly:

```sh
slingshot --profile local --environment author verify live-author --enable-live-author --content-root /content/site/en
```

Without `--enable-live-author` the leaf is refused before a byte of
configuration is read. What it may run is the registry's own answer: the
twenty-eight rows it calls reads that replace nothing, never the thirty-six that
write. One run is evidence about the author it ran against and about nothing
else.
[docs/COMMANDS.md](docs/COMMANDS.md) says what it reports and what it refuses to
claim.

## The daemon

One daemon serves one profile and environment, outliving the clients that ask
it for work. [docs/DAEMON.md](docs/DAEMON.md) says what it guarantees, what it
refuses, and what a product build deliberately does not do.

## Checking a change

```sh
SLINGSHOT_RUSTSEC_ADVISORY_DATABASE_DIRECTORY=<checkout> scripts/quality
```

The gate takes no argument. It verifies every pinned external executable,
authenticates the advisory database as one exact snapshot, and then runs
formatting, compilation, lints, tests, documentation, script linting, the
dependency direction, the source policy, and the dependency policy. It never
fetches anything.

The advisory input is one exact snapshot pinned in
`compatibility/rustsec-advisory-database.toml` by origin, full commit, and
content tree. There is deliberately no timestamp and no freshness claim: a Git
author chooses those values, so none of them authenticates anything.

A release builds from a cache prepared once, deliberately, over the network, and
verified offline before anything is compiled from it. `scripts/prepare_locked_source_cache`
is the one command here that reaches the network and says so when it runs;
`scripts/verify_locked_source_cache` never fetches, repairs, installs, or
consults an ambient cache. What verification establishes is narrow and stated as
narrowly as it is true: the cache is the one prepared for this lockfile,
unchanged, and inside what a Cargo home may be. Whether its bytes were
trustworthy when they were fetched is a different question that nothing here
answers.

See [ARCHITECTURE.md](ARCHITECTURE.md) for how the pieces fit together,
[CONTRIBUTING.md](CONTRIBUTING.md) for the rules a change is held to, and
[docs/AGENT_PROTOCOL.md](docs/AGENT_PROTOCOL.md) for the contract the daemon
holds an author to.
