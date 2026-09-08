//! Terminal-failure envelope identity, canonical bytes and redaction boundaries.

use slingshot_agent_connection::{
    structured_job_result::ResultExpectation, terminal_failure::decode_terminal_failure,
};
use slingshot_agent_protocol::{
    identity::WireOperationIdentity, terminal_failure::TerminalFailureDocument,
    wire_contract::ExpectedProvenance,
};
use slingshot_domain::{
    agent_identity::AgentEventStoreGeneration,
    author_agent_transport_contract::AuthorAgentTransportContract,
    selected_command_contract_identity::SelectedCommandContractIdentity,
};

#[test]
fn targeted_read_failures_bind_subjects_and_selected_categories() {
    use slingshot_agent_connection::terminal_failure::decode_read_failure;
    use slingshot_domain::command::catalog::{Command, CommandCatalog};
    for (arguments, field, subject) in [
        (
            serde_json::json!({"command":"find_open_service_gateway_initiative_configurations"}),
            "",
            "",
        ),
        (serde_json::json!({"command":"find_sling_jobs","states":["error"]}), "", ""),
        (serde_json::json!({"command":"find_workflow_instances","states":["running"]}), "", ""),
        (serde_json::json!({"command":"list_open_service_gateway_initiative_bundles"}), "", ""),
        (serde_json::json!({"command":"list_open_service_gateway_initiative_components"}), "", ""),
        (serde_json::json!({"command":"list_replication_agents"}), "", ""),
        (serde_json::json!({"command":"list_resource_mappings"}), "", ""),
        (serde_json::json!({"command":"list_sling_job_queues"}), "", ""),
        (serde_json::json!({"command":"list_workflow_models"}), "", ""),
        (
            serde_json::json!({"command":"read_content_fragment","fragment_path":"/content/dam/example/offer","variation_name":"web"}),
            "fragment_path",
            "/content/dam/example/offer",
        ),
        (
            serde_json::json!({"command":"list_child_pages","root_path":"/content/example"}),
            "root_path",
            "/content/example",
        ),
        (
            serde_json::json!({"command":"list_group_members","group_identifier":"authors","include_indirect":false}),
            "group_identifier",
            "authors",
        ),
        (
            serde_json::json!({"command":"list_asset_renditions","asset_path":"/content/dam/example/logo.png"}),
            "asset_path",
            "/content/dam/example/logo.png",
        ),
        (
            serde_json::json!({"command":"inspect_replication_queue","agent_identifier":"publish"}),
            "agent_identifier",
            "publish",
        ),
        (
            serde_json::json!({"command":"inspect_sling_job","job_identifier":"2024/01/01/example-job-1"}),
            "job_identifier",
            "2024/01/01/example-job-1",
        ),
        (
            serde_json::json!({"command":"inspect_workflow_instance","instance_identifier":"/var/workflow/instances/server0/2024-01-01/request-for-activation_1"}),
            "instance_identifier",
            "/var/workflow/instances/server0/2024-01-01/request-for-activation_1",
        ),
        (
            serde_json::json!({"command":"inspect_replication_agent","agent_identifier":"publish"}),
            "agent_identifier",
            "publish",
        ),
        (
            serde_json::json!({"command":"resolve_resource_path","include_trace":false,"request_address":"https://example.test/en/report.html"}),
            "subject",
            "https://example.test/en/report.html",
        ),
        (
            serde_json::json!({"command":"map_resource_path","include_trace":false,"repository_path":"/content/example"}),
            "subject",
            "/content/example",
        ),
    ] {
        let initial: Command = serde_json::from_value(arguments.clone()).unwrap();
        let windowed = matches!(
            initial,
            Command::ListChildPages(_)
                | Command::ListGroupMembers(_)
                | Command::ListAssetRenditions(_)
                | Command::InspectReplicationQueue(_)
                | Command::FindOpenServiceGatewayInitiativeConfigurations(_)
                | Command::FindSlingJobs(_)
                | Command::FindWorkflowInstances(_)
                | Command::ListOpenServiceGatewayInitiativeBundles(_)
                | Command::ListOpenServiceGatewayInitiativeComponents(_)
                | Command::ListReplicationAgents(_)
                | Command::ListResourceMappings(_)
                | Command::ListSlingJobQueues(_)
                | Command::ListWorkflowModels(_)
        );
        let mut arguments = arguments;
        if windowed {
            arguments["result_window"] = serde_json::to_value(
                slingshot_domain::command::result_window::ResultWindow::continuation(
                    "opaque-token",
                )
                .unwrap(),
            )
            .unwrap();
        }
        let command: Command = serde_json::from_value(arguments).unwrap();
        let expected = ResultExpectation {
            operation: WireOperationIdentity::of(
                &"a".repeat(64),
                &"b".repeat(64),
                "local-one",
                AgentEventStoreGeneration::of(7),
            ),
            daemon_subscription_identifier: "subscription-one".to_owned(),
            expected_provenance: ExpectedProvenance {
                command_contract: SelectedCommandContractIdentity::installed(command.wire_name())
                    .unwrap(),
                canonical_json_contract_digest:
                    slingshot_domain::command::schema::canonical_contract_digest(),
                transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
            },
            submitted_command_digest: "c".repeat(64),
            wire_name: command.wire_name().to_owned(),
        };
        let decode_with = |value: &serde_json::Value, command: &Command| {
            let document = TerminalFailureDocument {
                operation: expected.operation.clone(),
                daemon_subscription_identifier: expected.daemon_subscription_identifier.clone(),
                provenance: expected.expected_provenance.provenance(),
                submitted_command_digest: expected.submitted_command_digest.clone(),
                canonical_failure: slingshot_domain::command::canonical_json::write_canonical(
                    value,
                )
                .unwrap(),
            };
            decode_read_failure(&serde_json::to_vec(&document).unwrap(), &expected, command)
        };
        let decode = |value: &serde_json::Value| decode_with(value, &command);
        for category in
            &CommandCatalog::published().find(command.wire_name()).unwrap().failure_categories
        {
            let value = if category == "discovery_budget_exceeded" {
                serde_json::json!({"budget":"candidate_nodes","failure":category})
            } else if category == "configuration_lookup_budget_exceeded" {
                serde_json::json!({"budget":"lookup_duration","failure":category})
            } else if category.starts_with("continuation_token_") || field.is_empty() {
                serde_json::json!({"failure":category})
            } else {
                serde_json::json!({"failure":category,field:subject})
            };
            let decoded = decode(&value).unwrap();
            assert_eq!(decoded.category(), category);
            assert_eq!(format!("{decoded:?}"), "ValidatedReadFailure([redacted])");
            for (member, changed) in [
                (field, serde_json::json!(format!("{subject}-other"))),
                ("failure", serde_json::json!("unknown_category")),
                ("private_payload", serde_json::json!("private")),
                ("matches", serde_json::json!([])),
                ("next_continuation_token", serde_json::json!("private")),
            ] {
                let mut invalid = value.clone();
                invalid[member] = changed;
                assert!(decode(&invalid).is_err(), "{member}: {value}");
            }
            if value.get(field).is_some() {
                let mut missing = value.clone();
                missing.as_object_mut().unwrap().remove(field);
                assert!(decode(&missing).is_err());
            }
            if category.starts_with("continuation_token_") {
                assert!(decode_with(&value, &initial).is_err());
            }
            if category == "configuration_lookup_budget_exceeded" {
                assert!(
                    decode(
                        &serde_json::json!({"budget":"matching_configurations","failure":category})
                    )
                    .is_ok()
                );
                assert!(
                    decode(&serde_json::json!({"budget":"property_values","failure":category}))
                        .is_err()
                );
            }
            if category == "variation_not_found" {
                let mut master = command.clone();
                if let Command::ReadContentFragment(asked) = &mut master {
                    asked.variation_name = None;
                }
                assert!(decode_with(&value, &master).is_err());
            }
        }
        if windowed {
            for budget in [
                "candidate_nodes",
                "property_values",
                "property_bytes",
                "criterion_evaluations",
                "execution_duration",
            ] {
                assert!(
                    decode(
                        &serde_json::json!({"budget":budget,"failure":"discovery_budget_exceeded"})
                    )
                    .is_ok()
                );
            }
            assert!(decode(&serde_json::json!({"budget":"result_bytes","failure":"discovery_budget_exceeded"})).is_err());
        }
        if matches!(command, Command::MapResourcePath(_)) {
            assert!(
                decode(
                    &serde_json::json!({"failure":"request_address_rejected","subject":subject})
                )
                .is_err()
            );
        }
        if field.is_empty() {
            // A valid shared inventory variant is still invalid for a different
            // selected command, even though it carries no request subject.
            let foreign_category =
                if matches!(command, Command::FindSlingJobs(_) | Command::ListSlingJobQueues(_)) {
                    "workflow_inventory_failed"
                } else {
                    "job_inventory_failed"
                };
            assert!(decode(&serde_json::json!({"failure":foreign_category})).is_err());
        }
    }
}

