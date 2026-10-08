//! Every statement this crate may run, written down once.
//!
//! A closed inventory is worth the bother because the ways a database can
//! surprise you are mostly statements nobody reviewed: a dynamic string built
//! from a caller's value, an `ATTACH` that reaches another file, a `VACUUM` that
//! writes a whole second database beside this one, a sort large enough to spill.
//! A statement that is not here cannot ship.
//!
//! Each entry carries its own bounded shape, so reviewing the list is reviewing
//! the database's whole behaviour rather than reading the code that calls it.

mod definitions;

pub use definitions::{FORBIDDEN_CONSTRUCTS, InventoriedStatement, is_inventoried, statement_text};
use definitions::{LISTING_ROWS, PHYSICAL_JOB_ROWS, SINGLE_ROW, mutation};

/// Every statement, in the order a reader would want to read them.
pub const STATEMENTS: &[InventoriedStatement] = &[
    mutation(
        "install a reconciled subscription boundary without inventing an event digest",
        "UPDATE subscription_ledger SET agent_event_store_generation = ?, canonical_digest = NULL, cursor = NULL, high_water_cursor = ?, compacted_below_cursor = ?, unresolved_incident = NULL, unresolved_incident_count = 0, event_bytes = 0, event_rows = 0 WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? AND agent_event_store_generation = ? AND COALESCE(cursor, high_water_cursor) IS ? AND unresolved_incident IS ?",
        8,
    ),
    InventoriedStatement {
        purpose: "page unsettled subscription members across retained generations",
        text: "SELECT agent_operation_identifier FROM agent_operation AS a WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? AND (terminal_disposition IS NULL OR EXISTS(SELECT 1 FROM operation AS o WHERE o.author_target_identity_digest = a.author_target_identity_digest AND o.operation_identifier = a.operation_identifier AND o.lifecycle_state NOT IN ('succeeded', 'failed'))) AND NOT EXISTS(SELECT 1 FROM operation AS o WHERE o.author_target_identity_digest = a.author_target_identity_digest AND o.operation_identifier = a.operation_identifier AND o.lifecycle_state IN ('succeeded', 'failed')) AND (? IS NULL OR agent_operation_identifier > ?) ORDER BY agent_operation_identifier LIMIT 256",
        parameters: 4,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "detect unsettled members before a ledger-only reset",
        text: "SELECT EXISTS(SELECT 1 FROM agent_operation AS a WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? AND (terminal_disposition IS NULL OR EXISTS(SELECT 1 FROM operation AS o WHERE o.author_target_identity_digest = a.author_target_identity_digest AND o.operation_identifier = a.operation_identifier AND o.lifecycle_state NOT IN ('succeeded', 'failed'))) AND NOT EXISTS(SELECT 1 FROM operation AS o WHERE o.author_target_identity_digest = a.author_target_identity_digest AND o.operation_identifier = a.operation_identifier AND o.lifecycle_state IN ('succeeded', 'failed')))",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    mutation(
        "record the first artifact acquisition for one retained child",
        "UPDATE agent_operation SET acquisition_artifact_identifier = ?, acquisition_artifact_slot = ?, acquisition_content_digest = ?, acquisition_started_at_unix_milliseconds = ? WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? AND acquisition_started_at_unix_milliseconds IS NULL",
        6,
    ),
    InventoriedStatement {
        purpose: "read one retained artifact acquisition anchor",
        text: "SELECT acquisition_artifact_identifier, acquisition_artifact_slot, acquisition_content_digest, acquisition_started_at_unix_milliseconds FROM agent_operation WHERE author_target_identity_digest = ? AND agent_operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "read retained execution input inside its target partition",
        text: "SELECT canonical_command, daemon_runtime_contract_digest FROM operation \
               WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    mutation(
        "record this installation's identifier once",
        "INSERT INTO installation \
               (singleton, installation_identifier, recorded_at_unix_milliseconds) \
               VALUES (0, ?, ?)",
        2,
    ),
    InventoriedStatement {
        purpose: "read this installation's identifier",
        text: "SELECT installation_identifier FROM installation WHERE singleton = 0",
        parameters: 0,
        maximum_rows: SINGLE_ROW,
    },
    mutation(
        "admit one operation",
        "INSERT INTO operation \
               (author_target_identity, author_target_identity_digest, caller_identity, \
                canonical_command, command_fingerprint, command_wire_name, \
                daemon_runtime_contract_digest, enqueue_sequence, installation_identifier, \
                lifecycle_state, operation_identifier, operation_revision, \
                recorded_at_unix_milliseconds, selected_environment_revision, \
                workflow_correlation_identifier) \
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        15,
    ),
    InventoriedStatement {
        purpose: "read one operation inside its target partition",
        // A summary is not a payload: the canonical command, the opaque author
        // identity, and the contract digest stay in the row rather than
        // travelling with every lookup that only wants to know where the work
        // has got to.
        text: "SELECT caller_identity, command_fingerprint, command_wire_name, \
                      enqueue_sequence, installation_identifier, latest_progress, \
                      lifecycle_state, operation_revision, recorded_at_unix_milliseconds, \
                      result_disposition, result_inline_bytes, selected_environment_revision, \
                      settled_at_unix_milliseconds, terminal_failure_disposition, \
                      terminal_failure_kind, terminal_failure_metadata, \
                      workflow_correlation_identifier \
               FROM operation \
               WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "reserve the next enqueue sequence inside one target partition",
        text: "SELECT COALESCE(MAX(enqueue_sequence), 0) + 1 FROM operation \
               WHERE author_target_identity_digest = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "list every partition holding work that has not ended",
        // Startup audits these before it binds anything. Terminal rows are
        // deliberately excluded: history from a target this daemon no longer
        // serves is something to keep and answer questions about, while
        // unfinished work under another identity is something no daemon may
        // quietly adopt.
        text: "SELECT DISTINCT author_target_identity_digest, selected_environment_revision, daemon_runtime_contract_digest \
               FROM operation \
               WHERE lifecycle_state NOT IN ('succeeded', 'failed') \
               ORDER BY author_target_identity_digest, selected_environment_revision, daemon_runtime_contract_digest",
        parameters: 0,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "count waiting operations during admission",
        // Stream identifiers, not command/result bodies. The namespace owner
        // excludes its live execution slots while this write transaction holds
        // the admission decision and insert together.
        text: "SELECT operation_identifier, caller_identity FROM operation \
               WHERE author_target_identity_digest = ? \
                 AND lifecycle_state NOT IN ('succeeded', 'failed')",
        parameters: 1,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "reconstruct one target's operations in enqueue order",
        text: "SELECT operation_identifier FROM operation \
               WHERE author_target_identity_digest = ? \
               ORDER BY enqueue_sequence, operation_identifier",
        parameters: 1,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "list one target's operations, newest first",
        // Ordered by the stable arrival sequence rather than by a timestamp, so
        // a cursor names a position that cannot move. No command payload is
        // read: a listing says what happened to work, not what the work was.
        text: "SELECT enqueue_sequence, lifecycle_state, operation_identifier, \
                      operation_revision, caller_identity, workflow_correlation_identifier, \
                      terminal_failure_kind, settled_at_unix_milliseconds \
               FROM operation \
               WHERE author_target_identity_digest = ? AND enqueue_sequence < ? \
               ORDER BY enqueue_sequence DESC, operation_identifier \
               LIMIT ?",
        parameters: 3,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "list one target's operations with lifecycle filters",
        text: "SELECT enqueue_sequence, lifecycle_state, operation_identifier, \
                      operation_revision, caller_identity, workflow_correlation_identifier, \
                      terminal_failure_kind, settled_at_unix_milliseconds \
               FROM operation WHERE author_target_identity_digest = ? \
                 AND (enqueue_sequence < ? OR (enqueue_sequence = ? AND operation_identifier > ?)) \
                 AND ((lifecycle_state = 'queued' AND ?) OR (lifecycle_state = 'submitting' AND ?) \
                   OR (lifecycle_state = 'accepted' AND ?) OR (lifecycle_state = 'running' AND ?) \
                   OR (lifecycle_state = 'succeeded' AND ?) OR (lifecycle_state = 'failed' AND ?)) \
                 AND (? IS NULL OR caller_identity = ?) \
                 AND (? IS NULL OR (lifecycle_state IN ('succeeded', 'failed')) = ?) \
                 AND (? IS NULL OR workflow_correlation_identifier = ?) \
               ORDER BY enqueue_sequence DESC, operation_identifier LIMIT ?",
        parameters: 17,
        maximum_rows: LISTING_ROWS,
    },
    mutation(
        "record one folded operation under compare-and-set",
        "UPDATE operation \
               SET latest_progress = ?, lifecycle_state = ?, operation_revision = ?, \
                   result_disposition = ?, result_inline_bytes = ?, \
                   settled_at_unix_milliseconds = ?, \
                   terminal_failure_disposition = ?, terminal_failure_kind = ?, \
                   terminal_failure_metadata = ? \
               WHERE author_target_identity_digest = ? AND operation_identifier = ? \
                 AND operation_revision = ?",
        12,
    ),
    mutation(
        "record one folded operation under compare-and-set, releasing its scheduler claim",
        "UPDATE operation \
               SET latest_progress = ?, lifecycle_state = ?, operation_revision = ?, \
                   result_disposition = ?, result_inline_bytes = ?, \
                   settled_at_unix_milliseconds = ?, \
                   terminal_failure_disposition = ?, terminal_failure_kind = ?, \
                   terminal_failure_metadata = ?, \
                   scheduler_checkpoint = NULL, scheduler_fence = NULL, \
                   scheduler_lease_expires_at_unix_milliseconds = NULL \
               WHERE author_target_identity_digest = ? AND operation_identifier = ? \
                 AND operation_revision = ?",
        12,
    ),
    mutation(
        "release a scheduler claim this attempt proved but left the operation unchanged",
        "UPDATE operation \
               SET scheduler_checkpoint = NULL, scheduler_fence = NULL, \
                   scheduler_lease_expires_at_unix_milliseconds = NULL \
               WHERE author_target_identity_digest = ? AND operation_identifier = ? \
                 AND scheduler_fence = ?",
        3,
    ),
    mutation(
        "record the one recovery fact an operation is waiting on",
        "INSERT OR REPLACE INTO recovery_fact \
               (attempt_count, author_target_identity_digest, category, detail, \
                evidence_certainty, evidence_kind, manual_resume_eligible, \
                operation_identifier, retry_delay_milliseconds, \
                retry_observed_at_unix_milliseconds) \
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        10,
    ),
    InventoriedStatement {
        purpose: "read the one recovery fact an operation is waiting on",
        text: "SELECT attempt_count, category, detail, evidence_certainty, evidence_kind, \
                      manual_resume_eligible, retry_delay_milliseconds, \
                      retry_observed_at_unix_milliseconds \
               FROM recovery_fact \
               WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    mutation(
        "clear the recovery fact an operation is no longer waiting on",
        "DELETE FROM recovery_fact \
               WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        2,
    ),
    mutation(
        "record one recovery-resume receipt",
        "INSERT INTO recovery_resume_receipt \
               (applied_operation_revision, author_target_identity_digest, \
                operation_identifier, recorded_at_unix_milliseconds, \
                selected_environment_revision, source_fingerprint) \
               VALUES (?, ?, ?, ?, ?, ?)",
        6,
    ),
    InventoriedStatement {
        purpose: "count one operation's recovery-resume receipts",
        text: "SELECT COUNT(*) FROM recovery_resume_receipt \
               WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "count this namespace's retained operation rows",
        text: "SELECT COUNT(*) FROM operation",
        parameters: 0,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "count one target's maintenance-application receipts",
        text: "SELECT COUNT(*) FROM maintenance_application_receipt \
               WHERE author_target_identity_digest = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "count one target's maintenance-result associations",
        text: "SELECT COUNT(*) FROM maintenance_result_association \
               WHERE author_target_identity_digest = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "measure the bytes this namespace's committed content occupies",
        text: "SELECT COALESCE(SUM(byte_length), 0) FROM artifact_blob",
        parameters: 0,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "measure this namespace's durable artifact reservations",
        text: "SELECT COALESCE(SUM(byte_length), 0) FROM artifact_reservation",
        parameters: 0,
        maximum_rows: SINGLE_ROW,
    },
    mutation(
        "hold one artifact reservation durably",
        "INSERT INTO artifact_reservation (byte_length) VALUES (?)",
        1,
    ),
    mutation(
        "release one durable artifact reservation",
        "DELETE FROM artifact_reservation WHERE ticket = ?",
        1,
    ),
    mutation(
        "reconcile abandoned artifact reservations at startup",
        "DELETE FROM artifact_reservation",
        0,
    ),
    InventoriedStatement {
        purpose: "read one artifact blob's recorded length",
        text: "SELECT byte_length FROM artifact_blob WHERE content_digest = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    mutation(
        "record one artifact's content, once per digest",
        "INSERT OR IGNORE INTO artifact_blob \
               (byte_length, content_digest, recorded_at_unix_milliseconds) \
               VALUES (?, ?, ?)",
        3,
    ),
    InventoriedStatement {
        purpose: "read one durable artifact reservation",
        text: "SELECT byte_length FROM artifact_reservation WHERE ticket = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "reconstruct bounded pending artifact publications",
        text: "SELECT p.publication_identifier, p.artifact_identifier, p.content_digest, \
                      b.byte_length, p.recorded_at_unix_milliseconds \
               FROM artifact_publication p LEFT JOIN artifact_blob b ON b.content_digest = p.content_digest \
               WHERE (? IS NULL OR p.publication_identifier > ?) \
               ORDER BY p.publication_identifier LIMIT 256",
        parameters: 2,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "count this namespace's pending artifact publications",
        text: "SELECT COUNT(*) FROM artifact_publication",
        parameters: 0,
        maximum_rows: SINGLE_ROW,
    },
    mutation(
        "retain one artifact publication across restart",
        "INSERT INTO artifact_publication (publication_identifier, artifact_identifier, content_digest, recorded_at_unix_milliseconds) VALUES (?, ?, ?, ?)",
        4,
    ),
    mutation(
        "bind a maintenance publication to its operation-free owner",
        "INSERT INTO maintenance_publication (publication_identifier, author_target_identity_digest, kind, reviewed_source_digest) VALUES (?, ?, ?, ?)",
        4,
    ),
    InventoriedStatement {
        purpose: "read one target's pending maintenance publication",
        text: "SELECT p.publication_identifier, p.artifact_identifier, p.content_digest, b.byte_length, p.recorded_at_unix_milliseconds, m.kind, m.reviewed_source_digest FROM maintenance_publication m JOIN artifact_publication p ON p.publication_identifier = m.publication_identifier LEFT JOIN artifact_blob b ON b.content_digest = p.content_digest WHERE m.author_target_identity_digest = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    mutation(
        "consume one completed artifact publication",
        "DELETE FROM artifact_publication WHERE publication_identifier = ? AND artifact_identifier = ? AND content_digest = ?",
        3,
    ),
    InventoriedStatement {
        purpose: "find an artifact's pending publication without hiding ambiguity",
        text: "SELECT p.publication_identifier, p.content_digest, b.byte_length FROM artifact_publication p JOIN artifact_blob b ON b.content_digest = p.content_digest WHERE p.artifact_identifier = ? ORDER BY p.publication_identifier LIMIT 2",
        parameters: 1,
        maximum_rows: 2,
    },
    mutation(
        "associate one artifact with the operation slot it fills",
        "INSERT INTO artifact_association \
               (artifact_identifier, artifact_slot, author_target_identity_digest, \
                byte_length, content_digest, media_type, operation_identifier) \
               VALUES (?, ?, ?, ?, ?, ?, ?)",
        7,
    ),
    InventoriedStatement {
        purpose: "read the artifact one operation slot holds",
        text: "SELECT artifact_identifier, byte_length, content_digest, media_type \
               FROM artifact_association \
               WHERE author_target_identity_digest = ? AND operation_identifier = ? \
                 AND artifact_slot = ?",
        parameters: 3,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "resolve an artifact identifier inside one operation",
        text: "SELECT artifact_slot FROM artifact_association WHERE author_target_identity_digest = ? AND operation_identifier = ? AND artifact_identifier = ? LIMIT 2",
        parameters: 3,
        maximum_rows: 2,
    },
    mutation(
        "record one maintenance-application receipt",
        "INSERT INTO maintenance_application_receipt \
               (application_receipt_identifier, author_target_identity_digest, \
                recorded_at_unix_milliseconds, released_operation_rows, reviewed_manifest_digest) \
               VALUES (?, ?, ?, ?, ?)",
        5,
    ),
    InventoriedStatement {
        purpose: "select one target's operations that ended before a cutoff",
        // Terminal only, and never a nonterminal row under any criteria. Work
        // that has not ended is work somebody may still be waiting on, and no
        // amount of age makes it safe to remove.
        text: "SELECT operation_identifier, operation_revision, settled_at_unix_milliseconds \
               FROM operation \
               WHERE author_target_identity_digest = ? \
                 AND lifecycle_state IN ('succeeded', 'failed') \
                 AND settled_at_unix_milliseconds IS NOT NULL \
                 AND settled_at_unix_milliseconds < ? \
               ORDER BY settled_at_unix_milliseconds, operation_identifier \
               LIMIT ?",
        parameters: 3,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "remove one terminal operation and everything hanging off it",
        // The children go with it through the schema's own cascades rather than
        // through a list this statement has to keep in step with the schema.
        text: "DELETE FROM operation \
               WHERE author_target_identity_digest = ? AND operation_identifier = ? \
                 AND operation_revision = ? AND settled_at_unix_milliseconds = ? \
                 AND lifecycle_state IN ('succeeded', 'failed')",
        parameters: 4,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "list one target's unfinished maintenance receipts",
        text: "SELECT application_receipt_identifier FROM maintenance_application_receipt \
               WHERE author_target_identity_digest = ? AND stage = 'database_applied' \
               ORDER BY application_receipt_identifier LIMIT ?",
        parameters: 2,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "list one receipt's pending artifact cleanup work",
        text: "SELECT content_digest FROM maintenance_artifact_cleanup_work \
               WHERE author_target_identity_digest = ? AND application_receipt_identifier = ? \
               ORDER BY content_digest",
        parameters: 2,
        maximum_rows: LISTING_ROWS,
    },
    mutation(
        "remove one completed maintenance artifact cleanup item",
        "DELETE FROM maintenance_artifact_cleanup_work \
               WHERE author_target_identity_digest = ? AND application_receipt_identifier = ? \
                 AND content_digest = ?",
        3,
    ),
    InventoriedStatement {
        purpose: "count one receipt's pending artifact cleanup work",
        text: "SELECT COUNT(*) FROM maintenance_artifact_cleanup_work \
               WHERE author_target_identity_digest = ? AND application_receipt_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    mutation(
        "mark one maintenance receipt completed",
        "UPDATE maintenance_application_receipt SET stage = 'completed' \
               WHERE author_target_identity_digest = ? AND application_receipt_identifier = ? \
                 AND stage = 'database_applied'",
        2,
    ),
    InventoriedStatement {
        purpose: "list an operation's artifact cleanup candidates",
        text: "SELECT content_digest FROM artifact_association \
               WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: LISTING_ROWS,
    },
    mutation(
        "record one maintenance artifact cleanup item",
        "INSERT OR IGNORE INTO maintenance_artifact_cleanup_work \
               (application_receipt_identifier, author_target_identity_digest, content_digest) \
               VALUES (?, ?, ?)",
        3,
    ),
    InventoriedStatement {
        purpose: "count what still references one artifact's content",
        text: "SELECT (SELECT COUNT(*) FROM artifact_association WHERE content_digest = ?) \
                      + (SELECT COUNT(*) FROM maintenance_result_association \
                         WHERE content_digest = ?) \
                      + (SELECT COUNT(*) FROM artifact_publication WHERE content_digest = ?)",
        parameters: 3,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "count durable associations retaining shared artifact content",
        text: "SELECT (SELECT COUNT(*) FROM artifact_association WHERE content_digest = ?) + (SELECT COUNT(*) FROM maintenance_result_association WHERE content_digest = ?)",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    mutation(
        "remove one artifact's content, once nothing references it",
        "DELETE FROM artifact_blob WHERE content_digest = ?",
        1,
    ),
    InventoriedStatement {
        purpose: "read one target's maintenance-application receipt",
        text: "SELECT recorded_at_unix_milliseconds, released_operation_rows, stage, \
                      reviewed_manifest_digest \
               FROM maintenance_application_receipt \
               WHERE author_target_identity_digest = ? AND application_receipt_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "read one maintenance result by target and identifier alone",
        text: "SELECT a.association_revision, a.byte_length, a.content_digest, a.kind, a.media_type, \
                      a.owning_application_receipt_identifier, a.reviewed_source_digest, \
                      a.is_current_preview, b.byte_length, r.application_receipt_identifier \
               FROM maintenance_result_association a \
               LEFT JOIN artifact_blob b ON b.content_digest = a.content_digest \
               LEFT JOIN maintenance_application_receipt r \
                 ON r.author_target_identity_digest = a.author_target_identity_digest \
                AND r.application_receipt_identifier = a.owning_application_receipt_identifier \
               WHERE a.author_target_identity_digest = ? \
                 AND a.maintenance_result_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "read the current maintenance preview identifier",
        text: "SELECT maintenance_result_identifier FROM maintenance_result_association WHERE author_target_identity_digest = ? AND is_current_preview = 1",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "read pending superseded preview cleanup",
        text: "SELECT application_receipt_identifier, content_digest FROM maintenance_artifact_cleanup_work WHERE author_target_identity_digest = ? AND substr(application_receipt_identifier, 1, 8) = 'preview:' ORDER BY application_receipt_identifier LIMIT 2",
        parameters: 1,
        maximum_rows: 2,
    },
    mutation(
        "retain an applied preview under its application receipt",
        "UPDATE maintenance_result_association SET is_current_preview = 0, owning_application_receipt_identifier = ?, association_revision = association_revision + 1 WHERE author_target_identity_digest = ? AND maintenance_result_identifier = ? AND reviewed_source_digest = ? AND association_revision = ? AND is_current_preview = 1 AND owning_application_receipt_identifier IS NULL",
        5,
    ),
    mutation(
        "retire the current maintenance preview association",
        "DELETE FROM maintenance_result_association WHERE author_target_identity_digest = ? AND is_current_preview = 1 AND owning_application_receipt_identifier IS NULL",
        1,
    ),
    mutation(
        "record the current maintenance preview association",
        "INSERT INTO maintenance_result_association (association_revision, author_target_identity_digest, byte_length, content_digest, is_current_preview, kind, maintenance_result_identifier, media_type, owning_application_receipt_identifier, reviewed_source_digest) VALUES (1, ?, ?, ?, 1, 'preview', ?, 'application/json', NULL, ?)",
        5,
    ),
    InventoriedStatement {
        purpose: "read application result identifiers owned by one receipt",
        text: "SELECT maintenance_result_identifier FROM maintenance_result_association WHERE author_target_identity_digest = ? AND owning_application_receipt_identifier = ? AND kind = 'application' LIMIT 2",
        parameters: 2,
        maximum_rows: 2,
    },
    InventoriedStatement {
        purpose: "read maintenance result identifiers retained by one receipt",
        text: "SELECT maintenance_result_identifier FROM maintenance_result_association WHERE author_target_identity_digest = ? AND owning_application_receipt_identifier = ? ORDER BY maintenance_result_identifier LIMIT 2",
        parameters: 2,
        maximum_rows: 2,
    },
    mutation(
        "record a receipt-owned maintenance application result",
        "INSERT INTO maintenance_result_association (association_revision, author_target_identity_digest, byte_length, content_digest, is_current_preview, kind, maintenance_result_identifier, media_type, owning_application_receipt_identifier, reviewed_source_digest) VALUES (1, ?, ?, ?, 0, 'application', ?, 'application/json', ?, ?)",
        6,
    ),
    InventoriedStatement {
        purpose: "read one recovery-resume receipt by operation and source fingerprint",
        text: "SELECT applied_operation_revision, operation_identifier, \
                      recorded_at_unix_milliseconds, selected_environment_revision \
               FROM recovery_resume_receipt \
               WHERE author_target_identity_digest = ? AND operation_identifier = ? \
                 AND source_fingerprint = ?",
        parameters: 3,
        maximum_rows: SINGLE_ROW,
    },
    mutation(
        "admit one agent submission",
        "INSERT INTO agent_operation \
               (agent_event_store_generation, agent_operation_identifier, applied_sequence, \
                argument_schema_digest, attempt, author_agent_transport_contract_digest, \
                author_target_identity_digest, canonical_submission, \
                command_canonical_json_contract_digest, command_contract_limits_digest, \
                command_semantic_contract_version, command_wire_name, \
                daemon_subscription_identifier, job_state, operation_identifier, progress, \
                recorded_at_unix_milliseconds, remaining_retention_milliseconds, \
                request_start_unix_milliseconds, result_schema_digest, \
                selected_environment_revision, snapshot_watermark, submitted_command_digest) \
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        23,
    ),
    InventoriedStatement {
        purpose: "count unfinished author submissions without the selected local owner",
        text: "SELECT COUNT(*) FROM agent_operation a LEFT JOIN operation o \
               ON o.author_target_identity_digest = a.author_target_identity_digest \
               AND o.operation_identifier = a.operation_identifier \
               WHERE (o.operation_identifier IS NULL AND a.job_state IN ('queued', 'running')) \
               OR (o.lifecycle_state NOT IN ('succeeded', 'failed') \
                   AND (a.author_target_identity_digest != ? OR a.selected_environment_revision != ?))",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "count unfinished operations from another installation",
        text: "SELECT COUNT(*) FROM operation WHERE lifecycle_state NOT IN ('succeeded', 'failed') \
               AND installation_identifier != ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "find one local operation's retained author submission",
        text: "SELECT agent_operation_identifier FROM agent_operation \
               WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "read one agent submission inside its target partition",
        // Every derivation input comes back, because a restart re-derives the
        // identity from what this build has and compares it with what is here.
        // A summary that dropped the contract columns would make that
        // comparison impossible and the resumption dishonest.
        text: "SELECT agent_event_store_generation, applied_sequence, argument_schema_digest, \
                      attempt, author_agent_transport_contract_digest, canonical_submission, \
                      command_canonical_json_contract_digest, command_contract_limits_digest, \
                      command_semantic_contract_version, command_wire_name, \
                      daemon_subscription_identifier, job_state, operation_identifier, progress, \
                      recorded_at_unix_milliseconds, remaining_retention_milliseconds, \
                      request_start_unix_milliseconds, result_schema_digest, \
                      selected_environment_revision, snapshot_watermark, \
                      submitted_command_digest, terminal_disposition \
               FROM agent_operation \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "count the agent submissions one target holds",
        text: "SELECT COUNT(*) FROM agent_operation WHERE author_target_identity_digest = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "record one physical Sling job for one agent submission",
        // Several physical records for one logical submission is ordinary, so a
        // repeat is not an error. It records the same name again and changes
        // nothing, which is what at-least-once delivery looks like when it is
        // handled rather than merely survived.
        text: "INSERT OR IGNORE INTO agent_physical_job \
               (agent_operation_identifier, author_target_identity_digest, \
                recorded_at_unix_milliseconds, sling_job_identifier) \
               VALUES (?, ?, ?, ?)",
        parameters: 4,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "read one agent submission's physical Sling jobs in order",
        text: "SELECT sling_job_identifier FROM agent_physical_job \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? \
               ORDER BY sling_job_identifier",
        parameters: 2,
        maximum_rows: PHYSICAL_JOB_ROWS,
    },
    mutation(
        "retain one acknowledged submission lifetime",
        "UPDATE agent_operation SET remaining_retention_milliseconds = ? \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? \
                 AND submitted_command_digest = ? AND terminal_disposition IS NULL",
        4,
    ),
    InventoriedStatement {
        purpose: "fold one believed event into one agent submission",
        // The applied sequence is in the predicate as well as the assignment,
        // so two folds racing on one row cannot both succeed and neither can
        // apply an event to a row that has already moved past it.
        //
        // The last three clauses are the domain's transition table, restated
        // where the write happens. A row may be set to queued only while it is
        // queued, because Sling delivers at least once and a physical requeue
        // is the same work running rather than work that stopped; and attempts
        // and progress only ever increase. Restated rather than trusted, so a
        // caller that skipped the reducer cannot write a fact the domain would
        // have refused.
        text: "UPDATE agent_operation \
               SET applied_sequence = ?, attempt = ?, job_state = ?, progress = ? \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? \
                 AND applied_sequence = ? AND terminal_disposition IS NULL \
                 AND (job_state = 'queued' OR ? <> 'queued') \
                 AND attempt <= ? AND progress <= ?",
        parameters: 10,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "record one snapshot watermark on one agent submission",
        // Never backwards. A snapshot that covered less than one already
        // applied would make settled events look unsettled again.
        text: "UPDATE agent_operation SET snapshot_watermark = ? \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? \
                 AND snapshot_watermark <= ?",
        parameters: 4,
        maximum_rows: 0,
    },
    mutation(
        "settle one agent submission",
        "UPDATE agent_operation \
               SET applied_sequence = ?, attempt = ?, job_state = ?, progress = ?, \
                   remaining_retention_milliseconds = ?, terminal_disposition = ? \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? \
                 AND terminal_disposition IS NULL",
        8,
    ),
    mutation(
        "open one subscription ledger",
        "INSERT INTO subscription_ledger \
               (agent_event_store_generation, author_target_identity_digest, \
                daemon_subscription_identifier, event_bytes, event_rows, \
                recorded_at_unix_milliseconds, unresolved_incident_count) \
               VALUES (?, ?, ?, 0, 0, ?, 0)",
        4,
    ),
    InventoriedStatement {
        purpose: "read one subscription ledger",
        text: "SELECT agent_event_store_generation, canonical_digest, compacted_below_cursor, \
                      COALESCE(cursor, high_water_cursor) AS cursor, event_bytes, event_rows, high_water_cursor, \
                      recorded_at_unix_milliseconds, unresolved_incident, \
                      unresolved_incident_count \
               FROM subscription_ledger \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "count the subscription ledgers one target holds",
        text: "SELECT COUNT(*) FROM subscription_ledger WHERE author_target_identity_digest = ?",
        parameters: 1,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "advance one subscription ledger to a later position",
        // The predicate is the whole idempotency story. A position that is not
        // strictly later changes nothing, so a replayed event and a stale one
        // both leave the ledger where it was without the caller having to
        // decide which it was looking at.
        text: "UPDATE subscription_ledger \
               SET canonical_digest = ?, cursor = ?, event_bytes = event_bytes + ?, \
                   event_rows = event_rows + 1 \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? \
                 AND agent_event_store_generation = ? \
                 AND (COALESCE(cursor, high_water_cursor) IS NULL OR COALESCE(cursor, high_water_cursor) COLLATE slingshot_cursor < ?)",
        parameters: 7,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "record one unresolved integrity incident on a subscription",
        // One slot, and the counter moves only when the slot was empty.
        // Repeated conflicts about one subscription are one disagreement being
        // reported again, and charging capacity for each report would let a
        // misbehaving agent exhaust it by repeating itself.
        text: "UPDATE subscription_ledger \
               SET unresolved_incident = COALESCE(unresolved_incident, ?), \
                   unresolved_incident_count = \
                       unresolved_incident_count + (unresolved_incident IS NULL) \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ?",
        parameters: 3,
        maximum_rows: 0,
    },
    mutation(
        "install a captured high-water position on a subscription",
        "UPDATE subscription_ledger \
               SET agent_event_store_generation = ?, canonical_digest = ?, cursor = ?, \
                   high_water_cursor = ?, unresolved_incident = NULL, \
                   unresolved_incident_count = 0, event_bytes = 0, event_rows = 0, \
                   compacted_below_cursor = NULL \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? \
                 AND agent_event_store_generation = ? AND COALESCE(cursor, high_water_cursor) IS ? \
                 AND unresolved_incident IS ? AND ? > agent_event_store_generation",
        10,
    ),
    mutation(
        "remove a subscription generation's retained events",
        "DELETE FROM subscription_event \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? \
                 AND agent_event_store_generation = ?",
        3,
    ),
    mutation(
        "record one subscription event",
        "INSERT INTO subscription_event \
               (agent_event_store_generation, agent_operation_identifier, \
                author_target_identity_digest, canonical_digest, \
                cursor, daemon_subscription_identifier, disposition, event_bytes, job_sequence, \
                recorded_at_unix_milliseconds) \
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        10,
    ),
    InventoriedStatement {
        purpose: "read one subscription event by its position",
        text: "SELECT agent_operation_identifier, canonical_digest, disposition, event_bytes, \
                      job_sequence, recorded_at_unix_milliseconds \
               FROM subscription_event \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? \
                 AND cursor = ?",
        parameters: 3,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "measure one subscription's retained events",
        text: "SELECT COUNT(*), COALESCE(SUM(event_bytes), 0) FROM subscription_event \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? \
                 AND agent_event_store_generation = ?",
        parameters: 3,
        maximum_rows: SINGLE_ROW,
    },
    mutation(
        "compact one subscription's events below a position",
        "DELETE FROM subscription_event \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? \
                 AND agent_event_store_generation = ? AND cursor COLLATE slingshot_cursor < ?",
        4,
    ),
    mutation(
        "record one subscription's compaction floor",
        "UPDATE subscription_ledger \
               SET compacted_below_cursor = ?, event_bytes = ?, event_rows = ? \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? \
                 AND agent_event_store_generation = ?",
        6,
    ),
    InventoriedStatement {
        purpose: "select the agent submissions one maintenance run would remove",
        // Ended work only, and named in one fixed order so two previews of the
        // same target under the same window digest alike.
        text: "SELECT agent_operation_identifier, submitted_command_digest, \
                      COALESCE(terminal_disposition, (SELECT operation.terminal_failure_kind FROM operation \
                        WHERE operation.author_target_identity_digest = agent_operation.author_target_identity_digest \
                          AND operation.operation_identifier = agent_operation.operation_identifier)) AS terminal_disposition \
               FROM agent_operation \
               WHERE author_target_identity_digest = ? AND (terminal_disposition IS NOT NULL \
                 OR EXISTS (SELECT 1 FROM operation WHERE operation.author_target_identity_digest = agent_operation.author_target_identity_digest \
                   AND operation.operation_identifier = agent_operation.operation_identifier \
                   AND operation.selected_environment_revision = agent_operation.selected_environment_revision \
                   AND operation.settled_at_unix_milliseconds < ? \
                   AND operation.terminal_failure_kind IN ('recovery_window_expired', 'result_unavailable', 'remote_state_lost'))) \
                 AND recorded_at_unix_milliseconds < ? \
               ORDER BY agent_operation_identifier LIMIT ?",
        parameters: 4,
        maximum_rows: LISTING_ROWS,
    },
    mutation(
        "remove one ended agent submission",
        "DELETE FROM agent_operation \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? \
                 AND submitted_command_digest = ? AND (terminal_disposition = ? \
                   OR (terminal_disposition IS NULL AND EXISTS (SELECT 1 FROM operation \
                     WHERE operation.author_target_identity_digest = agent_operation.author_target_identity_digest \
                       AND operation.operation_identifier = agent_operation.operation_identifier \
                       AND operation.selected_environment_revision = agent_operation.selected_environment_revision \
                       AND operation.settled_at_unix_milliseconds IS NOT NULL \
                       AND operation.terminal_failure_kind IN ('recovery_window_expired', 'result_unavailable', 'remote_state_lost'))))",
        4,
    ),
    InventoriedStatement {
        purpose: "select the subscriptions no retained agent submission needs",
        text: "SELECT daemon_subscription_identifier FROM subscription_ledger \
               WHERE author_target_identity_digest = ? \
                 AND daemon_subscription_identifier NOT IN ( \
                     SELECT daemon_subscription_identifier FROM agent_operation \
                     WHERE author_target_identity_digest = ?) \
               ORDER BY daemon_subscription_identifier LIMIT ?",
        parameters: 3,
        maximum_rows: LISTING_ROWS,
    },
    mutation(
        "retire one subscription no retained agent submission needs",
        "DELETE FROM subscription_ledger \
               WHERE author_target_identity_digest = ? AND daemon_subscription_identifier = ? \
                 AND daemon_subscription_identifier NOT IN ( \
                     SELECT daemon_subscription_identifier FROM agent_operation \
                     WHERE author_target_identity_digest = ?)",
        3,
    ),
    InventoriedStatement {
        purpose: "claim the right to start one agent submission",
        // A higher fence takes the claim from a lower one, and nothing takes it
        // after the checkpoint. That second clause is the whole no-return rule:
        // a lease that expired after the work started does not become a licence
        // for somebody else to start it again.
        text: "UPDATE agent_operation SET worker_fence = ? \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? \
                 AND execution_checkpoint IS NULL \
                 AND (worker_fence IS NULL OR worker_fence < ?)",
        parameters: 4,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "record the no-return checkpoint on one agent submission",
        // Only the holder of the current fence, and only once. The attempt
        // count moves with it, so an outbox attempt is counted exactly when an
        // effect may have happened rather than when one was contemplated.
        text: "UPDATE agent_operation \
               SET execution_checkpoint = ?, outbox_attempts = outbox_attempts + 1 \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ? \
                 AND worker_fence = ? AND execution_checkpoint IS NULL",
        parameters: 4,
        maximum_rows: 0,
    },
    InventoriedStatement {
        purpose: "read one agent submission's execution fence",
        text: "SELECT execution_checkpoint, outbox_attempts, worker_fence \
               FROM agent_operation \
               WHERE author_target_identity_digest = ? AND agent_operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "read one scheduler claim candidate",
        text: "SELECT lifecycle_state, operation_revision, scheduler_checkpoint FROM operation WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    mutation(
        "clear one resumed operation's stale scheduler claim",
        "UPDATE operation SET scheduler_fence = NULL, scheduler_lease_expires_at_unix_milliseconds = NULL, scheduler_checkpoint = NULL WHERE author_target_identity_digest = ? AND operation_identifier = ? AND lifecycle_state = ? AND operation_revision = ?",
        4,
    ),
    mutation(
        "claim one retained operation for execution",
        "UPDATE operation SET scheduler_fence = ?, scheduler_lease_expires_at_unix_milliseconds = ? WHERE author_target_identity_digest = ? AND operation_identifier = ? AND lifecycle_state = ? AND operation_revision = ? AND scheduler_checkpoint IS NULL AND (scheduler_lease_expires_at_unix_milliseconds IS NULL OR scheduler_lease_expires_at_unix_milliseconds <= ?) AND (scheduler_fence IS NULL OR scheduler_fence < ?)",
        8,
    ),
    mutation(
        "checkpoint one retained operation execution",
        "UPDATE operation SET scheduler_checkpoint = ? WHERE author_target_identity_digest = ? AND operation_identifier = ? AND scheduler_fence = ? AND scheduler_checkpoint IS NULL",
        4,
    ),
    mutation(
        "renew one retained operation execution lease",
        "UPDATE operation SET scheduler_lease_expires_at_unix_milliseconds = ? WHERE author_target_identity_digest = ? AND operation_identifier = ? AND scheduler_fence = ? AND scheduler_checkpoint IS NULL AND (scheduler_lease_expires_at_unix_milliseconds IS NULL OR scheduler_lease_expires_at_unix_milliseconds >= ?)",
        5,
    ),
    InventoriedStatement {
        purpose: "read one retained operation scheduler claim",
        text: "SELECT scheduler_fence, scheduler_lease_expires_at_unix_milliseconds, scheduler_checkpoint FROM operation WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "read one retained operation execution fence",
        text: "SELECT scheduler_fence, scheduler_checkpoint FROM operation WHERE author_target_identity_digest = ? AND operation_identifier = ?",
        parameters: 2,
        maximum_rows: SINGLE_ROW,
    },
    InventoriedStatement {
        purpose: "select queued candidates for scheduler claim",
        text: "SELECT operation.operation_identifier, operation.lifecycle_state, operation.operation_revision, operation.caller_identity, COALESCE(recovery_fact.retry_observed_at_unix_milliseconds, 0) AS retry_observed_at_unix_milliseconds, COALESCE(recovery_fact.retry_delay_milliseconds, 0) AS retry_delay_milliseconds, COALESCE(recovery_fact.attempt_count, 0) AS attempt_count FROM operation LEFT JOIN recovery_fact ON recovery_fact.author_target_identity_digest = operation.author_target_identity_digest AND recovery_fact.operation_identifier = operation.operation_identifier WHERE operation.author_target_identity_digest = ? AND operation.lifecycle_state = 'queued' AND operation.scheduler_checkpoint IS NULL AND (operation.scheduler_lease_expires_at_unix_milliseconds IS NULL OR operation.scheduler_lease_expires_at_unix_milliseconds <= ?) AND COALESCE(recovery_fact.manual_resume_eligible, 0) = 0 ORDER BY operation.enqueue_sequence ASC, operation.operation_identifier ASC",
        parameters: 2,
        maximum_rows: LISTING_ROWS,
    },
    InventoriedStatement {
        purpose: "select queued operations paused for manual recovery",
        text: "SELECT operation.operation_identifier, operation.operation_revision, operation.scheduler_fence, recovery_fact.evidence_kind FROM operation INNER JOIN recovery_fact ON recovery_fact.author_target_identity_digest = operation.author_target_identity_digest AND recovery_fact.operation_identifier = operation.operation_identifier WHERE operation.author_target_identity_digest = ? AND operation.lifecycle_state = 'queued' AND recovery_fact.manual_resume_eligible = 1",
        parameters: 1,
        maximum_rows: LISTING_ROWS,
    },
    mutation(
        "clear every scheduler claim a dead instance left behind",
        "UPDATE operation SET scheduler_checkpoint = NULL, scheduler_fence = NULL, scheduler_lease_expires_at_unix_milliseconds = NULL WHERE author_target_identity_digest = ? AND lifecycle_state NOT IN ('succeeded', 'failed') AND (scheduler_checkpoint IS NOT NULL OR scheduler_fence IS NOT NULL OR scheduler_lease_expires_at_unix_milliseconds IS NOT NULL) AND NOT EXISTS (SELECT 1 FROM recovery_fact WHERE recovery_fact.author_target_identity_digest = operation.author_target_identity_digest AND recovery_fact.operation_identifier = operation.operation_identifier AND recovery_fact.manual_resume_eligible = 1)",
        1,
    ),
    InventoriedStatement {
        purpose: "read producer turns for one target",
        text: "SELECT producer_key, turn_sequence FROM producer_turn WHERE author_target_identity_digest = ?",
        parameters: 1,
        maximum_rows: LISTING_ROWS,
    },
    mutation(
        "remove an inactive producer turn",
        "DELETE FROM producer_turn WHERE author_target_identity_digest = ? AND producer_key = ?",
        2,
    ),
    mutation(
        "append or rotate one producer turn",
        "INSERT INTO producer_turn (author_target_identity_digest, producer_key, turn_sequence) VALUES (?, ?, ?) ON CONFLICT (author_target_identity_digest, producer_key) DO UPDATE SET turn_sequence = excluded.turn_sequence",
        3,
    ),
];
