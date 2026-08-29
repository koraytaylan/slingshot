//! Probe for the platform-directories capability.
//!
//! Requires per-user configuration, state, and runtime directories that are
//! absolute and distinct, because a runtime namespace must never fall back to a
//! shared location.

use directories::ProjectDirs;

#[test]
fn the_current_user_directories_are_absolute_and_distinct() {
    let directories =
        ProjectDirs::from("", "", "slingshot").expect("the current user has a home directory");
    let configuration = directories.config_dir().to_path_buf();
    let data = directories.data_dir().to_path_buf();
    assert!(configuration.is_absolute(), "{}", configuration.display());
    assert!(data.is_absolute(), "{}", data.display());
    assert!(configuration.ends_with("slingshot"), "{}", configuration.display());
    if let Some(runtime) = directories.runtime_dir() {
        assert!(runtime.is_absolute(), "{}", runtime.display());
        assert_ne!(runtime, configuration);
    }
}
