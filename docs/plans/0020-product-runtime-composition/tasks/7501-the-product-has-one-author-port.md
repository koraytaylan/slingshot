---
id: the-product-has-one-author-port
title: "The Product Has One Author Port"
workstream: "0075"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-agent-connection/src/lib.rs
  - crates/slingshot-daemon/src/author_agent_operation_executor.rs
  - crates/slingshot-daemon/tests/author_agent_operation_executor.rs
  - crates/slingshot-daemon/tests/author_agent_conformance.rs
status: planned
merged_as: ""
---
# The Product Has One Author Port

`AuthorPorts` is the executor boundary, but only tests implement it. The agent-connection crate publishes structures and dependencies without a product connection, so no compiled command can reach an author service.

**Steps:**

1. Implement one product `AuthorPorts` adapter over the exact selected author endpoint, immutable target identity, verified author trust policy, bounded DNS/connect/TLS/request/response phases, and protocol codec already specified by Plan 0005.
2. Reject proxies, publisher endpoints, redirects outside the selected origin, ambient trust, trust-policy reload, response compression/framing ambiguity, and any command or result identity mismatch before returning evidence to the executor.
3. Preserve request-start retention, logical outbox/fence, reconnect, uncertainty, and terminal receipt distinctions without logging credentials or private identity material.
4. Drive the concrete adapter against a protocol-faithful fake author for every success/refusal/uncertain phase, including hostile additional CA, redirected origin, duplicate physical record, truncated response, restart, and cancellation.

- **Done when:** the concrete product adapter satisfies the complete existing agent conformance suite and the executor has no test-only or alternate path for network operations.
