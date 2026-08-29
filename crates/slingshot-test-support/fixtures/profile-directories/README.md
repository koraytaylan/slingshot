# Profile directories

Each directory is one committed configuration root, inventory included.

- `ordered` names its files in the opposite order to the names its documents
  declare, and holds one opted-in cleartext installation profile, so a loader
  that ordered by file name, by enumeration order, or by anything other than the
  declared name would produce a different result here.
- `duplicate-name` holds two profiles declaring one name, which no root may.
