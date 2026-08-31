# Protocol compatibility snapshots

What a consumer would break on if it changed. Each row is a value some other
program has already read: a wire name, a code, a tag, a revision. A change here
is not necessarily wrong, but it is never accidental - the snapshot exists so
the decision is made deliberately and written down in the same commit.

The snapshot is rewritten with `SLINGSHOT_REVIEW_PROTOCOL_COMPATIBILITY=1`.
