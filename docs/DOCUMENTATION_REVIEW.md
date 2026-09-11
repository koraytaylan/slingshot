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

## Reviewed document identities

This review applies only to the complete product documents named below. Each
identity is the SHA-256 digest of the document bytes reviewed; a changed byte
cannot inherit this review without an explicit renewed record.

| Document | SHA-256 |
|---|---|
| `README.md` | `bdf7f219dd17955eac0b8c8c807c4c60bed343a79c3715b1ff984a9abdbe5ad5` |
| `CONTRIBUTING.md` | `97e3f1a0bd8723865cc4b858606e3a25dafe75d18039e99d49a2eafde2c7be0d` |
| `ARCHITECTURE.md` | `acaa53946f9299d0f11f32a67969ef2c77e7bcbe93955c34d225cc3b63e4b7aa` |
| `docs/AGENT_PROTOCOL.md` | `ece5eae8453303b299635ede39026267888f13749ca718f0a1a528f4ef738806` |
| `docs/COMMANDS.md` | `dfe0c21d59a198443978220b88e17eb7658df943c649c90d265ab7c58e8b0f5e` |
| `docs/CONFIGURATION.md` | `775ce5363790d1b44a91fdb7e7b2015d538cce224de5030f5e13edec87b089b3` |
| `docs/DAEMON.md` | `4d1f7748636d8e0a44195755ab1d8492c561f6c3dedeb5c88e95ebd9ed7df6d6` |
| `docs/MODEL_CONTEXT_PROTOCOL.md` | `0e7de936b9f044b95131aeccf576bd8724f806bf37b661581e5a5f1b9fcaeae9` |
| `docs/RELEASES.md` | `c25a1800a6e20515d29569eec77003a267184a35e88d2a1c21d24630f0501d6a` |
| `docs/WORKFLOWS.md` | `0bd107a373f258069f2e9472f5c17bea2ccb675aa070ccc581d49ce11c9f3f6e` |
