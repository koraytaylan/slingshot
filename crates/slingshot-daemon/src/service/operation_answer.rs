//! Operation dispatch while retaining the runtime's synchronous ownership guard.

use super::*;

impl DaemonService {
    pub(super) fn answer_operation(&self, payload: &[u8]) -> ServiceOutcome {
        let ServiceLifetime::Runtime(runtime) = &self.lifetime else {
            return self.render_operation(OperationResponse::ExecutorUnavailable);
        };
        let guard = runtime.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let target = guard.context().target();
        let served = crate::operation_submission::ServedTarget {
            author_target_identity_digest: target.author_target_identity_digest.clone(),
            selected_environment_revision: target.selected_environment_revision.clone(),
            daemon_runtime_contract_digest: target.daemon_runtime_contract_digest.clone(),
            execution_available: true,
        };
        let version = slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
            .operation_protocol_version as u32;
        let bound = match crate::operation_dispatch::BoundRequest::decode(
            &self.contract,
            &served,
            &[version],
            payload,
        ) {
            Ok(bound) => bound,
            Err(response) => return self.render_operation(response),
        };
        match bound.request() {
            OperationRequest::ArtifactRead { .. } => self.answer_artifact(&bound, &guard),
            OperationRequest::MaintenanceResultRead { .. } => {
                self.answer_maintenance_stream(&bound, &guard)
            }
            OperationRequest::Wait { .. } => self.answer_ready_wait(&bound, &guard),
            OperationRequest::Execute { .. } => self.answer_execute(&bound, &guard),
            OperationRequest::TerminalMaintenancePreview {
                before_unix_milliseconds,
                maximum_operations,
                ..
            } => self.answer_maintenance_preview(
                &guard,
                before_unix_milliseconds,
                maximum_operations,
            ),
            OperationRequest::TerminalMaintenanceApply { reviewed_manifest_digest, .. } => {
                self.answer_maintenance_apply(&guard, reviewed_manifest_digest)
            }
            OperationRequest::OperationStatus { .. }
            | OperationRequest::ListOperations { .. }
            | OperationRequest::Result { .. }
            | OperationRequest::ResumeOperationRecovery { .. }
            | OperationRequest::MaintenanceResultMetadata { .. } => {
                self.answer_operation_query(&bound, &guard)
            }
        }
    }

    fn answer_artifact(
        &self,
        bound: &crate::operation_dispatch::BoundRequest,
        guard: &DurableRuntime,
    ) -> ServiceOutcome {
        match bound.artifact(guard.operations(), guard.installation(), guard.artifacts()) {
            Ok(Some(stream)) => self.render_operation_stream(stream),
            Ok(None) => self.render_operation(OperationResponse::MalformedFrame {
                detail: "the artifact request is malformed".to_owned(),
            }),
            Err(response) => self.render_operation(response),
        }
    }

    fn answer_maintenance_stream(
        &self,
        bound: &crate::operation_dispatch::BoundRequest,
        guard: &DurableRuntime,
    ) -> ServiceOutcome {
        match bound.maintenance_read(guard.database(), guard.artifacts()) {
            Ok(Some(stream)) => self.render_operation_stream(stream),
            Ok(None) => self.render_operation(OperationResponse::MalformedFrame {
                detail: "the maintenance request is malformed".to_owned(),
            }),
            Err(response) => self.render_operation(response),
        }
    }

