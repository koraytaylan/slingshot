---
id: cache-cloud-access-tokens
title: "Cache Cloud Access Tokens"
workstream: "0008"
kind: task
depends_on:
  - provide-environment-authentication
gated: false
touches:
  - crates/slingshot-agent-connection/src/authentication/access_token_cache.rs
  - crates/slingshot-agent-connection/src/authentication/environment_provider.rs
  - crates/slingshot-agent-connection/tests/access_token_cache.rs
status: planned
merged_as: ""
---
# Cache Cloud Access Tokens

Cache Cloud access tokens behind the provider with per-snapshot single-flight scheduled refresh and race-safe conditional forced refresh after an unauthorized response.

**Steps:**

1. Create a schedule fixture for fresh, exactly refresh-skew, one millisecond on either side, expired, maximum lifetime, delayed response transfer leaving one below/equal/one above refresh-skew-plus-minimum-usable-lease at body receipt, monotonic overflow, checked installation-generation exhaustion, concurrent scheduled refresh, scheduled-refresh/unauthorized overlap, concurrent unauthorized responses, stale unauthorized lease after a newer install, stale lease while that newer generation is unusable, forced-refresh failure/cancellation/retry, provider-snapshot identity change, eviction, and forward/backward UTC wall-clock movement cases.
2. Implement the in-memory cache with injected monotonic time, checked monotonic deadline from exchange, the shared named refresh skew and minimum usable lease, and one refresh flight per process-random opaque provider-snapshot identity, plus atomic replacement, waiter wakeup, and no failure caching. A typed too-short exchange failure installs nothing, reaches every waiter, releases the flight, and never recursively opens another exchange; accept no wall-clock or secret-derived value in cache identity/freshness decisions.
3. Return Cloud authentication through an opaque `AccessTokenLease` carrying a cache-internal checked installation generation with no display, serialization, persistence, equality-to-secret, token-byte derivation, or wrapping. Add provider operation `refresh_after_unauthorized(lease)` with the four synchronized branches from architecture: invalidate only an equal usable generation; return a different usable current generation; join a flight for an unusable current generation; or retry an unusable generation after a released failed/cancelled flight.
4. Use one cell flight for scheduled and forced refresh. Mark an equal unauthorized generation unusable before joining an already scheduled flight or starting one exchange. A failed or cancelled flight wakes all waiters, leaves the rejected generation unusable, releases the flight, and permits a later retry; no caller may fall back to the rejected token.
5. Run concurrency tests under controlled barriers and assert exact exchange-call counts, lease generations, stale-invalidation no-ops, waiter outcomes, and token zeroization on replacement/invalidation.

**Tests:**

- `access_token_cache` proves one exchange for concurrent callers and deterministic refresh at each millisecond clock boundary, with equality classified as refresh-required.
- Concurrent callers receiving a below/equal usable-lease response observe one shared `AccessTokenLifetimeTooShort` failure, zero installed leases, and exactly one exchange; one millisecond above the threshold installs normally.
- Two or more unauthorized responses carrying the same lease invalidate that generation once and join exactly one forced-refresh exchange; every waiter receives the same newer lease.
- An unauthorized response carrying a stale lease after scheduled or forced replacement cannot evict the newer token, cannot open another exchange, and returns the current newer lease.
- An unauthorized response overlapping an existing scheduled flight joins that flight after rejecting its equal old generation; a stale lease arriving while a newer generation is already unusable joins that generation's flight or retry and never returns the rejected token.
- Scheduled and forced failure/cancellation cases prove flights release, waiters wake, the rejected generation remains unusable, and the next caller can retry without receiving the rejected token.
- Arbitrary UTC wall-clock changes after receipt leave the exact refresh decision and monotonic boundary unchanged.
- Callers sharing one injected provider-snapshot identity share one flight; distinct snapshot identities remain separate even with byte-identical credentials, and changing secret bytes cannot determine either identity.
- Compile-time and scan fixtures prove `AccessTokenCacheIdentity`, `AccessTokenLease`, and its installation generation cannot be displayed, serialized, persisted, compared with, or derived from a `SecretValue`.

- **Done when:** `cargo test -p slingshot-agent-connection --test access_token_cache` proves concurrent callers of one opaque provider snapshot share one scheduled or conditional unauthorized refresh across every synchronized state branch, too-short returned lifetimes fail one flight without installation or looping, a stale lease cannot evict a newer token or return an unusable one, generations never wrap, rejected generations never return after invalidation, distinct snapshots remain isolated without secret-derived identity, and deadline/skew decisions remain independent of UTC wall-clock movement.
