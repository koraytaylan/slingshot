//! Compiled daemon startup must execute work admitted through its actual CLI.

use super::{
    ENVIRONMENT, PROFILE, configured_runtime_root, cooperatively_stop, product_executable,
    run_product,
};
use sha2::{Digest as _, Sha256};
use slingshot_local_protocol::foundation_contract::FoundationContract;
use slingshot_test_support::process_harness::{ProcessHarness, ProcessRequest};
use slingshot_test_support::runtime_harness::wait_until;

#[test]
fn compiled_daemon_starts_the_scheduler_and_joins_it_on_stop() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let root = configured_runtime_root("compiled-scheduler");
    let configuration = root.path().join("fixture-home/.config/slingshot");
    let profile_path = configuration.join("profiles/local.toml");
    let profile = std::fs::read_to_string(&profile_path)
        .unwrap()
        .replace("127.0.0.1:9/", &format!("{}/", listener.local_addr().unwrap()));
    std::fs::write(&profile_path, &profile).unwrap();
    std::fs::write(
        configuration.join("configuration-snapshot.toml"),
        format!(
            "format_version = 1\n[[sources]]\nreference = \"profiles/local.toml\"\nsha256 = \"{}\"\n",
            hex::encode(Sha256::digest(profile.as_bytes()))
        ),
    )
    .unwrap();
    let started = run_product(&root, ENVIRONMENT, "start");
    assert!(started.status.success(), "{started:?}");
    let contract = FoundationContract::embedded();
    let submitted = ProcessHarness::new().run_within(
        &product_executable(),
        &ProcessRequest::new(&[
            "--runtime-root",
            root.path().to_str().unwrap(),
            "--profile",
            PROFILE,
            "--environment",
            ENVIRONMENT,
            "--machine",
            "query_paths",
            "--path",
            "/content",
            "--detach",
        ]),
        contract.startup.explicit_start_total(),
    );
    let connected =
        wait_until(contract.startup.explicit_start_total(), || listener.accept().is_ok());
    cooperatively_stop(&root, ENVIRONMENT);
    let submitted = submitted.expect("the admitted invocation finishes");
    assert!(submitted.status.success(), "{submitted:?}");
    assert!(connected, "the compiled daemon never executed its queued admission");
}