#[test]
fn authoring_mutation_failures_use_typed_request_policy() {
    let page = "/content/example/en/report";
    let component = "/content/example/en/report/jcr:content/root/text";
    check_mutation_failure_cases(&[
        (
            serde_json::json!({"command":"create_content_fragment","model_path":"/conf/example/settings/dam/cfm/models/offer","name":"offer","parent_path":"/content/dam/example/fragments"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/create_content_fragment/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"update_content_fragment","elements":{"title":"Spring offer"},"fragment_path":"/content/dam/example/fragments/offer","variation_name":"web"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/update_content_fragment/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"delete_content_fragment","fragment_path":"/content/dam/example/fragments/offer","reference_policy":"refuse_when_referenced"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/delete_content_fragment/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"create_experience_fragment","name":"hero","parent_path":"/content/experience-fragments/example","template_path":"/conf/example/settings/wcm/templates/experience-fragment","variation_name":"web"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/create_experience_fragment/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"update_experience_fragment","title":"Hero","variation_path":"/content/experience-fragments/example/hero/web"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/update_experience_fragment/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"delete_experience_fragment","fragment_path":"/content/experience-fragments/example/hero","reference_policy":"refuse_when_referenced"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/delete_experience_fragment/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"create_asset","name":"logo.png","parent_path":"/content/dam/example","payload":{"media_type":"image/png","encoded_content":"aGVsbG8="}}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/create_asset/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"create_asset_folder","name":"example","parent_path":"/content/dam"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/create_asset_folder/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"move_asset","source_path":"/content/dam/example/logo.png","destination_path":"/content/dam/archive/logo.png","adjust_references":true}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/move_asset/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"delete_asset","asset_path":"/content/dam/example/logo.png","reference_policy":"refuse_when_referenced"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/delete_asset/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"update_asset_metadata","asset_path":"/content/dam/example/logo.png","removed_property_names":["dc:title"]}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/update_asset_metadata/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"update_page","page_path":page,"title":"Annual Report"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/update_page/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"move_page","source_path":page,"destination_path":"/content/archive/report","adjust_references":true}),
            include_str!("../../slingshot-domain/tests/fixtures/commands/move_page/failures.jsonl"),
        ),
        (
            serde_json::json!({"command":"delete_page","page_path":page,"reference_policy":"refuse_when_referenced"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/delete_page/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"update_component","component_path":component,"removed_property_names":["text"]}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/update_component/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"delete_component","component_path":component}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/delete_component/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"reorder_component","component_path":component,"placement":{"mode":"before","sibling_name":"image"}}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/reorder_component/failures.jsonl"
            ),
        ),
    ]);
}

