-- A replay returns the immutable result the original maintenance transaction
-- committed, rather than attempting to infer released rows after they are gone.
ALTER TABLE maintenance_application_receipt
    ADD COLUMN released_operation_rows INTEGER NOT NULL DEFAULT 0
    CHECK (released_operation_rows >= 0);