    fn answer_ready_wait(
        &self,
        bound: &crate::operation_dispatch::BoundRequest,
        guard: &DurableRuntime,
    ) -> ServiceOutcome {
        let response = match bound.wait(guard.operations(), guard.waiters()) {
            Ok(Some(mut waiter)) => match waiter.take_ready() {
                Some(crate::operation_wait::WaitUpdate::Progress { detail, revision }) => {
                    OperationResponse::Progress {
                        detail,
                        operation_identifier: bound
                            .request()
                            .operation_identifier()
                            .unwrap_or_default()
                            .to_owned(),
                        operation_revision: revision,
                    }
                }
                Some(crate::operation_wait::WaitUpdate::RecoveryRequired { revision }) => {
                    OperationResponse::Status {
                        lifecycle_state: "recovery_required".to_owned(),
                        operation_identifier: bound
                            .request()
                            .operation_identifier()
                            .unwrap_or_default()
                            .to_owned(),
                        operation_revision: revision,
                    }
                }
                Some(crate::operation_wait::WaitUpdate::Resumed { revision }) => {
                    OperationResponse::Status {
                        lifecycle_state: "resumed".to_owned(),
                        operation_identifier: bound
                            .request()
                            .operation_identifier()
                            .unwrap_or_default()
                            .to_owned(),
                        operation_revision: revision,
                    }
                }
                Some(crate::operation_wait::WaitUpdate::Terminal { revision }) => bound
                    .result(guard.operations(), guard.installation())
                    .unwrap_or(OperationResponse::Status {
                        lifecycle_state: "terminal".to_owned(),
                        operation_identifier: bound
                            .request()
                            .operation_identifier()
                            .unwrap_or_default()
                            .to_owned(),
                        operation_revision: revision,
                    }),
                None => OperationResponse::InternalFailure {
                    detail: "the wait observer was closed before an update was available"
                        .to_owned(),
                },
            },
            Ok(None) => OperationResponse::MalformedFrame {
                detail: "the wait request is malformed".to_owned(),
            },
            Err(response) => response,
        };
        self.render_operation(response)
    }

    fn answer_execute(
        &self,
        bound: &crate::operation_dispatch::BoundRequest,
        guard: &DurableRuntime,
    ) -> ServiceOutcome {
        let response = {
            let prepared = match bound.prepare_admission(guard.installation()) {
                Ok(Some(prepared)) => prepared,
                Ok(None) => {
                    return self.render_operation(OperationResponse::MalformedFrame {
                        detail: "the operation request is malformed".to_owned(),
                    });
                }
                Err(response) => return self.render_operation(response),
            };
            let active = std::collections::BTreeSet::new();
            let now = unix_milliseconds();
            prepared.persist_scheduled(guard.operations(), &active, now).unwrap_or(
                OperationResponse::InternalFailure {
                    detail: "the operation could not be admitted".to_owned(),
                },
            )
        };
        self.render_operation(response)
    }

    fn answer_maintenance_preview(
        &self,
        guard: &DurableRuntime,
        before_unix_milliseconds: &u64,
        maximum_operations: &u32,
    ) -> ServiceOutcome {
        let target = guard.context().target();
        let response = {
            let allowed =
                slingshot_domain::daemon_runtime_contract::DaemonRuntimeContract::embedded()
                    .limit("maximum_terminal_maintenance_operations");
            if *maximum_operations == 0 || u64::from(*maximum_operations) > allowed {
                return self.render_operation(OperationResponse::MalformedFrame {
                    detail: "the maintenance preview bound is outside the runtime limit".to_owned(),
                });
            }
            match slingshot_storage::maintenance::preview(
                guard.database(),
                &target.author_target_identity_digest,
                *before_unix_milliseconds,
                u64::from(*maximum_operations),
            ) {
                Ok(manifest) => {
                    let digest = manifest.digest();
                    let rendered =
                        serde_json::to_value(manifest.clone()).unwrap_or(serde_json::Value::Null);
                    self.reviewed_maintenance
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .insert(digest.clone(), manifest);
                    OperationResponse::MaintenancePreview {
                        manifest: rendered,
                        reviewed_manifest_digest: digest,
                    }
                }
                Err(_) => OperationResponse::InternalFailure {
                    detail: "the maintenance preview could not be read".to_owned(),
                },
            }
        };
        self.render_operation(response)
    }

