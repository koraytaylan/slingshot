# Plan 0021 — Child Listings

## Architectural boundary

The three child listings are command-family leaves beside the page listing they
extend. `list_child_nodes` declares the general read, `list_child_nodes_by_type`
narrows it to one primary type, and `list_child_pages` is that narrowing with the
type a page has. The registry, the schemas, the byte contract's array inventory,
and the projection into the protocol server are all derived from these
declarations rather than written again beside them.

## What proves what

A listing is proved by what it admits. Every match is required to be an immediate
child of the anchor and, for a typed listing, to carry the type the request named;
a grandchild and a non-matching child are proved not to match, against fixtures
that contain both. The page listing keeps its own wire name and result shape, so a
caller that already reads pages reads the same document after this plan as before
it.

The registry's closed table is what decides safety: all three rows are reads that
are intrinsically idempotent, so each refuses an operation key and each projects a
read-only tool. The committed catalog and schema manifest are regenerated from the
same declarations and compared byte for byte, so a schema that disagrees with its
row fails the gate rather than being noticed later.

## What stays outside

No new failure category and no new bound is added. The two listings report the
failures `list_child_pages` already reported, and the agent that carries them out
is built in a separate repository.
