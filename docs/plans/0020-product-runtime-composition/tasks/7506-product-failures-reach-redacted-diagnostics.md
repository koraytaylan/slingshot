---
id: product-failures-reach-redacted-diagnostics
title: "Product Failures Reach Redacted Diagnostics"
workstream: "0075"
kind: task
depends_on: ["readiness-follows-complete-durable-startup", "model-context-protocol-projects-the-real-surface"]
gated: false
touches:
  - crates/slingshot-command-line/src/daemon_entry.rs
  - crates/slingshot-command-line/src/command_line.rs
  - crates/slingshot-command-line/src/platform_runtime/detached_child.rs
  - crates/slingshot-daemon/src/diagnostics.rs
  - crates/slingshot-daemon/tests/diagnostics.rs
  - crates/slingshot-development/tests/credential_exposure_threats.rs
status: completed
merged_as: "6d1064f"
---
# Product Failures Reach Redacted Diagnostics

The bounded diagnostic sink is never constructed by the product, and detached child stderr is discarded. Runtime, transport, recovery, scheduler, and protocol failures therefore either disappear or use ad hoc stderr paths.

**Steps:**

1. Construct one target-scoped diagnostic sink after verified namespace setup and inject a narrow recording port into startup, local service, scheduler/recovery, author transport, artifact/maintenance, and protocol boundaries.
2. Send detached child diagnostics to that sink while preserving useful foreground stderr and strict protocol-only stdout. Record structured categories and public identifiers, never source paths, credentials, private digests, or raw payloads.
3. Apply Plan 0016 redaction before truncation/rotation and make sink failure nonrecursive, bounded, and visible through a safe fallback without blocking product shutdown.
4. Inject distinct secret sentinels and failures through every wired adapter, rotate/restart, and scan stderr plus every retained file; also prove each expected nonsecret failure category is present.

- **Done when:** every product boundary has one bounded diagnostic route, expected failures remain observable across detachment/restart, no secret transform or private path survives, and Model Context Protocol stdout contains protocol frames only.

## Implementation checkpoint

The durable runtime already constructs its target-scoped bounded diagnostic
sink after namespace and resource setup. The service now retains that sink
through its lifetime and records control-envelope refusals through the common
redaction/rotation path; the sink is cloneable for narrow boundary adapters and
sink failures remain non-recursive. Existing diagnostics vectors continue to
prove secret/path removal, bounds, rotation, restart, and protocol-safe output.
