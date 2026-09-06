//! Private, operation-free stages for canonical maintenance documents.

use super::*;
use slingshot_domain::daemon_runtime_contract::{
    DIGEST_OCTETS, MaintenanceResultIdentifier, MaintenanceResultKind,
};

/// Verified document identity, before any association or retention owner exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceDocument {
    /// Target partition; no operation participates in the identity.
    pub target: String,
    /// Canonical document size.
    pub byte_length: u64,
    /// Verified canonical content digest.
    pub content_digest: String,
    /// Domain-separated operation-free identifier.
    pub identifier: MaintenanceResultIdentifier,
    /// Preview or application document.
    pub kind: MaintenanceResultKind,
    /// Reviewed manifest digest.
    pub reviewed_source_digest: String,
}

/// A private synchronized stage. Drop removes only its still-owned stage file.
#[must_use = "retain the stage through publication or drop it to abandon the document"]
pub struct StagedMaintenanceDocument<'store> {
    store: &'store ArtifactStore,
    stage: PrivateStage,
    document: MaintenanceDocument,
}

impl core::fmt::Debug for StagedMaintenanceDocument<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("StagedMaintenanceDocument([redacted])")
    }
}

impl ArtifactStore {
    /// Stages a bounded canonical document after the caller reserves capacity.
    /// This neither advertises a result nor creates an association/publication hold.
    ///
    /// # Errors
    /// Refuses invalid bindings/canonical JSON, excess size, or private-file failure.
    pub fn stage_maintenance_document(
        &self,
        target: &str,
        kind: MaintenanceResultKind,
        source: &str,
        canonical: &[u8],
    ) -> Result<StagedMaintenanceDocument<'_>, ArtifactFailure> {
        let target_bytes = digest_bytes(target)?;
        let source_bytes = digest_bytes(source)?;
        let limits = DaemonRuntimeContract::embedded();
        let maximum = match kind {
            MaintenanceResultKind::Preview => {
                limits.limit("maximum_terminal_maintenance_manifest_bytes")
            }
            MaintenanceResultKind::Application => {
                limits.limit("maximum_canonical_structured_result_bytes")
            }
        };
        let length = canonical.len() as u64;
        if length > maximum {
            return Err(ArtifactFailure::ContentTooLong { actual: length, allowed: maximum });
        }
        slingshot_domain::command::canonical_json::require_canonical_bytes(canonical).map_err(
            |_| {
                ArtifactFailure::FilesystemRefused(
                    "maintenance document is not canonical JSON".to_owned(),
                )
            },
        )?;
        let content: [u8; DIGEST_OCTETS] = Sha256::digest(canonical).into();
        let document = MaintenanceDocument {
            target: target.to_owned(),
            byte_length: length,
            content_digest: hex::encode(content),
            identifier: MaintenanceResultIdentifier::derive(
                &target_bytes,
                kind,
                &source_bytes,
                &content,
            ),
            kind,
            reviewed_source_digest: source.to_owned(),
        };
        let path = self.content.join(format!("{}{STAGING_SUFFIX}", uuid::Uuid::new_v4()));
        let mut file = create_private(&path)?;
        let mut stage = PrivateStage { path, identity: HandleSnapshot::of(&file)? };
        file.write_all(canonical).map_err(refused)?;
        file.sync_all().map_err(refused)?;
        stage.identity = HandleSnapshot::of(&file)?;
        Ok(StagedMaintenanceDocument { store: self, stage, document })
    }
}

impl StagedMaintenanceDocument<'_> {
    /// Identity verified while staging; contains no path or operation identifier.
    #[must_use]
    pub fn document(&self) -> &MaintenanceDocument {
        &self.document
    }

    /// Reads the still-private content through its verified handle.
    ///
    /// # Errors
    /// Refuses changed private identity, digest, or length.
    pub fn open_verified(&self) -> Result<VerifiedArtifactReader, ArtifactFailure> {
        let file = open_without_following(&self.stage.path)?;
        if HandleSnapshot::of(&file)? != self.stage.identity {
            return Err(ArtifactFailure::HandleMoved);
        }
        VerifiedArtifactReader::from_file(
            file,
            &self.document.content_digest,
            self.document.byte_length,
        )
    }

    /// Publishes verified bytes without replacing an existing content object.
    /// The caller must persist a matching publication hold before calling this,
    /// and retain it until association commit. This returns no logical success.
    ///
    /// # Errors
    /// Refuses changed stages, conflicting content, or filesystem failure.
    pub fn publish(self) -> Result<MaintenanceDocument, ArtifactFailure> {
        let file = open_without_following(&self.stage.path)?;
        if HandleSnapshot::of(&file)? != self.stage.identity {
            return Err(ArtifactFailure::HandleMoved);
        }
        self.store.require_existing(
            &self.stage.path,
            &self.document.content_digest,
            self.document.byte_length,
        )?;
        self.store.publish(
            &self.stage.path,
            &self.document.content_digest,
            self.document.byte_length,
            self.stage.identity,
        )?;
        Ok(self.document)
    }
}

