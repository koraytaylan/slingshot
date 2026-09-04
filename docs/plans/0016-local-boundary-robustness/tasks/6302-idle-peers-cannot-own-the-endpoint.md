---
id: idle-peers-cannot-own-the-endpoint
title: "Idle Peers Cannot Own The Endpoint"
workstream: "0063"
kind: task
depends_on: ["one-frame-reader-lives-with-the-connection"]
gated: false
touches:
  - support/foundation-contract.toml
  - crates/slingshot-daemon/src/local_server.rs
  - crates/slingshot-daemon/tests/local_server.rs
  - crates/slingshot-development/tests/local_endpoint_threats.rs
status: planned
merged_as: ""
---
# Idle Peers Cannot Own The Endpoint

After one complete response, an empty connection has no deadline but continues to hold one of sixty-four permits. Sixty-four same-user peers can ping once, idle forever, and prevent every legitimate connection from being accepted.

**Steps:**

1. Declare a quiescent post-response lease and close or release general request capacity when it expires; exact lease and one-unit boundary behavior comes from the foundation contract.
2. Give explicit operation-wait streams a separately bounded pool or lease tied to an active waiter, so supporting waits does not make every ping socket immortal.
3. Hold capacity per active request or otherwise prove a quiescent connection cannot retain all general admission permits; preserve shutdown cancellation and current-user endpoint policy.
4. Fill the real endpoint with the maximum ping-then-idle clients, wait through the lease, and prove a legitimate ping succeeds. Separately prove valid waits function within their bound and arbitrary idle peers cannot enter that class.

- **Done when:** no set of post-response idle peers can starve general local service indefinitely, while an authenticated active wait retains only its declared bounded resource and shuts down cleanly.
