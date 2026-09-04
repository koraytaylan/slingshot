# Plan 0014 — Release Acceptance Decision — 📋 Planned

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both.

- **Status:** 📋 Planned.
- **Goal:** make the releasability decision something that runs and records what it decided, so a release can say it was decided.
- **Root cause:** the command the acceptance invokes was never written. The verifier that reads its manifest exists, the isolation it runs in exists and works, and the producer between them does not.
- **Approach:** write the command that runs the gates inside the boundary and produces the manifest already specified, and admit into the closed environment exactly the facts about the run the manifest binds, each one recorded with its reason.
- **Progress:** 0/2 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run `develop`; base `main` @ `11c2e531d8a07a885321e7a09b1dccc623a733cf`; mode `sequential`.
- **Exceptions:** task 5901's authored footprint omitted `.github/workflows/release.yml`. The acceptance job declares no `SLINGSHOT_REPORTED_WORKFLOW`, so the provider run the manifest binds could not reach the container from inside the footprint as authored. The workflow was added to that task's footprint before any of it was written.
