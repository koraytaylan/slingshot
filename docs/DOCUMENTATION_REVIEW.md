# Documentation review

The source-policy command decides the falsifiable parts of documentation: that
an exported item carries some, that a function which can fail names what makes
it fail, that a function which can end the process says when, that no
unfinished-work marker or planning heading is left in product prose, and that
no marker switches a rule off. It decides nothing about whether the prose is
true, complete, or worth reading. Those are judgements, and this is where the
judgements are recorded.

The four subjects below are the closed inventory in
[`policy/documentation-rules.toml`](../policy/documentation-rules.toml). Each is
covered by exactly one checklist entry, and each answer here is a reviewer's,
not a checker's.

## Public contract and failure coverage

*Every contract, invariant, side effect, and bound that applies is stated.*

Reviewed across the workspace. Every public fallible function carries an
`# Errors` section naming which refusal it returns and what distinguishes it
from its neighbours, and the command surface states its bounds by asking the
command contract for them rather than by restating them. Two places had stated
a bound twice: the artifact store, which now reads both artifact bounds from the
contract, and the release input cache, which reads what a Cargo home may be from
the compatibility manifest instead of declaring it again.

## Non-obvious invariant comments

*A comment exists wherever a constraint is not visible from the code.*

Reviewed. The constraints that are not visible from the code are the ones about
ordering and about what a check does not establish: that the canonical-contract
annotation is authenticated before a role digest is believed, that a seed's
first violation in one traversal order is the diagnostic, that verifying a
prepared cache says nothing about whether its bytes were trustworthy when they
were fetched, and that idempotency is never read as an access decision. Each is
stated where the code depends on it.

## Non-narration

*No comment narrates syntax the types and control flow already show.*

Reviewed. Comments that restated a signature were removed as they were found;
what remains says why rather than what. The checker deliberately accepts
narrating prose, and a fixture proves it does, so this subject stays a
judgement rather than becoming a rule that would reward deleting comments.

## Present factual prose

*The documentation describes the code in this commit, not a plan for it.*

Reviewed. Product documentation describes the build it ships with: the command
reference is generated from the metadata the executable reads and compared
byte for byte, and the two protocol documents carry generated sections beside
hand-written prose. Prospective language belongs to the plan bundles under
`docs/plans/`, which the scan structurally excludes for exactly that reason.

Reviewed again after the command surface grew from twelve rows to sixty-four.
Three prose statements had counted the old surface and were corrected rather
than left to be discovered: the readme's live-author paragraph, which had said
nine rows were admissible and three were refused; the architecture note's
account of what is not here, which had said no Adobe Experience Manager
operation exists when sixty-four contracts do; and the live-author leaf's own
documentation, which had explained why three of nine submissions are enough.
Everything else that names a command is generated from the registry and
compared byte for byte, so no other document could have drifted without the
gate saying so.

