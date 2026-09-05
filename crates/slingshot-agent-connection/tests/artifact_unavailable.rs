//! Unavailable evidence is identity-bound and cannot be inferred from status.
use slingshot_agent_connection::{
    artifact_download::decode_artifact_unavailable,
    command_submission::{ExpectedArtifactManifest, ManifestKind, Submission},
};
use slingshot_agent_protocol::{
    artifact_unavailable::{ArtifactUnavailable, UnavailableReason},
    identity::WireOperationIdentity,
    wire_contract::ExpectedProvenance,
};

#[test]
fn unavailable_documents_match_status_provenance_operation_and_artifact_identity() {
    let provenance = ExpectedProvenance {
        command_contract: slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity::installed("download_content_package").unwrap(),
        canonical_json_contract_digest: slingshot_domain::command::schema::canonical_contract_digest(),
        transport_contract_digest: slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded_digest(),
    };
    let submission = Submission::build(
        &provenance,
        WireOperationIdentity::of(
            &"1".repeat(64),
            &"2".repeat(64),
            "local-one",
            slingshot_domain::agent_identity::AgentEventStoreGeneration::of(7),
        ),
        "subscription-one",
        r#"{"package_name":"example","roots":["/content/example"]}"#,
        ExpectedArtifactManifest::declaring(
            ManifestKind::Package,
            1,
            slingshot_domain::command::artifact::maximum_package_output_bytes(),
        )
        .unwrap(),
    )
    .unwrap();
    let artifact_identifier = "3".repeat(64);
    for (status, reason) in
        [(404, UnavailableReason::Missing), (410, UnavailableReason::RetentionExpired)]
    {
        let document = ArtifactUnavailable {
            provenance: provenance.provenance(),
            agent_event_store_generation: 7,
            agent_operation_identifier: submission.operation.agent_operation_identifier.clone(),
            artifact_identifier: artifact_identifier.clone(),
            artifact_slot: "content_package".to_owned(),
            reason,
        };
        assert_eq!(format!("{document:?}"), "ArtifactUnavailable([redacted])");
        let valid = serde_json::to_vec(&document).unwrap();
        assert_eq!(
            decode_artifact_unavailable(
                status,
                &valid,
                &submission,
                &artifact_identifier,
                "content_package"
            )
            .unwrap()
            .reason(),
            reason
        );
        for wrong_status in [200, 403, 500, if status == 404 { 410 } else { 404 }] {
            assert!(
                decode_artifact_unavailable(
                    wrong_status,
                    &valid,
                    &submission,
                    &artifact_identifier,
                    "content_package"
                )
                .is_err()
            );
        }
        for mutation in 0..10 {
            let mut altered = serde_json::to_value(&document).unwrap();
            match mutation {
                0 => altered["agent_event_store_generation"] = 8.into(),
                1 => altered["agent_operation_identifier"] = "4".repeat(64).into(),
                2 => altered["artifact_identifier"] = "4".repeat(64).into(),
                3 => altered["artifact_slot"] = "loaded_content_json".into(),
                4 => altered["artifact_digest"] = "private-canary".into(),
                5 => altered["reason"] = "unknown".into(),
                6 => altered["provenance"]["transport_contract_digest"] = "4".repeat(64).into(),
                7 => {
                    altered["provenance"]["canonical_json_contract_digest"] = "4".repeat(64).into()
                }
                8 => {
                    altered["provenance"]["command_contract"]["result_schema_digest"] =
                        "4".repeat(64).into()
                }
                9 => {
                    altered.as_object_mut().unwrap().remove("artifact_identifier");
                }
                _ => unreachable!(),
            }
            let refused = decode_artifact_unavailable(
                status,
                &serde_json::to_vec(&altered).unwrap(),
                &submission,
                &artifact_identifier,
                "content_package",
            )
            .unwrap_err();
            assert!(!format!("{refused:?} {refused}").contains("private-canary"));
        }
        let duplicate = String::from_utf8(valid.clone()).unwrap().replacen(
            '{',
            "{\"artifact_slot\":\"content_package\",",
            1,
        );
        for invalid in [duplicate.as_bytes(), b"{}", &valid[..valid.len() - 1]] {
            assert!(
                decode_artifact_unavailable(
                    status,
                    invalid,
                    &submission,
                    &artifact_identifier,
                    "content_package"
                )
                .is_err()
            );
        }
        let maximum = slingshot_domain::author_agent_transport_contract::AuthorAgentTransportContract::embedded().limit("maximum_agent_protocol_document_bytes");
        assert!(
            decode_artifact_unavailable(
                status,
                &vec![b' '; maximum as usize + 1],
                &submission,
                &artifact_identifier,
                "content_package"
            )
            .is_err()
        );
    }
}