#[test]
fn identity_mutation_failures_use_typed_request_policy() {
    check_mutation_failure_cases(&[
        (
            serde_json::json!({"command":"create_user","authorizable_identifier":"author"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/create_authorizable/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"create_group","authorizable_identifier":"author"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/create_authorizable/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"delete_authorizable","authorizable_identifier":"author","expected_kind":"group"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/delete_authorizable/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"update_user_profile","authorizable_identifier":"author","removed_property_names":["givenName"]}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/update_user_profile/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"set_user_disabled","authorizable_identifier":"author","disabled":true}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/set_user_disabled/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"add_group_member","group_identifier":"content-authors","member_identifier":"author"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/group_membership/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"remove_group_member","group_identifier":"content-authors","member_identifier":"author"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/group_membership/failures.jsonl"
            ),
        ),
    ]);
}

#[test]
fn operational_mutation_failures_use_typed_request_policy() {
    check_mutation_failure_cases(&[
        (
            serde_json::json!({"command":"update_open_service_gateway_initiative_configuration","assignments":{"host":{"cardinality":"scalar","type":"string","value":"example.test"}},"persistent_identifier":"com.example.service.Configuration"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/update_open_service_gateway_initiative_configuration/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"delete_open_service_gateway_initiative_configuration","persistent_identifier":"com.example.service.Configuration"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/delete_open_service_gateway_initiative_configuration/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"set_open_service_gateway_initiative_bundle_state","symbolic_name":"com.example.bundle","transition":"start"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/set_open_service_gateway_initiative_bundle_state/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"cancel_sling_job","job_identifier":"2024/01/01/example-job-1"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/cancel_sling_job/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"start_workflow","model_identifier":"/var/workflow/models/request-for-activation/jcr:content/model","payload_path":"/content/example/en/report"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/start_workflow/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"terminate_workflow_instance","instance_identifier":"/var/workflow/instances/server0/2024-01-01/request-for-activation_1"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/terminate_workflow_instance/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"set_workflow_instance_suspension","instance_identifier":"/var/workflow/instances/server0/2024-01-01/request-for-activation_1","requested_state":"suspended"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/set_workflow_instance_suspension/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"flush_replication_queue","agent_identifier":"publish","expected_entry_count":1}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/flush_replication_queue/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"retry_replication_queue_entry","agent_identifier":"publish","entry_identifier":"queue-entry-1"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/retry_replication_queue_entry/failures.jsonl"
            ),
        ),
    ]);
}

