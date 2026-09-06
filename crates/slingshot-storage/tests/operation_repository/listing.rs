//! History filters intersect without escaping the selected target partition.

use crate::fixtures::*;
use slingshot_domain::operation::{OperationLifecycleState, SuccessfulSettlement};
use slingshot_storage::operation::listing::{ListingFilters, list_filtered};

#[test]
fn caller_workflow_and_terminality_filters_intersect() {
    let store = in_memory();
    let target = partition(FIRST_PRINCIPAL);
    let foreign = partition(SECOND_PRINCIPAL);
    for (partition, identifier, caller, workflow, terminal) in [
        (&target, "a-queued", "caller-a", "flow-one", false),
        (&target, "b-terminal", "caller-b", "flow-one", true),
        (&target, "a-terminal", "caller-a", "flow-two", true),
        (&foreign, "foreign", "caller-a", "flow-one", false),
    ] {
        let mut admission =
            request(partition, identifier, r#"{"root_path":"/content"}"#, "revision");
        admission.caller_identity = Some(caller.to_owned());
        admission.workflow_correlation_identifier = Some(workflow.to_owned());
        store.admit(&admission, 1).unwrap();
        if terminal {
            store
                .settle_success(
                    partition,
                    identifier,
                    &SuccessfulSettlement {
                        artifacts: vec![],
                        inline_result: Some("{}".to_owned()),
                        expected_lifecycle_state: OperationLifecycleState::Queued,
                        expected_revision: 1,
                        settled_at_unix_milliseconds: 2,
                    },
                )
                .unwrap();
        }
    }
    let page = |states: &[OperationLifecycleState], caller, terminal, workflow| {
        list_filtered(
            store.database(),
            &target,
            i64::MAX as u64,
            "",
            ListingFilters {
                states,
                caller_identity: caller,
                terminal,
                workflow_correlation_identifier: workflow,
            },
            3,
        )
        .unwrap()
        .into_iter()
        .map(|row| row.operation_identifier)
        .collect::<Vec<_>>()
    };
    assert_eq!(page(&[], None, None, None), ["a-terminal", "b-terminal", "a-queued"]);
    assert_eq!(page(&[], Some("caller-a"), Some(false), Some("flow-one")), ["a-queued"]);
    assert_eq!(page(&[], Some("caller-a"), Some(true), None), ["a-terminal"]);
    assert_eq!(page(&[], None, Some(true), Some("flow-one")), ["b-terminal"]);
    assert!(page(&[OperationLifecycleState::Queued], None, Some(true), None).is_empty());
    assert!(page(&[], Some("missing-caller"), None, None).is_empty());
}
