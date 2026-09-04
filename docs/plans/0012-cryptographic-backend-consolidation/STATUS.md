# Plan 0012 — Cryptographic Backend Consolidation — 📋 Planned

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both.

- **Status:** 📋 Planned.
- **Goal:** link one cryptographic implementation instead of two, and make that count something an assertion holds rather than something a dependency's default decides.
- **Root cause:** the assertion library's default backend named an implementation nobody chose, and the graph resolved it beside the one the transports already use. Two bodies of C and assembly, two advisory streams, and two build toolchains a release reproduces on three platforms.
- **Approach:** produce the one signature that library made directly against the backend the transports already use, delete the library, and refuse a second implementation entering the resolved graph by any path.
- **Progress:** 0/2 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run `develop`; base `main` @ `11c2e531d8a07a885321e7a09b1dccc623a733cf`; mode `sequential`.
- **Exceptions:** none recorded yet.
