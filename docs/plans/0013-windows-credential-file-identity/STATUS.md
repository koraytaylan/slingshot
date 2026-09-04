# Plan 0013 — Windows Credential File Identity — 📋 Planned

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both.

- **Status:** 📋 Planned.
- **Goal:** make the Windows row a row that compiles, by reading a credential's identity through a stable interface from the handle the rest of the check already holds.
- **Root cause:** the credential filesystem reads identity through four unstable standard-library interfaces. The workspace is pinned to a released compiler, so the row has never built, and the release matrix claims a target no build has ever produced evidence for.
- **Approach:** settle which handle the row uses, read identity through an interface that exists, prove the identity and the content and the security decision describe one object, and decide what the second time is on a row that has no stable source for it.
- **Progress:** 0/2 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run `develop`; base `main` @ `11c2e531d8a07a885321e7a09b1dccc623a733cf`; mode `sequential`.
- **Exceptions:** none recorded yet.
