CREATE TABLE artifact_reservation (
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    ticket INTEGER PRIMARY KEY
) STRICT;
