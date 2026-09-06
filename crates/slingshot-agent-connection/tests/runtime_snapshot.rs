//! Runtime material comes from the same verified generation as its selection.

use sha2::{Digest, Sha256};
use slingshot_agent_connection::authentication::runtime_snapshot::{
    RuntimeSnapshotRefusal, build_runtime_snapshot,
};
use slingshot_configuration::{
    additional_certificate_authority::AdditionalAuthorCertificates,
    platform_trust::{PlatformTrustSource, ProviderDecision, ProviderRecord},
    profile_loader::{ConfigurationDiagnostic, LoadedProfiles, load_profiles},
    profile_selection::RequestedSelection,
    testing::credential_filesystem::ScriptedFilesystem,
};
use slingshot_domain::profile::{EnvironmentName, ProfileName};

struct Platform(std::cell::Cell<usize>);
impl PlatformTrustSource for Platform {
    fn records(&self) -> Result<Vec<ProviderRecord>, ConfigurationDiagnostic> {
        self.0.set(self.0.get() + 1);
        Ok(AdditionalAuthorCertificates::parse(include_bytes!("../../slingshot-test-support/fixtures/additional-certificate-authority/one-authority.pem")).unwrap()
            .certificates().iter().map(|der| ProviderRecord { der:der.clone(), decision:ProviderDecision::UnconditionallyTrustedForServerAuthentication }).collect())
    }
}

fn loaded(password: &str, credential: &[u8]) -> LoadedProfiles {
    loaded_with_certificate(
        password,
        credential,
        include_bytes!(
            "../../slingshot-test-support/fixtures/additional-certificate-authority/other-authority.pem"
        ),
    )
}

fn loaded_with_certificate(
    password: &str,
    credential: &[u8],
    certificate: &[u8],
) -> LoadedProfiles {
    let profile = include_str!(
        "../../slingshot-test-support/fixtures/profile-directories/ordered/profiles/mike.toml"
    )
    .replace("not-a-real-password", password);
    let cloud = include_str!(
        "../../slingshot-test-support/fixtures/profile-directories/ordered/profiles/zulu.toml"
    )
    .replace(
        "[environments.production]",
        "[environments.production]\nadditional_ca_certificate_file = \"certificates/author.pem\"",
    );
    let mut filesystem = ScriptedFilesystem::new()
        .with_directory("profiles")
        .with_directory("credentials")
        .with_directory("certificates");
    let mut inventory = String::from("format_version = 1\n");
    for (name, bytes) in [
        ("certificates/author.pem", certificate),
        ("credentials/alpha.json", credential),
        ("profiles/mike.toml", profile.as_bytes()),
        ("profiles/zulu.toml", cloud.as_bytes()),
    ] {
        filesystem = filesystem.with_source(name, bytes);
        inventory.push_str(&format!(
            "[[sources]]\nreference = \"{name}\"\nsha256 = \"{}\"\n",
            Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>()
        ));
    }
    load_profiles(filesystem.with_source("configuration-snapshot.toml", inventory.as_bytes()))
        .unwrap()
}

fn selection(cloud: bool) -> RequestedSelection {
    RequestedSelection {
        profile: Some(
            ProfileName::parse(if cloud { "alpha-site" } else { "remote-site" }).unwrap(),
        ),
        environment: Some(
            EnvironmentName::parse(if cloud { "production" } else { "staging" }).unwrap(),
        ),
    }
}

#[tokio::test]
async fn basic_uses_selected_generation_and_password_rotation_does_not_move_identity() {
    let platform = Platform(std::cell::Cell::new(0));
    let first = build_runtime_snapshot(
        loaded("first-password", b"unselected-invalid-credential"),
        &selection(false),
        &platform,
    )
    .unwrap();
    let second = build_runtime_snapshot(
        loaded("second-password", b"different-unselected-document"),
        &selection(false),
        &platform,
    )
    .unwrap();
    assert_eq!(first.target(), second.target());
    assert_eq!(first.revision(), second.revision());
    assert_eq!(first.profile_name().as_text(), "remote-site");
    assert_eq!(platform.0.get(), 2);
    assert!(!format!("{first:?}").contains("first-password"));
    struct NoClock;
    impl slingshot_agent_connection::authentication::identity_management_exchange::MonotonicClock
        for NoClock
    {
        fn reading_milliseconds(&self) -> u64 {
            panic!("Basic sampled a token clock")
        }
    }
    impl slingshot_agent_connection::authentication::token_assertion::CoordinatedUniversalTimeClock
        for NoClock
    {
        fn sample(&self) -> Option<u64> {
            panic!("Basic requested an assertion")
        }
    }
    for (snapshot, expected) in [
        (first, b"Basic YWRtaW46Zmlyc3QtcGFzc3dvcmQ=".as_slice()),
        (second, b"Basic YWRtaW46c2Vjb25kLXBhc3N3b3Jk".as_slice()),
    ] {
        let provider = slingshot_agent_connection::authentication::environment_provider::AsyncEnvironmentAuthenticationProvider::new_async(snapshot).unwrap();
        let (authentication, lease) =
            provider.authenticate("http://author.example.com", &NoClock, &NoClock).await.unwrap();
        assert!(lease.is_none());
        authentication.lend_value_bytes(|value| assert_eq!(value, expected));
    }
}

#[test]
fn cloud_parses_retained_credential_and_refuses_invalid_selected_material() {
    let platform = Platform(std::cell::Cell::new(0));
    let credentials =
        include_bytes!("../../slingshot-test-support/fixtures/cloud-credentials/valid.json");
    let selected =
        build_runtime_snapshot(loaded("unused", credentials), &selection(true), &platform).unwrap();
    assert_eq!(selected.profile_name().as_text(), "alpha-site");
    assert_eq!(selected.environment_name().as_text(), "production");
    assert_eq!(platform.0.get(), 1);
    let refused =
        build_runtime_snapshot(loaded("unused", b"not-credentials"), &selection(true), &platform)
            .unwrap_err();
    assert_eq!(refused, RuntimeSnapshotRefusal::Material);
    assert_eq!(platform.0.get(), 1, "invalid selected material reached trust construction");
    assert_eq!(
        build_runtime_snapshot(
            loaded_with_certificate("unused", credentials, b"not-a-certificate"),
            &selection(true),
            &platform
        )
        .unwrap_err(),
        RuntimeSnapshotRefusal::Material
    );
}
