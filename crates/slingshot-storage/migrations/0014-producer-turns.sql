-- Rotation belongs to the same transaction as the durable execution claim.
-- The empty key denotes the shared default producer; every explicit caller
-- identity receives a leading colon, so even an empty explicit identity cannot
-- alias the default. Production admissions accept only opaque named identities.
CREATE TABLE producer_turn (
    author_target_identity_digest TEXT NOT NULL,
    producer_key TEXT NOT NULL,
    turn_sequence INTEGER NOT NULL CHECK (turn_sequence >= 1),
    PRIMARY KEY (author_target_identity_digest, producer_key),
    UNIQUE (author_target_identity_digest, turn_sequence)
) STRICT;
