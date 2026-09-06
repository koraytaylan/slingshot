//! Bounded operation history and target/filter-bound continuation positions.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use slingshot_domain::command::canonical_json::write_canonical;
use slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract;
use slingshot_domain::operation::OperationLifecycleState;
use slingshot_local_protocol::message::{OperationRequest, OperationResponse};
use slingshot_storage::operation_repository::OperationRepository;

use super::{BoundRequest, internal_failure, malformed};

const CURSOR_FORMAT: u32 = 1;
const ORDERING: &str = "enqueue-descending-identifier-ascending";

struct RequestedFilters<'request> {
    lifecycle_states: &'request [String],
    caller_identity: Option<&'request str>,
    terminal: Option<bool>,
    workflow_correlation_identifier: Option<&'request str>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    author_target_identity_digest: String,
    filter_digest: String,
    format_version: u32,
    last_enqueue_sequence: u64,
    last_operation_identifier: String,
    ordering_identifier: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    integrity_digest: String,
    position: Position,
}

fn canonical(value: &impl Serialize) -> Result<String, OperationResponse> {
    write_canonical(&serde_json::to_value(value).map_err(|_| internal_failure())?)
        .map_err(|_| internal_failure())
}

fn checksum(value: &impl Serialize) -> Result<String, OperationResponse> {
    Ok(hex::encode(Sha256::digest(canonical(value)?.as_bytes())))
}

fn encode(position: Position) -> Result<String, OperationResponse> {
    let cursor = Cursor { integrity_digest: checksum(&position)?, position };
    let encoded = hex::encode(canonical(&cursor)?.as_bytes());
    if encoded.len() as u64
        > DaemonRuntimeContract::embedded().limit("maximum_operation_list_cursor_bytes")
    {
        return Err(malformed());
    }
    Ok(encoded)
}

fn decode(cursor: &str, target: &str, filter_digest: &str) -> Result<Position, OperationResponse> {
    if cursor.len() as u64
        > DaemonRuntimeContract::embedded().limit("maximum_operation_list_cursor_bytes")
    {
        return Err(malformed());
    }
    let bytes = hex::decode(cursor).map_err(|_| malformed())?;
    if hex::encode(&bytes) != cursor {
        return Err(malformed());
    }
    let decoded: Cursor = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
    if decoded.integrity_digest != checksum(&decoded.position)?
        || decoded.position.author_target_identity_digest != target
        || decoded.position.filter_digest != filter_digest
        || decoded.position.format_version != CURSOR_FORMAT
        || decoded.position.ordering_identifier != ORDERING
        || decoded.position.last_enqueue_sequence == 0
        || decoded.position.last_enqueue_sequence > i64::MAX as u64
        || decoded.position.last_operation_identifier.is_empty()
        || canonical(&decoded)?.as_bytes() != bytes
    {
        return Err(malformed());
    }
    Ok(decoded.position)
}

fn position(target: &str, filter_digest: &str, sequence: u64, identifier: &str) -> Position {
    Position {
        author_target_identity_digest: target.to_owned(),
        filter_digest: filter_digest.to_owned(),
        format_version: CURSOR_FORMAT,
        last_enqueue_sequence: sequence,
        last_operation_identifier: identifier.to_owned(),
        ordering_identifier: ORDERING.to_owned(),
    }
}

/// Every admitted identifier must fit a continuation at the largest sequence.
/// This derives representability from the cursor bound, not a second id limit.
pub(super) fn identifier_is_representable(target: &str, identifier: &str) -> bool {
    let filter_digest = hex::encode(Sha256::digest(b""));
    encode(position(target, &filter_digest, i64::MAX as u64, identifier)).is_ok()
}

impl BoundRequest {
    /// Returns a bounded history page for the exact requested lifecycle set.
    /// Returns `None` only for a request belonging to another handler.
    #[must_use]
    pub fn list(&self, repository: &OperationRepository) -> Option<OperationResponse> {
        let OperationRequest::ListOperations {
            cursor,
            lifecycle_states,
            page_size,
            caller_identity,
            terminal,
            workflow_correlation_identifier,
        } = self.request()
        else {
            return None;
        };
        Some(
            self.list_page(
                repository,
                cursor.as_deref(),
                RequestedFilters {
                    lifecycle_states,
                    caller_identity: caller_identity.as_deref(),
                    terminal: *terminal,
                    workflow_correlation_identifier: workflow_correlation_identifier.as_deref(),
                },
                *page_size,
            )
            .unwrap_or_else(|refusal| refusal),
        )
    }

