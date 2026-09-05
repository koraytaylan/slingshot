-- Each command declares at most one remote artifact. Its first acquisition
-- start is immutable across retries and is removed with the existing child.
ALTER TABLE agent_operation ADD COLUMN acquisition_artifact_identifier TEXT;
ALTER TABLE agent_operation ADD COLUMN acquisition_artifact_slot TEXT;
ALTER TABLE agent_operation ADD COLUMN acquisition_content_digest TEXT;
ALTER TABLE agent_operation ADD COLUMN acquisition_started_at_unix_milliseconds INTEGER
    CHECK (
        (acquisition_started_at_unix_milliseconds IS NULL
         AND acquisition_artifact_identifier IS NULL
         AND acquisition_artifact_slot IS NULL
         AND acquisition_content_digest IS NULL)
        OR
        (acquisition_started_at_unix_milliseconds IS NOT NULL
         AND acquisition_started_at_unix_milliseconds >= 0
         AND acquisition_artifact_identifier IS NOT NULL
         AND length(acquisition_artifact_identifier) = 64
         AND acquisition_artifact_identifier NOT GLOB '*[^0-9a-f]*'
         AND acquisition_artifact_slot IS NOT NULL
         AND acquisition_artifact_slot IN ('content_package', 'loaded_content_json')
         AND acquisition_content_digest IS NOT NULL
         AND length(acquisition_content_digest) = 64
         AND acquisition_content_digest NOT GLOB '*[^0-9a-f]*')
    );
