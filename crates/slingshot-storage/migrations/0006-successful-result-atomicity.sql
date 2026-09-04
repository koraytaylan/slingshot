-- A successful operation has one complete immutable result in the same row
-- transition that makes it terminal. Existing torn successes deliberately
-- abort this migration when the validation update below reaches them.
ALTER TABLE operation ADD COLUMN result_inline_bytes TEXT;

CREATE TRIGGER operation_result_shape_on_insert
BEFORE INSERT ON operation
WHEN (NEW.lifecycle_state = 'succeeded' AND NOT (
        (NEW.result_disposition = 'inline' AND NEW.result_inline_bytes IS NOT NULL)
        OR (NEW.result_disposition = 'artifact' AND NEW.result_inline_bytes IS NULL)
    ))
    OR (NEW.lifecycle_state != 'succeeded' AND
        (NEW.result_disposition IS NOT NULL OR NEW.result_inline_bytes IS NOT NULL))
BEGIN
    SELECT RAISE(ABORT, 'operation result does not match lifecycle');
END;

CREATE TRIGGER operation_result_shape_on_update
BEFORE UPDATE ON operation
WHEN (NEW.lifecycle_state = 'succeeded' AND NOT (
        (NEW.result_disposition = 'inline' AND NEW.result_inline_bytes IS NOT NULL)
        OR (NEW.result_disposition = 'artifact' AND NEW.result_inline_bytes IS NULL)
    ))
    OR (NEW.lifecycle_state != 'succeeded' AND
        (NEW.result_disposition IS NOT NULL OR NEW.result_inline_bytes IS NOT NULL))
BEGIN
    SELECT RAISE(ABORT, 'operation result does not match lifecycle');
END;

-- Run the persistent update trigger over pre-existing rows. A success without
-- a disposition, or any result on a non-success, is evidence of a torn prior
-- write and must be repaired explicitly rather than blessed by this migration.
UPDATE operation SET lifecycle_state = lifecycle_state;
