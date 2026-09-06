# Plan 0020 — Product Runtime Composition — 🚧 In Progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both.

- **Status:** 🚧 In progress.
- **Goal:** make the compiled `slingshot` daemon and Model Context Protocol server reach the implemented command, storage, recovery, artifact, and author-transport behavior through one auditable composition root.
- **Root cause:** production entry points stop at ping/stop and empty protocol payloads while successful tests assemble deeper modules directly, leaving no real author port, durable startup, scheduler claim, diagnostics wiring, or process proof.
- **Approach:** implement the author adapter, construct one immutable runtime after hardened startup, add versioned dispatch and fenced scheduling, project the real catalogs into Model Context Protocol, wire diagnostics, and verify the compiled processes end to end.
- **Progress:** 2/7 tasks done; 0 blocked; 0 dropped. Selected author transport and complete durable startup are installed; versioned dispatch is next.
- **Integration:** `planned`; run `develop`; base `develop` @ `28083543f0317caa287548090d8fb611588fe608`; mode `sequential`.
- **Exceptions:** none recorded yet.