fn check_mutation_failure_cases(cases: &[(serde_json::Value, &str)]) {
    use slingshot_agent_connection::terminal_failure::decode_mutation_failure;
    use slingshot_domain::command::catalog::Command;
    for (arguments, fixtures) in cases {
        let command: Command = serde_json::from_value(arguments.clone()).unwrap();
        let expected = ResultExpectation {
            operation: WireOperationIdentity::of(
                &"a".repeat(64),
                &"b".repeat(64),
                "local-one",
                AgentEventStoreGeneration::of(7),
            ),
            daemon_subscription_identifier: "subscription-one".to_owned(),
            expected_provenance: ExpectedProvenance {
                command_contract: SelectedCommandContractIdentity::installed(command.wire_name())
                    .unwrap(),
                canonical_json_contract_digest:
                    slingshot_domain::command::schema::canonical_contract_digest(),
                transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
            },
            submitted_command_digest: "c".repeat(64),
            wire_name: command.wire_name().to_owned(),
        };
        let mut covered = std::collections::BTreeSet::new();
        for line in fixtures.lines() {
            let fixture: serde_json::Value = serde_json::from_str(line).unwrap();
            let value: serde_json::Value =
                serde_json::from_str(fixture["document"].as_str().unwrap()).unwrap();
            let decode = |value: &serde_json::Value, command: &Command| {
                let document = TerminalFailureDocument {
                    operation: expected.operation.clone(),
                    daemon_subscription_identifier: expected.daemon_subscription_identifier.clone(),
                    provenance: expected.expected_provenance.provenance(),
                    submitted_command_digest: expected.submitted_command_digest.clone(),
                    canonical_failure: slingshot_domain::command::canonical_json::write_canonical(
                        value,
                    )
                    .unwrap(),
                };
                decode_mutation_failure(&serde_json::to_vec(&document).unwrap(), &expected, command)
            };
            let decoded = decode(&value, &command)
                .unwrap_or_else(|_| panic!("{}: {line}", command.wire_name()));
            covered.insert(decoded.category().to_owned());
            assert_eq!(decoded.category(), value["failure"].as_str().unwrap());
            assert_eq!(decoded.proves_no_effect(), fixture["proves_no_effect"].as_bool().unwrap());
            assert_eq!(format!("{decoded:?}"), "ValidatedMutationFailure([redacted])");
            for field in [
                "persistent_identifier",
                "symbolic_name",
                "job_identifier",
                "model_identifier",
                "instance_identifier",
                "agent_identifier",
                "entry_identifier",
                "authorizable_identifier",
                "group_identifier",
                "member_identifier",
                "fragment_path",
                "variation_path",
                "asset_path",
                "target_path",
                "page_path",
                "component_path",
                "source_path",
                "destination_path",
                "failure",
                "private_payload",
            ] {
                let mut changed = value.clone();
                changed[field] = if field.ends_with("_identifier") || field == "symbolic_name" {
                    "another-identity"
                } else {
                    "/unrelated"
                }
                .into();
                assert!(decode(&changed, &command).is_err(), "{field}: {line}");
            }
            let mut changed = arguments.clone();
            match decoded.category() {
                "queue_expectation_mismatch" => {
                    changed.as_object_mut().unwrap().remove("expected_entry_count");
                }
                "group_has_members" => changed["expected_kind"] = "user".into(),
                "target_is_referenced" | "asset_is_referenced" | "fragment_is_referenced" => {
                    changed["reference_policy"] = "ignore_references".into()
                }
                "variation_not_found" if command.wire_name() == "update_content_fragment" => {
                    changed.as_object_mut().unwrap().remove("variation_name");
                }
                "sibling_not_found" => changed["placement"] = serde_json::json!({"mode":"last"}),
                _ => continue,
            }
            assert!(decode(&value, &serde_json::from_value(changed).unwrap()).is_err());
        }
        let catalog = slingshot_domain::command::catalog::CommandCatalog::published();
        let declared: std::collections::BTreeSet<_> =
            catalog.find(command.wire_name()).unwrap().failure_categories.iter().cloned().collect();
        assert_eq!(
            covered, declared,
            "mutation failure fixtures must cover the installed registry exactly"
        );
    }
}

#[test]
fn replication_failures_keep_zero_partial_and_unknown_admission_distinct() {
    use slingshot_agent_connection::terminal_failure::{
        ReplicationFailureEffect, decode_replication_failure,
    };
    use slingshot_domain::command::catalog::Command;
    let command: Command = serde_json::from_value(serde_json::json!({"command":"replicate_content","path":"/content/example","recursive":true})).unwrap();
    let expected = ResultExpectation {
        operation: WireOperationIdentity::of(
            &"a".repeat(64),
            &"b".repeat(64),
            "local-one",
            AgentEventStoreGeneration::of(7),
        ),
        daemon_subscription_identifier: "subscription-one".to_owned(),
        expected_provenance: ExpectedProvenance {
            command_contract: SelectedCommandContractIdentity::installed(command.wire_name())
                .unwrap(),
            canonical_json_contract_digest:
                slingshot_domain::command::schema::canonical_contract_digest(),
            transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
        },
        submitted_command_digest: "c".repeat(64),
        wire_name: command.wire_name().to_owned(),
    };
    let mut cases = Vec::new();
    for category in [
        "source_not_found",
        "source_access_denied",
        "candidate_limit_exceeded",
        "traversal_budget_exceeded",
    ] {
        cases.push((
            serde_json::json!({"failure":category,"source_path":"/content/example"}),
            ReplicationFailureEffect::NoAdmission,
        ));
    }
    for category in ["admission_rejected", "admission_budget_exceeded", "admission_outcome_unknown"]
    {
        for count in [0, 1] {
            cases.push((serde_json::json!({"failure":category,"accepted_item_count":count,"remaining_item_count":1,
                "current_path":if count == 0 { "/content/example" } else { "/content/example/child" }}),
                if category == "admission_outcome_unknown" { ReplicationFailureEffect::Unknown }
                else if count == 0 { ReplicationFailureEffect::NoAdmission } else { ReplicationFailureEffect::PartialAdmission }));
        }
    }
    for (value, effect) in cases {
        let decode = |value: &serde_json::Value| {
            let document = TerminalFailureDocument {
                operation: expected.operation.clone(),
                daemon_subscription_identifier: expected.daemon_subscription_identifier.clone(),
                provenance: expected.expected_provenance.provenance(),
                submitted_command_digest: expected.submitted_command_digest.clone(),
                canonical_failure: slingshot_domain::command::canonical_json::write_canonical(
                    value,
                )
                .unwrap(),
            };
            decode_replication_failure(&serde_json::to_vec(&document).unwrap(), &expected, &command)
        };
        let decoded = decode(&value).unwrap();
        assert_eq!(decoded.effect(), effect);
        assert_eq!(decoded.category(), value["failure"].as_str().unwrap());
        assert_eq!(format!("{decoded:?}"), "ValidatedReplicationFailure([redacted])");
        for (field, invalid) in [
            ("source_path", serde_json::json!("/other")),
            ("current_path", serde_json::json!("/other")),
            ("accepted_item_count", serde_json::json!(u64::MAX)),
            ("remaining_item_count", serde_json::json!(0)),
            ("failure", serde_json::json!("unknown_category")),
            ("private_payload", serde_json::json!("private")),
        ] {
            let mut changed = value.clone();
            changed[field] = invalid;
            assert!(decode(&changed).is_err(), "{field}: {value}");
        }
    }
}

