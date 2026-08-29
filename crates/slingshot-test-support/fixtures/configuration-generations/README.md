# Committed configuration generations

Each directory is one configuration root exactly as a writer would leave it,
including the `configuration-snapshot.toml` that commits it. The digests are
computed from the bytes beside them, so a change to a source without a change to
the inventory is the mixed generation the coordinator has to refuse.

- `complete` is one whole committed generation.
- `missing-source` lists a source the tree does not hold.
- `surplus-profile` holds a profile the inventory does not list.
- `digest-mismatch` lists a digest the source does not produce.
