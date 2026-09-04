# Plan 0012 — Cryptographic Backend Consolidation

## Architectural boundary

The change is confined to the crate that builds credential assertions. No contract moves: the assertion's claims, its encoding, its clock sampling, and the exchange it is presented to are all fixed by Plan 0002 and stay exactly as they are. What changes is which library computes one signature.

The capability inventory records the removal as it records every dependency change: the assertion library's row leaves, and no row is added, because the backend that replaces it is already a row this workspace declares and probes for its transports.

## What proves what

The assertion fixtures are the arbiter. They pin the exact bytes this product signs for a given key and a given sampled instant, which is precisely the property a change of implementation could break and nothing above the signature could detect. A signature that differs is a defect regardless of whether the exchange still accepts it, because the fixtures are what make the product's behaviour reproducible.

The count of cryptographic implementations is decided by reading the resolved graph rather than by reading the manifest. A manifest says what was asked for; the graph says what a build actually links, and a default feature on a dependency of a dependency is exactly how the second one arrived the first time.

## What stays outside

The embedded database's own C compilation is untouched and stays a deliberate, separately pinned dependency. This plan makes no claim about constant-time behaviour, side channels, or algorithm choice beyond keeping the ones already in use; it removes a duplicate, and says so.