#[test]
fn discovery_failures_correlate_all_six_commands_without_partial_pages() {
    use slingshot_agent_connection::terminal_failure::decode_discovery_failure;
    use slingshot_domain::command::{catalog::Command, result_window::ResultWindow};
    for arguments in [
        serde_json::json!({"command":"query_paths","root_path":"/content/example"}),
        serde_json::json!({"command":"find_pages_by_template","root_path":"/content/example","template_path":"/conf/example/template"}),
        serde_json::json!({"command":"find_pages_containing_phrase","root_path":"/content/example","phrase":"annual report"}),
        serde_json::json!({"command":"find_pages_using_components","root_path":"/content/example","match_mode":"any","resource_types":["example/components/text"]}),
        serde_json::json!({"command":"find_assets_by_metadata","root_path":"/content/example"}),
        serde_json::json!({"command":"find_assets_referenced_by_page","page_path":"/content/example"}),
    ] {
        let command: Command = serde_json::from_value(arguments.clone()).unwrap();
        let expected = ResultExpectation {
            operation: WireOperationIdentity::of(
                &"a".repeat(64),
                &"b".repeat(64),
                "local-one",
                AgentEventStoreGeneration::of(7),
            ),
            daemon_subscription_identifier: "subscription-one".to_owned(),
            expected_provenance: ExpectedProvenance {
                command_contract: SelectedCommandContractIdentity::installed(command.wire_name())
                    .unwrap(),
                canonical_json_contract_digest:
                    slingshot_domain::command::schema::canonical_contract_digest(),
                transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
            },
            submitted_command_digest: "c".repeat(64),
            wire_name: command.wire_name().to_owned(),
        };
        let decode = |value: &serde_json::Value, command: &Command| {
            let document = TerminalFailureDocument {
                operation: expected.operation.clone(),
                daemon_subscription_identifier: expected.daemon_subscription_identifier.clone(),
                provenance: expected.expected_provenance.provenance(),
                submitted_command_digest: expected.submitted_command_digest.clone(),
                canonical_failure: slingshot_domain::command::canonical_json::write_canonical(
                    value,
                )
                .unwrap(),
            };
            decode_discovery_failure(&serde_json::to_vec(&document).unwrap(), &expected, command)
        };
        let page = matches!(command, Command::FindAssetsReferencedByPage(_));
        let mut valid = Vec::new();
        for (category, field, belongs) in [
            ("root_not_found", "root_path", !page),
            ("root_access_denied", "root_path", !page),
            ("page_not_found", "page_path", page),
            ("page_access_denied", "page_path", page),
            ("page_invalid", "page_path", page),
        ] {
            let value = serde_json::json!({"failure":category,field:"/content/example"});
            assert_eq!(decode(&value, &command).is_ok(), belongs);
            if belongs {
                valid.push(value.clone());
            }
            for path in ["/other", "/content/example/child", "/content"] {
                let mut changed = value.clone();
                changed[field] = path.into();
                assert!(decode(&changed, &command).is_err());
            }
        }
        for budget in [
            "candidate_nodes",
            "property_values",
            "property_bytes",
            "criterion_evaluations",
            "execution_duration",
        ] {
            valid.push(serde_json::json!({"failure":"discovery_budget_exceeded","budget":budget}));
        }
        assert!(
            decode(
                &serde_json::json!({"failure":"discovery_budget_exceeded","budget":"result_bytes"}),
                &command
            )
            .is_err()
        );
        let mut continued = arguments;
        continued["result_window"] =
            serde_json::to_value(ResultWindow::continuation("opaque-token").unwrap()).unwrap();
        let continued: Command = serde_json::from_value(continued).unwrap();
        for category in slingshot_domain::command::result_window::CONTINUATION_FAILURE_PRECEDENCE {
            let value = serde_json::json!({"failure":category});
            assert!(decode(&value, &command).is_err());
            assert!(decode(&value, &continued).is_ok());
            let mut partial = value;
            partial["next_continuation_token"] = "private".into();
            assert!(decode(&partial, &continued).is_err());
        }
        for value in valid {
            let decoded = decode(&value, &command).unwrap();
            assert_eq!(decoded.category(), value["failure"].as_str().unwrap());
            assert_eq!(format!("{decoded:?}"), "ValidatedDiscoveryFailure([redacted])");
            for field in ["matches", "next_continuation_token", "count", "private_payload"] {
                let mut partial = value.clone();
                partial[field] = "private".into();
                assert!(decode(&partial, &command).is_err());
            }
        }
    }
}

