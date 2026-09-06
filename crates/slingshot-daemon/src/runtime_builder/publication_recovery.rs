//! Associates publication evidence with selected retained operation slots.

use slingshot_domain::{command::catalog::CommandCatalog, installation::InstallationIdentifier};
use slingshot_storage::{
    artifact_store::{ArtifactIdentifier, STRUCTURED_RESULT_SLOT},
    persistent_capacity::PendingArtifactPublication,
};
use std::collections::BTreeMap;

/// A startup publication with a known retained owner, not a validated result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredPublication {
    /// Original producer record, including its protected content and timestamp.
    pub publication: PendingArtifactPublication,
    /// The retained local operation that owns this artifact identity. A terminal
    /// owner remains terminal; this association never reactivates it.
    pub operation_identifier: String,
    /// The command-declared or structured-result slot used to derive the identity.
    pub artifact_slot: String,
}

pub(super) fn bind_publications<'a>(
    installation: &InstallationIdentifier,
    target: &str,
    operations: impl Iterator<Item = (&'a str, &'a str)>,
    pending: Vec<PendingArtifactPublication>,
) -> Result<Vec<RecoveredPublication>, ()> {
    if pending.is_empty() {
        return Ok(Vec::new());
    }
    let catalog = CommandCatalog::published();
    let mut owners = BTreeMap::new();
    for (operation, command) in operations {
        // Unrelated historical commands need not exist in this build. A pending
        // publication for such an owner will still refuse as unresolvable below.
        let Some(descriptor) = catalog.find(command) else {
            continue;
        };
        let slots = std::iter::once((STRUCTURED_RESULT_SLOT, descriptor.maximum_result_bytes))
            .chain(
                descriptor
                    .remote_artifact_slots
                    .iter()
                    .map(|slot| (slot.slot.as_text(), slot.maximum_byte_length)),
            );
        for (slot, maximum) in slots {
            let identifier = ArtifactIdentifier::derive(installation, target, operation, slot);
            if owners.insert(identifier.as_text().to_owned(), (operation, slot, maximum)).is_some()
            {
                return Err(());
            }
        }
    }
    pending
        .into_iter()
        .map(|publication| {
            let (operation, slot, maximum) =
                owners.get(publication.artifact_identifier.as_text()).ok_or(())?;
            if publication.byte_length > *maximum {
                return Err(());
            }
            Ok(RecoveredPublication {
                publication,
                operation_identifier: (*operation).to_owned(),
                artifact_slot: (*slot).to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pending(
        installation: &InstallationIdentifier,
        target: &str,
        slot: &str,
    ) -> PendingArtifactPublication {
        PendingArtifactPublication {
            publication_identifier: "producer".into(),
            artifact_identifier: ArtifactIdentifier::derive(
                installation,
                target,
                "operation",
                slot,
            ),
            content_digest: "a".repeat(64),
            byte_length: 2,
            recorded_at_unix_milliseconds: 7,
        }
    }

    #[test]
    fn publication_owner_binding_is_exact_and_preserves_ambiguous_producers() {
        let installation = InstallationIdentifier::parse(&"a".repeat(64)).unwrap();
        let first = pending(&installation, "target", STRUCTURED_RESULT_SLOT);
        let mut second = first.clone();
        second.publication_identifier = "second".into();
        let recovered = bind_publications(
            &installation,
            "target",
            [("operation", "query_paths")].into_iter(),
            vec![first.clone(), second.clone()],
        )
        .unwrap();
        assert_eq!(recovered.len(), 2);
        assert_eq!(recovered[0].publication, first);
        assert_eq!(recovered[1].publication, second);
        assert_eq!(recovered[0].operation_identifier, "operation");
        assert_eq!(recovered[0].artifact_slot, STRUCTURED_RESULT_SLOT);
        for (command, slot) in [
            ("load_content_as_json", "loaded_content_json"),
            ("download_content_package", "content_package"),
        ] {
            let resolved = bind_publications(
                &installation,
                "target",
                [("operation", command), ("unrelated-history", "removed-command")].into_iter(),
                vec![pending(&installation, "target", slot)],
            )
            .unwrap();
            assert_eq!(resolved[0].artifact_slot, slot);
        }
        for (target, owner, command, slot) in [
            ("foreign", "operation", "query_paths", STRUCTURED_RESULT_SLOT),
            ("target", "other", "query_paths", STRUCTURED_RESULT_SLOT),
            ("target", "operation", "unknown", STRUCTURED_RESULT_SLOT),
            ("target", "operation", "query_paths", "content_package"),
        ] {
            assert!(
                bind_publications(
                    &installation,
                    target,
                    [(owner, command)].into_iter(),
                    vec![pending(&installation, "target", slot)]
                )
                .is_err()
            );
        }
        let mut oversized = first;
        oversized.byte_length = u64::MAX;
        assert!(
            bind_publications(
                &installation,
                "target",
                [("operation", "query_paths")].into_iter(),
                vec![oversized]
            )
            .is_err()
        );
        let foreign_installation = InstallationIdentifier::parse(&"b".repeat(64)).unwrap();
        assert!(
            bind_publications(
                &foreign_installation,
                "target",
                [("operation", "query_paths")].into_iter(),
                vec![second]
            )
            .is_err()
        );
    }
}
