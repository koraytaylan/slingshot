# Plan 0017 — Artifact Publication and Capacity — ✅ Complete

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both.

- **Status:** ✅ Complete.
- **Goal:** make artifact bytes publish without overwriting or substitution, survive reported-success crashes, and never escape one durable namespace capacity budget.
- **Root cause:** metadata checks are separated from replacing renames, staging/root identities are not pinned, durability errors are suppressed, and capacity reservations are unintegrated process-local helpers.
- **Approach:** use verified directory-relative no-follow objects and atomic no-replace publication, propagate and fault-test durability, then integrate a persisted shared reservation authority into storage and completion.
- **Progress:** 3/3 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run `develop`; base `develop` @ `28083543f0317caa287548090d8fb611588fe608`; mode `sequential`.
- **Exceptions:** none recorded yet.