#[test]
fn configuration_failures_accept_only_content_free_installed_shapes() {
    use slingshot_agent_connection::terminal_failure::decode_configuration_failure;
    use slingshot_domain::command::catalog::Command;
    let command: Command = serde_json::from_value(serde_json::json!({
        "command":"inspect_open_service_gateway_initiative_configuration", "persistent_identifier":"example.service"
    })).unwrap();
    let expected = ResultExpectation {
        operation: WireOperationIdentity::of(
            &"a".repeat(64),
            &"b".repeat(64),
            "local-one",
            AgentEventStoreGeneration::of(7),
        ),
        daemon_subscription_identifier: "subscription-one".to_owned(),
        expected_provenance: ExpectedProvenance {
            command_contract: SelectedCommandContractIdentity::installed(command.wire_name())
                .unwrap(),
            canonical_json_contract_digest:
                slingshot_domain::command::schema::canonical_contract_digest(),
            transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
        },
        submitted_command_digest: "c".repeat(64),
        wire_name: command.wire_name().to_owned(),
    };
    for line in include_str!("../../slingshot-domain/tests/fixtures/commands/inspect_open_service_gateway_initiative_configuration/failures.jsonl").lines() {
        let fixture: serde_json::Value = serde_json::from_str(line).unwrap();
        let value: serde_json::Value = serde_json::from_str(fixture["document"].as_str().unwrap()).unwrap();
        let mut document = TerminalFailureDocument {
            operation: expected.operation.clone(), daemon_subscription_identifier: expected.daemon_subscription_identifier.clone(),
            provenance: expected.expected_provenance.provenance(), submitted_command_digest: expected.submitted_command_digest.clone(),
            canonical_failure: slingshot_domain::command::canonical_json::write_canonical(&value).unwrap(),
        };
        let decoded = decode_configuration_failure(&serde_json::to_vec(&document).unwrap(), &expected, &command).unwrap();
        assert_eq!(decoded.category(), value["failure"].as_str().unwrap());
        assert_eq!(format!("{decoded:?}"), "ValidatedConfigurationFailure([redacted])");
        for field in ["persistent_identifier", "key", "value", "filter", "properties", "budget", "reason", "failure"] {
            let mut invalid = value.clone();
            invalid[field] = "private-unregistered-value".into();
            document.canonical_failure = slingshot_domain::command::canonical_json::write_canonical(&invalid).unwrap();
            assert!(decode_configuration_failure(&serde_json::to_vec(&document).unwrap(), &expected, &command).is_err(), "{field}: {line}");
        }
    }
}

