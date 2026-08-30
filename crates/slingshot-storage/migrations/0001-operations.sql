-- The first schema an operation database has.
--
-- Every row is partitioned by the opaque author-target digest. A caller picks
-- its own operation identifier, so the same identifier can legitimately name
-- different work against different targets, and the primary key says so.
--
-- No column here holds anything Plan 0002 keeps opaque. It hands over two
-- values, and those are the two that persist; everything a reader might want
-- to look up about them reaches this schema only through their digests.

CREATE TABLE installation (
    installation_identifier TEXT NOT NULL,
    recorded_at_unix_milliseconds INTEGER NOT NULL,
    singleton INTEGER NOT NULL PRIMARY KEY CHECK (singleton = 0)
) STRICT;

CREATE TABLE operation (
    author_target_identity TEXT NOT NULL,
    author_target_identity_digest TEXT NOT NULL,
    caller_identity TEXT,
    canonical_command TEXT NOT NULL,
    command_fingerprint TEXT NOT NULL,
    command_wire_name TEXT NOT NULL,
    daemon_runtime_contract_digest TEXT NOT NULL,
    enqueue_sequence INTEGER NOT NULL,
    installation_identifier TEXT NOT NULL,
    latest_progress TEXT,
    lifecycle_state TEXT NOT NULL,
    operation_identifier TEXT NOT NULL,
    operation_revision INTEGER NOT NULL CHECK (operation_revision >= 1),
    recorded_at_unix_milliseconds INTEGER NOT NULL,
    result_disposition TEXT,
    selected_environment_revision TEXT NOT NULL,
    settled_at_unix_milliseconds INTEGER,
    terminal_failure_disposition TEXT,
    terminal_failure_kind TEXT,
    terminal_failure_metadata TEXT,
    workflow_correlation_identifier TEXT,
    PRIMARY KEY (author_target_identity_digest, operation_identifier)
) STRICT;

-- Listing walks newest first inside one target, so the index is the target,
-- the sequence descending, and the identifier to break ties. A list never
-- reads a command payload, which is why the payload is not in it.
CREATE INDEX operation_by_target_and_sequence
    ON operation (author_target_identity_digest, enqueue_sequence DESC, operation_identifier);

CREATE TABLE recovery_fact (
    attempt_count INTEGER NOT NULL CHECK (attempt_count >= 0),
    author_target_identity_digest TEXT NOT NULL,
    category TEXT NOT NULL,
    detail TEXT,
    evidence_certainty TEXT,
    evidence_kind TEXT NOT NULL,
    manual_resume_eligible INTEGER NOT NULL CHECK (manual_resume_eligible IN (0, 1)),
    operation_identifier TEXT NOT NULL,
    retry_delay_milliseconds INTEGER NOT NULL CHECK (retry_delay_milliseconds >= 0),
    retry_observed_at_unix_milliseconds INTEGER NOT NULL,
    -- An unresolved category carries a certainty and a proven success carries
    -- none. The check is here as well as in the domain so a row that bypassed
    -- the domain cannot be read back as one that did not.
    CHECK ((evidence_kind = 'execution_certainty') = (evidence_certainty IS NOT NULL)),
    PRIMARY KEY (author_target_identity_digest, operation_identifier),
    FOREIGN KEY (author_target_identity_digest, operation_identifier)
        REFERENCES operation (author_target_identity_digest, operation_identifier)
        ON DELETE CASCADE
) STRICT;

CREATE TABLE recovery_resume_receipt (
    applied_operation_revision INTEGER NOT NULL CHECK (applied_operation_revision >= 1),
    author_target_identity_digest TEXT NOT NULL,
    operation_identifier TEXT NOT NULL,
    recorded_at_unix_milliseconds INTEGER NOT NULL,
    selected_environment_revision TEXT NOT NULL,
    source_fingerprint TEXT NOT NULL,
    PRIMARY KEY (author_target_identity_digest, source_fingerprint),
    FOREIGN KEY (author_target_identity_digest, operation_identifier)
        REFERENCES operation (author_target_identity_digest, operation_identifier)
        ON DELETE CASCADE
) STRICT;

CREATE TABLE artifact_blob (
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    content_digest TEXT NOT NULL PRIMARY KEY,
    recorded_at_unix_milliseconds INTEGER NOT NULL
) STRICT;

CREATE TABLE artifact_association (
    artifact_identifier TEXT NOT NULL,
    artifact_slot TEXT NOT NULL,
    author_target_identity_digest TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    content_digest TEXT NOT NULL,
    media_type TEXT NOT NULL,
    operation_identifier TEXT NOT NULL,
    PRIMARY KEY (author_target_identity_digest, operation_identifier, artifact_slot),
    FOREIGN KEY (author_target_identity_digest, operation_identifier)
        REFERENCES operation (author_target_identity_digest, operation_identifier)
        ON DELETE CASCADE,
    FOREIGN KEY (content_digest) REFERENCES artifact_blob (content_digest)
) STRICT;

CREATE TABLE maintenance_application_receipt (
    application_receipt_identifier TEXT NOT NULL,
    author_target_identity_digest TEXT NOT NULL,
    recorded_at_unix_milliseconds INTEGER NOT NULL,
    reviewed_manifest_digest TEXT NOT NULL,
    PRIMARY KEY (author_target_identity_digest, application_receipt_identifier)
) STRICT;

-- A maintenance result outlives the operation that produced it, so this table
-- references no operation at all. Its owner is either the one current preview
-- or an application receipt, and exactly one of those columns is filled.
CREATE TABLE maintenance_result_association (
    association_revision INTEGER NOT NULL CHECK (association_revision >= 1),
    author_target_identity_digest TEXT NOT NULL,
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    content_digest TEXT NOT NULL,
    is_current_preview INTEGER NOT NULL CHECK (is_current_preview IN (0, 1)),
    kind TEXT NOT NULL CHECK (kind IN ('preview', 'application')),
    maintenance_result_identifier TEXT NOT NULL,
    media_type TEXT NOT NULL CHECK (media_type = 'application/json'),
    owning_application_receipt_identifier TEXT,
    reviewed_source_digest TEXT NOT NULL,
    CHECK ((is_current_preview = 1) = (owning_application_receipt_identifier IS NULL)),
    PRIMARY KEY (author_target_identity_digest, maintenance_result_identifier),
    FOREIGN KEY (content_digest) REFERENCES artifact_blob (content_digest),
    FOREIGN KEY (author_target_identity_digest, owning_application_receipt_identifier)
        REFERENCES maintenance_application_receipt
            (author_target_identity_digest, application_receipt_identifier)
        ON DELETE CASCADE
) STRICT;

-- At most one unapplied preview per target. A partial index is the only way to
-- say "one row where this holds" without also constraining the rows where it
-- does not.
CREATE UNIQUE INDEX maintenance_current_preview_per_target
    ON maintenance_result_association (author_target_identity_digest)
    WHERE is_current_preview = 1;
