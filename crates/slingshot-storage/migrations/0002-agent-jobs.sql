-- What this daemon durably knows about work the agent is running.
--
-- Two subjects live here and they are deliberately not joined. One is the
-- remote submission: what was sent, under which contracts, to which target, and
-- what has become of it. The other is the subscription ledger: where one filtered
-- event stream has got to. A stream carries events about work this daemon does
-- not hold, so its position must be recordable without a job to hang it on, and
-- a foreign key from the ledger to a job would make the honest case impossible.
--
-- Every row is partitioned by the opaque author-target digest, like Plan 0004's.
-- The same local operation against two targets is two submissions, and the same
-- subscription name against two targets is two streams.

-- One remote submission, with every value a resubmission would have to derive.
--
-- The contract columns are not decoration. A restart re-derives the identity
-- from what this build has and compares it with what is stored; a build whose
-- schemas, limits, version, wire name, byte contract, or transport contract
-- moved is a build that must not resume somebody else's submission under its
-- own name.
CREATE TABLE agent_operation (
    agent_event_store_generation INTEGER NOT NULL CHECK (agent_event_store_generation >= 1),
    agent_operation_identifier TEXT NOT NULL,
    applied_sequence INTEGER NOT NULL CHECK (applied_sequence >= 1),
    argument_schema_digest TEXT NOT NULL,
    attempt INTEGER NOT NULL CHECK (attempt >= 0),
    author_agent_transport_contract_digest TEXT NOT NULL,
    author_target_identity_digest TEXT NOT NULL,
    canonical_submission TEXT NOT NULL,
    command_canonical_json_contract_digest TEXT NOT NULL,
    command_contract_limits_digest TEXT NOT NULL,
    command_semantic_contract_version TEXT NOT NULL,
    command_wire_name TEXT NOT NULL,
    daemon_subscription_identifier TEXT NOT NULL,
    job_state TEXT NOT NULL CHECK (job_state IN ('queued', 'running', 'succeeded', 'failed')),
    operation_identifier TEXT NOT NULL,
    progress INTEGER NOT NULL CHECK (progress >= 0),
    recorded_at_unix_milliseconds INTEGER NOT NULL,
    remaining_retention_milliseconds INTEGER NOT NULL CHECK (remaining_retention_milliseconds >= 0),
    request_start_unix_milliseconds INTEGER NOT NULL,
    result_schema_digest TEXT NOT NULL,
    selected_environment_revision TEXT NOT NULL,
    snapshot_watermark INTEGER NOT NULL CHECK (snapshot_watermark >= 0),
    submitted_command_digest TEXT NOT NULL,
    -- A terminal disposition is present exactly when the job has ended, so a
    -- reader never has to decide which of two columns to believe.
    terminal_disposition TEXT,
    CHECK (
        (job_state IN ('succeeded', 'failed')) = (terminal_disposition IS NOT NULL)
    ),
    PRIMARY KEY (author_target_identity_digest, agent_operation_identifier)
) STRICT;

-- One local operation submits once per target. The agent identifier is derived
-- from the local one, so this says the derivation is not to be worked around.
CREATE UNIQUE INDEX agent_operation_by_local_identifier
    ON agent_operation (author_target_identity_digest, operation_identifier);

-- Recovery asks "what did I submit under this digest", so that is an index.
CREATE INDEX agent_operation_by_submitted_digest
    ON agent_operation (author_target_identity_digest, submitted_command_digest);

-- The physical Sling jobs carrying one logical submission.
--
-- Several rows per submission is ordinary rather than exceptional: Sling
-- delivers at least once, and a requeue is another physical record for the same
-- work. None of them authorizes a second effect, which is why this table holds
-- names and nothing else - there is no state here for a second record to move.
CREATE TABLE agent_physical_job (
    agent_operation_identifier TEXT NOT NULL,
    author_target_identity_digest TEXT NOT NULL,
    recorded_at_unix_milliseconds INTEGER NOT NULL,
    sling_job_identifier TEXT NOT NULL CHECK (length(sling_job_identifier) > 0),
    PRIMARY KEY (
        author_target_identity_digest, agent_operation_identifier, sling_job_identifier
    ),
    FOREIGN KEY (author_target_identity_digest, agent_operation_identifier)
        REFERENCES agent_operation (author_target_identity_digest, agent_operation_identifier)
        ON DELETE CASCADE
) STRICT;

-- Where one filtered stream has got to, and what it has cost.
--
-- No foreign key to any job, on purpose. The cursor advances on events about
-- work this daemon does not hold, and a constraint requiring a job would either
-- block those or invent a row to satisfy itself.
CREATE TABLE subscription_ledger (
    agent_event_store_generation INTEGER NOT NULL CHECK (agent_event_store_generation >= 1),
    author_target_identity_digest TEXT NOT NULL,
    canonical_digest TEXT,
    -- The position everything below has been compacted away beneath.
    compacted_below_cursor TEXT,
    cursor TEXT,
    daemon_subscription_identifier TEXT NOT NULL,
    event_bytes INTEGER NOT NULL CHECK (event_bytes >= 0),
    event_rows INTEGER NOT NULL CHECK (event_rows >= 0),
    -- The position a high-water reset captured, which replay resumes above.
    high_water_cursor TEXT,
    recorded_at_unix_milliseconds INTEGER NOT NULL,
    -- One slot. Repeated conflicts about one subscription are one incident,
    -- because they are one disagreement being reported again.
    unresolved_incident TEXT,
    unresolved_incident_count INTEGER NOT NULL CHECK (unresolved_incident_count >= 0),
    -- A cursor exists exactly when something has been folded in, and what was
    -- at that position is recorded with it or the conflict case is undetectable.
    CHECK ((cursor IS NULL) = (canonical_digest IS NULL)),
    PRIMARY KEY (author_target_identity_digest, daemon_subscription_identifier)
) STRICT;

-- One position in one stream, with whatever it turned out to be about.
--
-- The association is nullable and is not a foreign key: an event may arrive
-- before the response that would have created the local row, and the position
-- is a fact whether or not the association ever happens.
CREATE TABLE subscription_event (
    agent_operation_identifier TEXT,
    author_target_identity_digest TEXT NOT NULL,
    canonical_digest TEXT NOT NULL,
    cursor TEXT NOT NULL,
    daemon_subscription_identifier TEXT NOT NULL,
    disposition TEXT NOT NULL CHECK (
        disposition IN ('advanced', 'exact_replay', 'stale_cursor_only', 'integrity_conflict')
    ),
    event_bytes INTEGER NOT NULL CHECK (event_bytes >= 0),
    job_sequence INTEGER CHECK (job_sequence IS NULL OR job_sequence >= 1),
    recorded_at_unix_milliseconds INTEGER NOT NULL,
    -- A sequence belongs to a job, so one without an association would be a
    -- number about nothing.
    CHECK ((agent_operation_identifier IS NULL) = (job_sequence IS NULL)),
    PRIMARY KEY (author_target_identity_digest, daemon_subscription_identifier, cursor),
    FOREIGN KEY (author_target_identity_digest, daemon_subscription_identifier)
        REFERENCES subscription_ledger (
            author_target_identity_digest, daemon_subscription_identifier
        )
        ON DELETE CASCADE
) STRICT;
