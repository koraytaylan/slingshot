CREATE TABLE operation (
    identifier TEXT PRIMARY KEY,
    operation_state TEXT NOT NULL
) STRICT;

CREATE INDEX operation_state_index ON operation (operation_state);
