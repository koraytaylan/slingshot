---
id: readiness-follows-complete-durable-startup
title: "Readiness Follows Complete Durable Startup"
workstream: "0075"
kind: task
depends_on: ["the-product-has-one-author-port"]
gated: false
touches:
  - crates/slingshot-command-line/src/daemon_entry.rs
  - crates/slingshot-daemon/src/startup.rs
  - crates/slingshot-daemon/src/service.rs
  - crates/slingshot-development/tests/operation_executor_composition.rs
status: planned
merged_as: ""
---
# Readiness Follows Complete Durable Startup

The product currently publishes readiness after only endpoint bind and a ping/stop service. It never establishes installation/database state, audits the target, installs an executor, or recovers unfinished work.

**Steps:**

1. Introduce one product runtime builder that consumes the already resolved immutable profile/target/trust snapshot and owns all storage, repository, artifact, transport, executor, recovery, maintenance, cancellation, and diagnostic lifetimes.
2. Run hardened installation/database establishment and foreign-state audit under ownership before constructing execution. Refuse wrong target/revision/contract or unfinished foreign state without publishing readiness.
3. Recover durable local/outbox/artifact/maintenance work and install the concrete author adapter before binding the operation plane.
4. Publish readiness only after every required component is usable and advertise exactly the installed protocol versions; unwind endpoint/readiness/leases in reverse order on every startup failure.

- **Done when:** a ready compiled daemon has passed complete durable startup and installed execution, every injected startup/audit/recovery failure leaves no readiness record, and restart reconstructs the same target-bound runtime without direct test composition.
