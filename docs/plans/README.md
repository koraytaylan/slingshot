# Plan authoring contract

Makina executes validated plan directories, identified by their repository-relative directory path.

## Layout

```text
docs/plans/
├── STATUS.md
└── NNNN-ASCII-Slug/
    ├── SCOPE.md
    ├── ARCHITECTURE.md
    ├── STATUS.md
    └── tasks/
        └── WWSS-task-id.md
```

## Task document contract

Each task file has closed YAML frontmatter (`id`, `title`, four-digit `workstream`, `kind`, `depends_on`, `gated`, `touches`, `status`, and `merged_as`) followed by an exact title, ordered `**Steps:**`, and one falsifiable `- **Done when:**` criterion. Filenames, IDs, workstreams, dependencies, and repository-relative mutation footprints are validated as one bundle-local DAG per plan directory. A later task that owns source inside a crate whose `Cargo.toml` another task already claims will have that crate's manifest adopted into its footprint on generation.

Before a generated blueprint is published, a critic model judges each task as a single independently testable target, a compound ticket, or a landlocked one. Compound and landlocked tickets are handed back to the author to split or to pin on the owner. The host does not count types or commas.

Task lifecycle fields, landing OIDs, plan progress/integration evidence, and the root roll-up are coordinator-owned. Volatile checkpoints live outside the repository and cannot establish completion. A committed but not yet registered bundle remains `📋 Planned` with integration state `planned` in its plan `STATUS.md` and root row; `Unregistered` is an orchestrator registration-registry state, not a valid plan integration state. Exact Phase R binds the immutable validation base, after which bundle-local dependency-ready ungated tasks become `Ready`. Working-tree-only bundles are `AwaitingCommit` in the publication workflow rather than a plan `STATUS.md` integration value.

## Sequential Phase R registration

Task `depends_on` identifiers are resolved only inside one plan directory. A cross-plan identifier is invalid, so cross-plan readiness and shared mutation ordering use this closed registration chain instead of fictional DAG edges:

1. At most one numbered bundle may be registered and not yet successfully final-integrated. Plan 0001 is the only first registration candidate and has no predecessor. Its Phase R validation-base OID must be the exact reviewed current base commit that contains the committed source-digest-matching Plan 0001 bundle, must descend from the authored base OID recorded in that plan's status, and is accepted only when no other registration is active.
2. For every Plan N greater than 0001, Phase R may register it only when Plan N-1 is `✅ Complete`, its `final integration` field contains a verified full OID, every earlier numbered plan is also complete, and no later bundle is already registered or active.
3. Plan N's Phase R validation-base OID must equal Plan N-1's verified final-integration OID exactly. Descendancy, a newer branch tip, numeric plan order by itself, or prose claiming an earlier feature is insufficient.
4. Registration refuses a missing/incomplete predecessor, a numbering gap, concurrent incomplete registration, a missing or non-full predecessor final OID, a candidate validation base unequal to that OID, any earlier plan not complete, or a root/plan status mismatch. Phase R records the accepted validation base before any task can become ready, and that base is immutable for the run.
5. Therefore all files and capabilities integrated by earlier plans—including a shared path later touched again—exist in the exact base of the next plan. Within one bundle, overlapping mutation footprints still require bundle-local `depends_on` ordering.

Historical monolithic task-list plans are inert records, not executable inputs. Do not create a fallback or mixed-format plan.
