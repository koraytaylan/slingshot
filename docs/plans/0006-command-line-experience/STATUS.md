# Plan 0006 — Command-Line Experience — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.
- **Goal:** expose the complete Slingshot operation registry through a deterministic command surface with namespaced daemon startup, durable detachment, stable output, and process-level proof.
- **Root cause:** operators and automation otherwise need to construct local protocol requests themselves, and long-running jobs become unsafe when command parsing, daemon lifecycle, stream routing, and interruption behavior are implicit.
- **Approach:** build a side-effect-free command tree, authenticate daemon-runtime, author-agent-transport, and canonical-JSON/`1.0.0`/limits/schema provenance, validate raw bytes before decoded shape and typed conversion, externalize complete over-inline maintenance bytes under an operation-free target-qualified association/URI without truncation, authenticate caller digest and read-start facts through target-and-identifier metadata lookup, make operation-artifact or maintenance-result publication the success commit, centralize daemon observation and lossless structured-failure rendering, and pin compiled-process ownership with exhaustive golden sessions and secret scans.
- **Progress:** 3/20 tasks done; 0 blocked; 0 dropped.
- **Integration:** `in progress`; run `develop`; base `main` @ `8d286e88c06f91a1513834a4839ae36582212242`; validation base `95f1e298dbac69e2a37e48d338409fd7c1cf74d5`; mode `sequential`; final integration —.
- **Exceptions:**
  - **2502 needed the configuration crate the contract already allowed it.** Resolving a
    target is Plan 0002's selector and its closed diagnostic vocabulary; the dependency
    table already permitted the edge and the manifest simply did not have it yet, so the
    manifest joins this task's footprint.
  - **2501 needed the registry it is specified to consult.** The task requires an
    operation key for every command the catalog classifies as not intrinsically
    idempotent, and the command-line crate had no edge to the domain that publishes that
    classification. Restating the classification here would have created a second list to
    keep in step with the first, so the crate takes the inward dependency and its manifest
    joins the footprint.
    It also relaxes the scaffold suite's emptiness check, which was true of the commit
    that created the leaves and stops being true of the first one to implement any of
    them; what stays checked is the enduring claim, that every leaf opens with
    documentation of what it owns and claims nothing it has not done.
  - **2505 had to stop the formatter reordering its declarations.** The task fixes the
    command family's declaration order, and `rustfmt` sorts module declarations
    alphabetically by default, so the order could not survive the formatter the gate runs.
    `reorder_modules = false` joins `rustfmt.toml` and the task's footprint. It reformats
    nothing that exists: every other declaration list in the workspace is already
    alphabetical, and stays so because that is how it was written rather than because a
    tool insists.
- **Outcome:** Every exact-version/limits-bound catalog operation is available from the command line with canonical phrase/asset/token inputs, reaches one profile/environment daemon, survives detachment or interrupt, exposes complete maintenance results through exact target-qualified operation-free identities, and renders every registered closed result or failure byte-stably without aliases, lost fields, or secret exposure.

_Last updated: 2026-08-29, against `main` @ `8d286e8`._
