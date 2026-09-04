-- An event cursor is meaningful only inside one agent event-store generation.
CREATE TABLE subscription_event_new (
    agent_event_store_generation INTEGER NOT NULL CHECK (agent_event_store_generation >= 1),
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
    CHECK ((agent_operation_identifier IS NULL) = (job_sequence IS NULL)),
    PRIMARY KEY (
        author_target_identity_digest, daemon_subscription_identifier,
        agent_event_store_generation, cursor
    )
) STRICT;

INSERT INTO subscription_event_new
    (agent_event_store_generation, agent_operation_identifier,
     author_target_identity_digest, canonical_digest, cursor,
     daemon_subscription_identifier, disposition, event_bytes, job_sequence,
     recorded_at_unix_milliseconds)
SELECT ledger.agent_event_store_generation, event.agent_operation_identifier,
       event.author_target_identity_digest, event.canonical_digest, event.cursor,
       event.daemon_subscription_identifier, event.disposition, event.event_bytes,
       event.job_sequence, event.recorded_at_unix_milliseconds
FROM subscription_event AS event
JOIN subscription_ledger AS ledger
  ON ledger.author_target_identity_digest = event.author_target_identity_digest
 AND ledger.daemon_subscription_identifier = event.daemon_subscription_identifier;

DROP TABLE subscription_event;
ALTER TABLE subscription_event_new RENAME TO subscription_event;
