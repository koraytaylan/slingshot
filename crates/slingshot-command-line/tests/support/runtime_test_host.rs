//! Explicit test process host; the product binary never reads these sources.

use slingshot_command_line::{
    command_line,
    daemon_entry::{self, DaemonEntryArguments, DaemonEntryOutcome},
};
use slingshot_configuration::configuration_root::{
    AccountResolver, ConfigurationRoot, OperatingSystemAccountResolver,
};
use slingshot_local_protocol::foundation_contract::FoundationContract;

fn main() -> std::process::ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let named = command_line::normalized(&arguments);
    let code = if named.first().is_some_and(|leaf| leaf == "daemon-serve") {
        let value = |key: &str| {
            named.windows(2).find(|pair| pair[0] == key).map(|pair| pair[1].as_str()).unwrap_or("")
        };
        let runtime_root = std::path::PathBuf::from(value("--runtime-root"));
        let entry =
            DaemonEntryArguments::new(&runtime_root, value("--profile"), value("--environment"));
        let account = OperatingSystemAccountResolver.resolve().expect("test account resolves");
        let root = ConfigurationRoot::at_explicit_home(
            account.identity,
            runtime_root.join("fixture-home"),
        );
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("test runtime builds");
        match runtime.block_on(daemon_entry::run_daemon_entry_for_test(
            &FoundationContract::embedded(),
            &entry,
            tokio_util::sync::CancellationToken::new(),
            root,
            runtime_root.join("state"),
        )) {
            Ok(DaemonEntryOutcome::Served) => command_line::EXIT_SUCCESS,
            Ok(DaemonEntryOutcome::AlreadyOwned) => command_line::EXIT_ALREADY_OWNED,
            Err(failure) => {
                eprintln!("slingshot: {failure}");
                command_line::EXIT_RUNTIME_UNUSABLE
            }
        }
    } else {
        let executable = std::env::current_exe().expect("test host resolves itself");
        let code = command_line::run(
            &arguments,
            &executable,
            &mut std::io::stdout().lock(),
            &mut std::io::stderr().lock(),
        );
        u8::try_from(code).unwrap_or(command_line::EXIT_RUNTIME_UNUSABLE)
    };
    std::process::ExitCode::from(code)
}