    fn list_page(
        &self,
        repository: &OperationRepository,
        cursor: Option<&str>,
        filters: RequestedFilters<'_>,
        page_size: u32,
    ) -> Result<OperationResponse, OperationResponse> {
        let limits = DaemonRuntimeContract::embedded();
        let lifecycle_states = filters.lifecycle_states;
        let filter_values = lifecycle_states.len()
            + usize::from(filters.caller_identity.is_some())
            + usize::from(filters.terminal.is_some())
            + usize::from(filters.workflow_correlation_identifier.is_some());
        if page_size == 0
            || u64::from(page_size) > limits.limit("maximum_operation_list_page_size")
            || filter_values as u64 > limits.limit("maximum_operation_list_filter_values")
            || [filters.caller_identity, filters.workflow_correlation_identifier]
                .into_iter()
                .flatten()
                .any(|value| value.is_empty() || value.contains('\0'))
            || filters.workflow_correlation_identifier.is_some_and(|value| {
                value.len() as u64 > limits.limit("maximum_workflow_correlation_identifier_bytes")
            })
        {
            return Err(malformed());
        }
        let normalized: std::collections::BTreeSet<_> = if lifecycle_states.is_empty() {
            ["queued", "submitting", "accepted", "running", "succeeded", "failed"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        } else {
            lifecycle_states.iter().cloned().collect()
        };
        let states: Vec<OperationLifecycleState> = normalized
            .iter()
            .map(|state| {
                serde_json::from_value(serde_json::Value::String(state.clone()))
                    .map_err(|_| malformed())
            })
            .collect::<Result<_, _>>()?;
        let filter_digest = checksum(&serde_json::json!({
            "caller_identity": filters.caller_identity,
            "lifecycle_states": normalized,
            "terminal": filters.terminal,
            "workflow_correlation_identifier": filters.workflow_correlation_identifier,
        }))?;
        let target = &self.envelope.author_target_identity_digest;
        let (sequence, identifier) = match cursor {
            Some(cursor) => {
                let position = decode(cursor, target, &filter_digest)?;
                (position.last_enqueue_sequence, position.last_operation_identifier)
            }
            None => (i64::MAX as u64, String::new()),
        };
        let rows = slingshot_storage::operation::listing::list_filtered(
            repository.database(),
            target,
            sequence,
            &identifier,
            slingshot_storage::operation::listing::ListingFilters {
                states: &states,
                caller_identity: filters.caller_identity,
                terminal: filters.terminal,
                workflow_correlation_identifier: filters.workflow_correlation_identifier,
            },
            u64::from(page_size),
        )
        .map_err(|_| internal_failure())?;
        let next_cursor = if rows.len() == page_size as usize {
            rows.last()
                .map(|row| {
                    encode(position(
                        target,
                        &filter_digest,
                        row.enqueue_sequence,
                        &row.operation_identifier,
                    ))
                    .map_err(|_| internal_failure())
                })
                .transpose()?
        } else {
            None
        };
        Ok(OperationResponse::ListPage {
            next_cursor,
            operations: rows.into_iter().map(|row| row.operation_identifier).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slingshot_domain::daemon_runtime_contract::DIGEST_OCTETS;

    #[test]
    fn cursor_rejects_wrong_bindings_formats_order_and_tampering() {
        let target = "a".repeat(DIGEST_OCTETS * 2);
        let filters = "b".repeat(DIGEST_OCTETS * 2);
        let cursor = encode(position(&target, &filters, 7, "operation")).unwrap();
        assert_eq!(decode(&cursor, &target, &filters).unwrap().last_enqueue_sequence, 7);
        assert!(decode(&cursor, "another-target", &filters).is_err());
        assert!(decode(&cursor, &target, "another-filter").is_err());
        for defect in ["format", "order", "sequence", "identifier"] {
            let mut value = position(&target, &filters, 7, "operation");
            match defect {
                "format" => value.format_version += 1,
                "order" => value.ordering_identifier = "another-order".to_owned(),
                "sequence" => value.last_enqueue_sequence = 0,
                _ => value.last_operation_identifier.clear(),
            }
            assert!(decode(&encode(value).unwrap(), &target, &filters).is_err());
        }
        let mut bytes = hex::decode(&cursor).unwrap();
        let at = bytes.iter().position(|byte| *byte == b'7').unwrap();
        bytes[at] = b'8';
        assert!(decode(&hex::encode(bytes), &target, &filters).is_err());
        assert!(decode(&cursor.to_uppercase(), &target, &filters).is_err());
    }

    #[test]
    fn admitted_identifier_must_fit_the_largest_cursor_position() {
        let target = "a".repeat(DIGEST_OCTETS * 2);
        let limit =
            DaemonRuntimeContract::embedded().limit("maximum_operation_list_cursor_bytes") as usize;
        assert!(identifier_is_representable(&target, "ordinary-operation"));
        assert!(!identifier_is_representable(&target, &"x".repeat(limit)));
        assert!(!identifier_is_representable(&target, &"\n".repeat(limit)));
        let mut fits = 0;
        let mut exceeds = limit;
        while exceeds - fits > 1 {
            let middle = (fits + exceeds) / 2;
            if identifier_is_representable(&target, &"x".repeat(middle)) {
                fits = middle;
            } else {
                exceeds = middle;
            }
        }
        assert!(identifier_is_representable(&target, &"x".repeat(fits)));
        assert!(!identifier_is_representable(&target, &"x".repeat(fits + 1)));
    }
}
