-- A recovery receipt belongs to the operation whose resumability it proved.
-- Source fingerprints were formerly target-wide, allowing two operations with
-- the same command-derived source to replay one another's receipt.
CREATE TABLE recovery_resume_receipt_new (
    applied_operation_revision INTEGER NOT NULL CHECK (applied_operation_revision >= 1),
    author_target_identity_digest TEXT NOT NULL,
    operation_identifier TEXT NOT NULL,
    recorded_at_unix_milliseconds INTEGER NOT NULL,
    selected_environment_revision TEXT NOT NULL,
    source_fingerprint TEXT NOT NULL,
    PRIMARY KEY (author_target_identity_digest, operation_identifier, source_fingerprint),
    FOREIGN KEY (author_target_identity_digest, operation_identifier)
        REFERENCES operation (author_target_identity_digest, operation_identifier)
        ON DELETE CASCADE
) STRICT;

INSERT INTO recovery_resume_receipt_new
    (applied_operation_revision, author_target_identity_digest, operation_identifier,
     recorded_at_unix_milliseconds, selected_environment_revision, source_fingerprint)
SELECT applied_operation_revision, author_target_identity_digest, operation_identifier,
       recorded_at_unix_milliseconds, selected_environment_revision, source_fingerprint
FROM recovery_resume_receipt;

DROP TABLE recovery_resume_receipt;
ALTER TABLE recovery_resume_receipt_new RENAME TO recovery_resume_receipt;