#[test]
fn content_failures_are_closed_correlated_and_keep_publication_uncertainty() {
    use slingshot_agent_connection::terminal_failure::{
        decode_load_failure, decode_package_failure,
    };
    use slingshot_domain::command::catalog::Command;
    for (arguments, fixtures, package) in [
        (
            serde_json::json!({"command":"load_content_as_json","path":"/content/example","depth":1}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/load_content_as_json/failures.jsonl"
            ),
            false,
        ),
        (
            serde_json::json!({"command":"download_content_package","package_name":"example","roots":["/content/example"],"inclusion_filters":["/content/example/a/(.*)","/content/example/b/(.*)","/content/example/c/(.*)"],"exclusion_filters":["/content/example/a/private/(.*)","/content/example/b/private/(.*)","/content/example/c/private/(.*)"]}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/download_content_package/failures.jsonl"
            ),
            true,
        ),
    ] {
        let command: Command = serde_json::from_value(arguments).unwrap();
        let expected = ResultExpectation {
            operation: WireOperationIdentity::of(
                &"a".repeat(64),
                &"b".repeat(64),
                "local-one",
                AgentEventStoreGeneration::of(7),
            ),
            daemon_subscription_identifier: "subscription-one".to_owned(),
            expected_provenance: ExpectedProvenance {
                command_contract: SelectedCommandContractIdentity::installed(command.wire_name())
                    .unwrap(),
                canonical_json_contract_digest:
                    slingshot_domain::command::schema::canonical_contract_digest(),
                transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
            },
            submitted_command_digest: "c".repeat(64),
            wire_name: command.wire_name().to_owned(),
        };
        for line in fixtures.lines() {
            let fixture: serde_json::Value = serde_json::from_str(line).unwrap();
            let mut document = TerminalFailureDocument {
                operation: expected.operation.clone(),
                daemon_subscription_identifier: expected.daemon_subscription_identifier.clone(),
                provenance: expected.expected_provenance.provenance(),
                submitted_command_digest: expected.submitted_command_digest.clone(),
                canonical_failure: fixture["document"].as_str().unwrap().to_owned(),
            };
            let original: serde_json::Value =
                serde_json::from_str(&document.canonical_failure).unwrap();
            // Domain fixtures pin closed shapes, not transport canonical ordering.
            document.canonical_failure =
                slingshot_domain::command::canonical_json::write_canonical(&original).unwrap();
            let bytes = serde_json::to_vec(&document).unwrap();
            if package {
                let decoded = decode_package_failure(&bytes, &expected, &command).unwrap();
                assert_eq!(decoded.category(), original["failure"].as_str().unwrap());
                assert_eq!(
                    decoded.proves_no_publication(),
                    fixture["proves_no_publication"].as_bool().unwrap()
                );
                assert_eq!(format!("{decoded:?}"), "ValidatedPackageFailure([redacted])");
                assert!(decode_load_failure(&bytes, &expected, &command).is_err());
            } else {
                let decoded = decode_load_failure(&bytes, &expected, &command).unwrap();
                assert_eq!(decoded.category(), original["failure"].as_str().unwrap());
                assert_eq!(format!("{decoded:?}"), "ValidatedLoadFailure([redacted])");
                assert!(decode_package_failure(&bytes, &expected, &command).is_err());
            }
            let rejected = |document: &TerminalFailureDocument, expected: &ResultExpectation| {
                let bytes = serde_json::to_vec(document).unwrap();
                if package {
                    decode_package_failure(&bytes, expected, &command).is_err()
                } else {
                    decode_load_failure(&bytes, expected, &command).is_err()
                }
            };
            let mut drifted = expected.clone();
            drifted.expected_provenance.transport_contract_digest = "d".repeat(64);
            let mut matching_drift = document.clone();
            matching_drift.provenance = drifted.expected_provenance.provenance();
            assert!(rejected(&matching_drift, &drifted));
            for (field, changed) in [
                ("failure", serde_json::json!("unknown_category")),
                ("private_payload", serde_json::json!("private")),
                ("path", serde_json::json!("/unrelated")),
                ("root_path", serde_json::json!("/unrelated")),
                ("expression_index", serde_json::json!(u64::MAX)),
            ] {
                let mut invalid = original.clone();
                invalid[field] = changed;
                document.canonical_failure =
                    slingshot_domain::command::canonical_json::write_canonical(&invalid).unwrap();
                assert!(rejected(&document, &expected), "{field}: {line}");
            }
        }
    }
}

