//! Private configuration generation for the explicit compiled runtime host.

pub fn prepare(root: &std::path::Path, profile_name: &str, environments: &[&str]) {
    use sha2::{Digest as _, Sha256};
    use std::io::Write as _;
    use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
    std::fs::create_dir_all(root).unwrap();
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700)).unwrap();
    clear_inherited_extended_access_control_list(root);
    let configuration = root.join("fixture-home/.config/slingshot");
    for path in [
        root.join("fixture-home"),
        root.join("fixture-home/.config"),
        configuration.clone(),
        configuration.join("profiles"),
    ] {
        slingshot_daemon::platform_runtime::current_user::create_owner_only_directory(&path)
            .unwrap();
        clear_inherited_extended_access_control_list(&path);
    }
    let mut profile = format!("format_version = 1\nname = \"{profile_name}\"\n");
    for environment in environments {
        profile.push_str(&format!("\n[environments.{environment}]\ndeployment = \"adobe_experience_manager_6_5\"\n[environments.{environment}.author]\nbase_address = \"http://127.0.0.1:9/{environment}\"\n[environments.{environment}.publisher]\nbase_address = \"http://127.0.0.1:10/{environment}\"\n[environments.{environment}.authentication]\nmethod = \"basic\"\nuser_name = \"fixture\"\npassword = \"not-a-secret\"\n"));
    }
    let inventory = format!(
        "format_version = 1\n[[sources]]\nreference = \"profiles/local.toml\"\nsha256 = \"{}\"\n",
        hex::encode(Sha256::digest(profile.as_bytes()))
    );
    for (relative, bytes) in [
        ("profiles/local.toml", profile.as_bytes()),
        ("configuration-snapshot.toml", inventory.as_bytes()),
    ] {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(configuration.join(relative))
            .unwrap()
            .write_all(bytes)
            .unwrap();
        clear_inherited_extended_access_control_list(&configuration.join(relative));
    }
    use slingshot_configuration::configuration_root::{
        AccountResolver as _, ConfigurationRoot, OperatingSystemAccountResolver,
    };
    let account = OperatingSystemAccountResolver.resolve().unwrap();
    let configured =
        ConfigurationRoot::at_explicit_home(account.identity, root.join("fixture-home"));
    let authority =
        slingshot_configuration::credential_filesystem::UnixConfigurationFilesystem::new(
            configured,
        )
        .unwrap();
    slingshot_configuration::profile_loader::load_profiles(authority)
        .expect("the complete fixture generation verifies");
}

#[cfg(target_os = "macos")]
fn clear_inherited_extended_access_control_list(path: &std::path::Path) {
    let names: Vec<_> =
        xattr::list(path).map(|attributes| attributes.collect()).unwrap_or_default();
    for name in names {
        let _ = xattr::remove(path, name);
    }
    let _ = xattr::remove(path, "com.apple.system.Security");
    let _ = xattr::remove(path, "com.apple.macl");
}

#[cfg(not(target_os = "macos"))]
fn clear_inherited_extended_access_control_list(_path: &std::path::Path) {}