fn digest_bytes(text: &str) -> Result<[u8; DIGEST_OCTETS], ArtifactFailure> {
    if !is_canonical_digest(text) {
        return Err(ArtifactFailure::DigestNotCanonical);
    }
    let mut result = [0; DIGEST_OCTETS];
    hex::decode_to_slice(text, &mut result).map_err(|_| ArtifactFailure::DigestNotCanonical)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_stages_publish_identical_content_under_distinct_operation_free_identities() {
        let root = tempfile::tempdir().unwrap();
        let store = ArtifactStore::open(root.path()).unwrap();
        let target = "a".repeat(DIGEST_OCTETS * 2);
        let source = "b".repeat(DIGEST_OCTETS * 2);
        let first = store
            .stage_maintenance_document(&target, MaintenanceResultKind::Preview, &source, b"{}")
            .unwrap();
        let private = first.stage.path.clone();
        let metadata = first.document().clone();
        assert!(private.is_file());
        assert!(!store.content.join(&metadata.content_digest).exists());
        let mut reader = first.open_verified().unwrap();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        reader.finish().unwrap();
        assert_eq!(bytes, b"{}");
        // Physical publication primitive only; durable hold/association handoff
        // is deliberately not claimed by this unit test.
        assert_eq!(first.publish().unwrap(), metadata);
        assert!(!private.exists());
        let second = store
            .stage_maintenance_document(&target, MaintenanceResultKind::Application, &source, b"{}")
            .unwrap();
        assert_ne!(second.document().identifier, metadata.identifier);
        assert_eq!(second.document().content_digest, metadata.content_digest);
        second.publish().unwrap();
        assert_eq!(std::fs::read_dir(&store.content).unwrap().count(), 1);
        let abandoned = store
            .stage_maintenance_document(&target, MaintenanceResultKind::Preview, &source, b"[]")
            .unwrap();
        let path = abandoned.stage.path.clone();
        drop(abandoned);
        assert!(!path.exists());
    }

    #[test]
    fn invalid_and_oversized_documents_create_no_stage_and_changed_stage_cannot_publish() {
        let root = tempfile::tempdir().unwrap();
        let store = ArtifactStore::open(root.path()).unwrap();
        let target = "a".repeat(DIGEST_OCTETS * 2);
        let source = "b".repeat(DIGEST_OCTETS * 2);
        for document in [b" { }".as_slice(), b"{\"a\":1,\"a\":1}", b"not-json"] {
            assert!(
                store
                    .stage_maintenance_document(
                        &target,
                        MaintenanceResultKind::Preview,
                        &source,
                        document
                    )
                    .is_err()
            );
        }
        assert!(
            store
                .stage_maintenance_document(
                    "not-a-target",
                    MaintenanceResultKind::Preview,
                    &source,
                    b"{}"
                )
                .is_err()
        );
        for (kind, limit) in [
            (MaintenanceResultKind::Preview, "maximum_terminal_maintenance_manifest_bytes"),
            (MaintenanceResultKind::Application, "maximum_canonical_structured_result_bytes"),
        ] {
            let maximum = DaemonRuntimeContract::embedded().limit(limit) as usize;
            let exact = format!("\"{}\"", "x".repeat(maximum - 2));
            let stage =
                store.stage_maintenance_document(&target, kind, &source, exact.as_bytes()).unwrap();
            drop(stage);
            let exceeded = format!("\"{}\"", "x".repeat(maximum - 1));
            assert!(matches!(
                store.stage_maintenance_document(&target, kind, &source, exceeded.as_bytes()),
                Err(ArtifactFailure::ContentTooLong { .. })
            ));
        }
        assert_eq!(std::fs::read_dir(&store.content).unwrap().count(), 0);
        let stage = store
            .stage_maintenance_document(&target, MaintenanceResultKind::Preview, &source, b"{}")
            .unwrap();
        let path = stage.stage.path.clone();
        std::fs::OpenOptions::new().write(true).open(&path).unwrap().set_len(0).unwrap();
        assert!(stage.open_verified().is_err());
        assert!(stage.publish().is_err());
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(&store.content).unwrap().count(), 0);
    }
}
