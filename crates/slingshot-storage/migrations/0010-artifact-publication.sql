-- Content pending the file/database handoff remains charged and protected.
-- One producer must not consume another producer's protection for shared bytes.
CREATE TABLE artifact_publication (
    publication_identifier TEXT NOT NULL PRIMARY KEY,
    artifact_identifier TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    recorded_at_unix_milliseconds INTEGER NOT NULL,
    FOREIGN KEY (content_digest) REFERENCES artifact_blob (content_digest)
) STRICT;
