//! Windows-specific proof that the credential authority is selected from one
//! sampled account identity and keeps its handle-based implementation behind
//! the target boundary.

#![cfg(windows)]

use std::path::PathBuf;

use slingshot_configuration::configuration_root::{AccountIdentity, ConfigurationRoot};
use slingshot_configuration::credential_filesystem::WindowsConfigurationFilesystem;

#[test]
fn the_windows_authority_is_bound_to_the_sampled_security_identifier() {
    let root = ConfigurationRoot::at_explicit_home(
        AccountIdentity::WindowsUser("S-1-5-18".to_owned()),
        PathBuf::from(r"C:\Users\slingshot-test"),
    );
    let authority =
        WindowsConfigurationFilesystem::new(root).expect("the Windows authority is selected");
    let debug = format!("{authority:?}");
    assert!(debug.contains("attempts"));
    assert!(!debug.contains("credentials"), "the authority does not expose source contents");
}
