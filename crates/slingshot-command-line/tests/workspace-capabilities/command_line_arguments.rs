//! Probe for the command-line-arguments capability.
//!
//! Requires derived parsing of a subcommand with required named values, a typed
//! rejection of an unknown argument that names the offending token, and
//! rendered help, all without the default feature set.

use clap::error::ErrorKind;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "probe", version, about = "A capability probe surface")]
struct ProbeArguments {
    #[arg(long)]
    profile: String,
    #[command(subcommand)]
    command: ProbeCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
enum ProbeCommand {
    Start {
        #[arg(long)]
        environment: String,
    },
    Ping,
}

#[test]
fn the_derived_surface_parses_renders_and_refuses_arguments() {
    let parsed = ProbeArguments::try_parse_from([
        "probe",
        "--profile",
        "one",
        "start",
        "--environment",
        "author",
    ])
    .expect("the invocation parses");
    assert_eq!(parsed.profile, "one");
    assert_eq!(parsed.command, ProbeCommand::Start { environment: "author".to_owned() });

    let missing = ProbeArguments::try_parse_from(["probe", "ping"])
        .expect_err("a missing required value is refused");
    assert_eq!(missing.kind(), ErrorKind::MissingRequiredArgument);

    let unknown =
        ProbeArguments::try_parse_from(["probe", "--profile", "one", "--surprise", "ping"])
            .expect_err("an unknown argument is refused");
    assert_eq!(unknown.kind(), ErrorKind::UnknownArgument);
    assert!(unknown.to_string().contains("--surprise"), "{unknown}");

    let rendered = <ProbeArguments as clap::CommandFactory>::command().render_help().to_string();
    assert!(rendered.contains("--profile"), "{rendered}");
    assert!(rendered.contains("start"), "{rendered}");
}
