//! Shared access to the committed test provider records.

use std::path::PathBuf;

use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject;

/// Directory holding the committed provider records.
const CERTIFICATE_DIRECTORY: &str = "tests/workspace-capabilities/certificates";

/// Returns the path of one committed provider record.
pub fn certificate_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CERTIFICATE_DIRECTORY).join(name)
}

/// Reads one committed provider record as its encoded bytes.
pub fn certificate(name: &str) -> CertificateDer<'static> {
    CertificateDer::from_pem_file(certificate_path(&format!("{name}.pem")))
        .unwrap_or_else(|failure| panic!("{name} is a readable provider record: {failure}"))
}
