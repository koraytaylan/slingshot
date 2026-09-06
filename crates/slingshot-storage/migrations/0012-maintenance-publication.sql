-- Maintenance documents have no operation/slot owner. Retain their full
-- operation-free derivation beside the shared content-protection record.
CREATE TABLE maintenance_publication (
    publication_identifier TEXT NOT NULL PRIMARY KEY,
    author_target_identity_digest TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('preview', 'application')),
    reviewed_source_digest TEXT NOT NULL,
    FOREIGN KEY (publication_identifier) REFERENCES artifact_publication (publication_identifier)
        ON DELETE CASCADE
) STRICT;
