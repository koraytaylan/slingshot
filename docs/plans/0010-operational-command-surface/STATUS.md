# Plan 0010 — Operational Command Surface — 📋 Planned

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 📋 Planned.
- **Goal:** publish sixty-four commands instead of twelve, so that page and component lifecycle, asset lifecycle, content and experience fragments, Open Service Gateway Initiative platform state, resource mapping and resolution, workflow management, Sling job management, authorizable administration, and replication queue control are each one bounded typed command rather than a browser tab.
- **Root cause:** twelve rows cover one console. Half of them are one side of a pair whose other side does not exist - a page can be created and never changed or removed, an asset can be searched and never written, a configuration can be read one identifier at a time and never listed, content can be offered to replication and the queue that accepted it can never be examined - and every operational question outside that console leaves the executable entirely.
- **Approach:** land the operational limits and the six validated vocabulary leaves first; then one independently tested command contract per operation, each with canonical fixtures, boundary proofs at and one step past every bound, exact no-effect failure documents, and language-neutral agent-conformance scenarios; then one sixty-four-row registry in ascending wire-name order under an access definition widened from repository content to any state the author retains, both role schemas with committed bytes and digests, the command-line builders and options that construct every new command, and the rendered reference and compatibility surfaces that are checked against the registry rather than written beside it.
- **Progress:** 0/60 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run `develop`; base `main` @ `11c2e531d8a07a885321e7a09b1dccc623a733cf`; validation base `e1152a1b1cf8a78877b3a0f372e7635779bd85e4`; mode `sequential`; final integration `pending`.
- **Exceptions:**
  - **4001's recorded footprint named the manifest and its test and nothing that records the manifest's digest.** Three committed documents record that digest rather than compute it - the catalog fixture, the schema manifest, and the protocol compatibility snapshot - so extending the manifest leaves all three disagreeing with the build, and the gate refuses the tree until they are regenerated. They are the task's own consequence and belong in its footprint, so the footprint was corrected in its own change before the task landed.
- **Outcome:** pending.

_Last updated: 2026-08-31, against `develop` @ `e1152a1b1cf8a78877b3a0f372e7635779bd85e4`._
