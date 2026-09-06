-- Durable local scheduler lease and no-return checkpoint.
ALTER TABLE operation ADD COLUMN scheduler_fence INTEGER;
ALTER TABLE operation ADD COLUMN scheduler_lease_expires_at_unix_milliseconds INTEGER;
ALTER TABLE operation ADD COLUMN scheduler_checkpoint TEXT;
