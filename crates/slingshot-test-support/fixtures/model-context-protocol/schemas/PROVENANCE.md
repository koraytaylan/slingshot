# Projected schemas

One row per tool, naming the digest of the input schema it declares and the
digest of the output schema it answers under. The schemas themselves are
projected from the registry at build time; what is committed here is what they
digest to, so a projection that changes without anybody choosing the change is
a failing test rather than a client discovering it.

The digests are written by the suite that reads them, under
`SLINGSHOT_REVIEW_PROJECTED_SCHEMAS=1`.