#[test]
fn creation_failures_use_closed_types_and_the_retained_computed_target() {
    use slingshot_agent_connection::terminal_failure::decode_creation_failure;
    use slingshot_domain::command::catalog::Command;
    let cases = [
        (
            serde_json::json!({"command":"create_page","parent_path":"/content/example/en","page_name":"annual-report","template_path":"/conf/example/templates/page","title":"Annual Report"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/create_page/failures.jsonl"
            ),
        ),
        (
            serde_json::json!({"command":"add_component","page_path":"/content/example/en","content_parent":"content_root","component_name":"text","resource_type":"example/components/text"}),
            include_str!(
                "../../slingshot-domain/tests/fixtures/commands/add_component/failures.jsonl"
            ),
        ),
    ];
    for (arguments, fixtures) in cases {
        let command: Command = serde_json::from_value(arguments).unwrap();
        let expected = ResultExpectation {
            operation: WireOperationIdentity::of(
                &"a".repeat(64),
                &"b".repeat(64),
                "local-one",
                AgentEventStoreGeneration::of(7),
            ),
            daemon_subscription_identifier: "subscription-one".to_owned(),
            expected_provenance: ExpectedProvenance {
                command_contract: SelectedCommandContractIdentity::installed(command.wire_name())
                    .unwrap(),
                canonical_json_contract_digest:
                    slingshot_domain::command::schema::canonical_contract_digest(),
                transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
            },
            submitted_command_digest: "c".repeat(64),
            wire_name: command.wire_name().to_owned(),
        };
        for line in fixtures.lines() {
            let fixture: serde_json::Value = serde_json::from_str(line).unwrap();
            let mut document = TerminalFailureDocument {
                operation: expected.operation.clone(),
                daemon_subscription_identifier: expected.daemon_subscription_identifier.clone(),
                provenance: expected.expected_provenance.provenance(),
                submitted_command_digest: expected.submitted_command_digest.clone(),
                canonical_failure: fixture["document"].as_str().unwrap().to_owned(),
            };
            let decoded = decode_creation_failure(
                &serde_json::to_vec(&document).unwrap(),
                &expected,
                &command,
            )
            .unwrap();
            assert_eq!(decoded.proves_no_effect(), fixture["proves_no_effect"].as_bool().unwrap());
            let original: serde_json::Value =
                serde_json::from_str(&document.canonical_failure).unwrap();
            assert_eq!(decoded.category(), original["failure"].as_str().unwrap());
            assert_eq!(format!("{decoded:?}"), "ValidatedCreationFailure([redacted])");
            for (field, changed) in [
                ("target_path", serde_json::json!("/content/another")),
                ("failure", serde_json::json!("unregistered_remote_category")),
                ("private_payload", serde_json::json!("must not be accepted")),
            ] {
                let mut invalid = original.clone();
                invalid[field] = changed;
                document.canonical_failure =
                    slingshot_domain::command::canonical_json::write_canonical(&invalid).unwrap();
                assert!(
                    decode_creation_failure(
                        &serde_json::to_vec(&document).unwrap(),
                        &expected,
                        &command
                    )
                    .is_err()
                );
            }
        }
    }
}

#[test]
fn failure_envelope_binds_identity_preserves_bytes_and_exposes_no_details() {
    let expected = ResultExpectation {
        operation: WireOperationIdentity::of(
            &"a".repeat(64),
            &"b".repeat(64),
            "local-one",
            AgentEventStoreGeneration::of(7),
        ),
        daemon_subscription_identifier: "subscription-one".to_owned(),
        expected_provenance: ExpectedProvenance {
            command_contract: SelectedCommandContractIdentity::installed("create_page").unwrap(),
            canonical_json_contract_digest:
                slingshot_domain::command::schema::canonical_contract_digest(),
            transport_contract_digest: AuthorAgentTransportContract::embedded_digest(),
        },
        submitted_command_digest: "c".repeat(64),
        wire_name: "create_page".to_owned(),
    };
    let document = TerminalFailureDocument {
        operation: expected.operation.clone(),
        daemon_subscription_identifier: expected.daemon_subscription_identifier.clone(),
        provenance: expected.expected_provenance.provenance(),
        submitted_command_digest: expected.submitted_command_digest.clone(),
        canonical_failure: r#"{"failure":"parent_not_found","target_path":"/content/private"}"#
            .to_owned(),
    };
    let bytes = serde_json::to_vec(&document).unwrap();
    assert_eq!(decode_terminal_failure(&bytes, &expected).unwrap(), document);
    assert_eq!(format!("{document:?}"), "TerminalFailureDocument([redacted])");
    let value = serde_json::to_value(&document).unwrap();
    for pointer in [
        "/operation/agent_operation_identifier",
        "/operation/author_target_identity_digest",
        "/operation/selected_environment_revision",
        "/submitted_command_digest",
        "/daemon_subscription_identifier",
        "/provenance/transport_contract_digest",
        "/provenance/canonical_json_contract_digest",
        "/provenance/command_contract/argument_schema_digest",
        "/provenance/command_contract/result_schema_digest",
        "/provenance/command_contract/command_contract_limits_digest",
        "/provenance/command_contract/command_semantic_contract_version",
        "/provenance/command_contract/command_wire_name",
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(pointer).unwrap() = "d".repeat(64).into();
        assert!(
            decode_terminal_failure(&serde_json::to_vec(&changed).unwrap(), &expected).is_err(),
            "{pointer}"
        );
    }
    for key in value.as_object().unwrap().keys() {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(
            decode_terminal_failure(&serde_json::to_vec(&missing).unwrap(), &expected).is_err()
        );
    }
    let mut generation = value.clone();
    generation["operation"]["agent_event_store_generation"] = 8.into();
    assert!(decode_terminal_failure(&serde_json::to_vec(&generation).unwrap(), &expected).is_err());
    for payload in
        [" {\"failure\":\"x\"}", "{\"failure\":\"a\",\"failure\":\"b\"}", "{", "{} trailing"]
    {
        let mut changed = value.clone();
        changed["canonical_failure"] = payload.into();
        assert!(
            decode_terminal_failure(&serde_json::to_vec(&changed).unwrap(), &expected).is_err()
        );
    }
    let mut changed = value.clone();
    changed["declared_artifacts"] = serde_json::json!([]);
    assert!(decode_terminal_failure(&serde_json::to_vec(&changed).unwrap(), &expected).is_err());
    let duplicate = format!(
        "{{\"canonical_failure\":\"{{}}\",{}",
        &String::from_utf8(bytes.clone()).unwrap()[1..]
    );
    assert!(decode_terminal_failure(duplicate.as_bytes(), &expected).is_err());
    assert!(decode_terminal_failure(&bytes[..bytes.len() - 1], &expected).is_err());
    let oversized = vec![
        b' ';
        slingshot_agent_connection::structured_job_result::maximum_document_bytes()
            as usize
            + 1
    ];
    assert!(decode_terminal_failure(&oversized, &expected).is_err());
}
