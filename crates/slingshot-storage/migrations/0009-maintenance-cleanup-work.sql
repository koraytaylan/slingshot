ALTER TABLE maintenance_application_receipt
    ADD COLUMN stage TEXT NOT NULL DEFAULT 'database_applied'
    CHECK (stage IN ('database_applied', 'completed'));

CREATE TABLE maintenance_artifact_cleanup_work (
    application_receipt_identifier TEXT NOT NULL,
    author_target_identity_digest TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    PRIMARY KEY (author_target_identity_digest, application_receipt_identifier, content_digest)
) STRICT;
