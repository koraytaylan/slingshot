# Contributing to Slingshot

Every rule below is enforced by `scripts/quality`, which takes no argument and
must hold before a change lands.

## Claims come with the assertions that prove them

Write the assertion, the fixture, or the structural check before the code it
constrains. An assertion drives real processes, real endpoints, and real files
wherever the claim is about processes, endpoints, or files. A deadline is
proved against the value the contract declares, either on a paused runtime
clock or by waiting against the monotonic clock; nothing sleeps for a fixed
span and nothing asserts how long something took.

Where a value sits at a boundary, prove both sides: the value exactly at the
limit and the one step beyond it.

## Unchecked code

Every workspace target inherits `unsafe_code = "forbid"`. Repository-owned Rust
contains no unchecked block, function, contract, implementation, foreign block,
or attribute, and no lint allowance or documentation exempts one. A dependency
may contain unchecked code; the interface this workspace calls must be safe.

## Names and values

A declared name spells its words in full. `policy/abbreviated-identifiers.txt`
lists the shortened forms a name may not use, and a single-character name is
refused everywhere.

There is exactly one structural exception, and it is closed interface data
rather than an allowlist. `policy/external-interface-identifiers.toml` binds an
exact leading-colon fully qualified external path to the signature it dictates.
An implementation is exempt only when its header literally names that path; an
alias, a renamed import, an inferred short path, an inherent method, and a
project-owned contract that borrows the spelling are all refused. The exemption
covers that signature and nothing else: every name the body declares is still
this workspace's own.

A numeric value that carries meaning carries a name. A named constant, a named
static, an enumeration discriminant, an array literal, and an index position are
already data with a name, data laid out as data, or a position rather than a
quantity. Everything else is named.

## Size and shape

No repository-owned code file exceeds 1,000 physical lines, whether it is Rust,
a manifest, a workflow, an executable script, or a migration. No function,
script function, or migration conditional exceeds a cyclomatic complexity of
10. Both values live once, in `policy/source-policy.toml`.

A migration is code under the same contract. It is parsed with the embedded
engine's dialect, and its declared table, column, and index names are held to
the same spelling rule.

## Documentation

Documentation describes the code in the commit it ships with, not a plan for
it. Planning prose belongs in `docs/plans/`.

The checker enforces the falsifiable forms: every exported item carries
documentation that is not empty, a fallible interface says what makes it fail,
and product prose carries no marker for unfinished work and no planning
heading.

Everything else is a reader's judgement, and `policy/documentation-rules.toml`
records it as a checklist rather than pretending a checker decided it:

- The documentation describes the code in this commit, not a plan for it.
- Every contract, invariant, side effect, and bound that applies is stated.
- No comment narrates syntax the types and control flow already show.
- A comment exists wherever a constraint is not visible from the code.
- A rendered failure message names what the caller can do about it.

## Dependency direction

The crate diagram is executable. A product crate depends inward on exactly the
contracts the architecture gives it, reaches test support only through a
development edge, and never reaches the outermost tooling crate. Test support
names only the inward contracts it fakes. Storage reaches the domain agent-job
values without an edge to the wire protocol crate.

Every external dependency is one row in `policy/workspace-capabilities.toml`,
selected once with an exact version, feature set, and target condition, and
inherited by members from the workspace dependency table. A row has a consumer,
a consumer has a row, and a probe exercises the public interface each row
promises.

## Footprints

A change touches the files its task records and no others. When a task cannot
be finished inside its footprint, that is a defect in the plan: fix the plan in
its own change, and say what was wrong with it.

## Workflows

A workflow pins every nonlocal action to a full commit, declares explicit least
privilege, disables credential persistence for checkout, and never lets a
workflow expression or a value a caller controls reach a shell. Exactly one job
may hold provenance-attestation permissions beside read-only content access.

## The gate

```sh
SLINGSHOT_RUSTSEC_ADVISORY_DATABASE_DIRECTORY=<checkout> scripts/quality
```

It verifies the pinned external executables, authenticates the advisory
snapshot, and then runs `cargo fmt --all --check`, `cargo check`, `cargo
clippy` with warnings denied, `cargo test`, and `cargo doc` with warnings
denied, each over the whole workspace with every target and feature and with
the resolved graph locked, followed by script linting, the dependency
direction, the source policy, and the dependency policy. It fetches nothing and
writes nothing into the repository.