    fn answer_maintenance_apply(
        &self,
        guard: &DurableRuntime,
        reviewed_manifest_digest: &String,
    ) -> ServiceOutcome {
        let target = guard.context().target();
        let response = {
            let reviewed = self
                .reviewed_maintenance
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(reviewed_manifest_digest)
                .cloned();
            let Some(reviewed) = reviewed else {
                return self.render_operation(OperationResponse::InternalFailure {
                    detail:
                        "the reviewed maintenance manifest is unavailable; request a new preview"
                            .to_owned(),
                });
            };
            match slingshot_storage::maintenance::apply(
                guard.database(),
                &reviewed,
                unix_milliseconds(),
            ) {
                Ok(slingshot_storage::maintenance::ApplyOutcome::Applied(receipt)) => {
                    let result =
                        slingshot_storage::maintenance_results::result_identifiers_for_receipt(
                            guard.database(),
                            &target.author_target_identity_digest,
                            &receipt.application_receipt_identifier,
                        )
                        .ok()
                        .and_then(|mut values| if values.len() == 1 { values.pop() } else { None });
                    self.reviewed_maintenance
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .remove(reviewed_manifest_digest);
                    OperationResponse::MaintenanceApplied {
                        application_receipt_identifier: receipt.application_receipt_identifier,
                        maintenance_result_identifier: result
                            .unwrap_or_else(|| reviewed_manifest_digest.clone()),
                    }
                }
                Ok(slingshot_storage::maintenance::ApplyOutcome::Replayed(receipt)) => {
                    let result =
                        slingshot_storage::maintenance_results::result_identifiers_for_receipt(
                            guard.database(),
                            &target.author_target_identity_digest,
                            &receipt.application_receipt_identifier,
                        )
                        .ok()
                        .and_then(|mut values| if values.len() == 1 { values.pop() } else { None });
                    self.reviewed_maintenance
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .remove(reviewed_manifest_digest);
                    OperationResponse::MaintenanceReplayed {
                        application_receipt_identifier: receipt.application_receipt_identifier,
                        maintenance_result_identifier: result
                            .unwrap_or_else(|| reviewed_manifest_digest.clone()),
                    }
                }
                Err(_) => OperationResponse::InternalFailure {
                    detail: "the reviewed maintenance manifest no longer matches retained state"
                        .to_owned(),
                },
            }
        };
        self.render_operation(response)
    }

    fn answer_operation_query(
        &self,
        bound: &crate::operation_dispatch::BoundRequest,
        guard: &DurableRuntime,
    ) -> ServiceOutcome {
        let response = match bound.request() {
            OperationRequest::OperationStatus { .. } => {
                bound.status(guard.operations()).unwrap_or(OperationResponse::MalformedFrame {
                    detail: "the operation request is malformed".to_owned(),
                })
            }
            OperationRequest::ListOperations { .. } => {
                bound.list(guard.operations()).unwrap_or(OperationResponse::MalformedFrame {
                    detail: "the operation request is malformed".to_owned(),
                })
            }
            OperationRequest::Result { .. } => bound
                .result(guard.operations(), guard.installation())
                .unwrap_or(OperationResponse::MalformedFrame {
                    detail: "the operation request is malformed".to_owned(),
                }),
            OperationRequest::ResumeOperationRecovery { .. } => {
                bound.resume(guard.operations(), unix_milliseconds()).ok().flatten().unwrap_or(
                    OperationResponse::InternalFailure {
                        detail: "the recovery request could not be applied".to_owned(),
                    },
                )
            }
            OperationRequest::MaintenanceResultMetadata { .. } => bound
                .maintenance_metadata(guard.database())
                .unwrap_or(OperationResponse::MalformedFrame {
                    detail: "the maintenance request is malformed".to_owned(),
                }),
            OperationRequest::Wait { .. }
            | OperationRequest::ArtifactRead { .. }
            | OperationRequest::MaintenanceResultRead { .. }
            | OperationRequest::Execute { .. }
            | OperationRequest::TerminalMaintenancePreview { .. }
            | OperationRequest::TerminalMaintenanceApply { .. } => {
                OperationResponse::InternalFailure {
                    detail:
                        "this operation method requires its streaming or maintenance coordinator"
                            .to_owned(),
                }
            }
        };
        self.render_operation(response)
    }
}
