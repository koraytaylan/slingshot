//! One invocation, one service, one exit.
//!
//! Not zero and not two. A leaf that fell through would leave a caller with a
//! successful exit and nothing done; one that reached two services would
//! perform an unowned side effect on the way to the one it meant. So the
//! routing is walked exhaustively - every local leaf in the fixture and every
//! published command from the registry - and a leaf that routes nowhere fails
//! here rather than at a caller's terminal.
//!
//! Provenance is checked once, in the dispatcher, rather than in each service.
//! A gate per service is a gate somebody eventually forgets, and the path they
//! forget is the one that reaches a versioned daemon without agreeing with it.
//! The suite asserts that every versioned service is refused when the contracts
//! disagree and that the three unversioned ones still work - because a daemon
//! being absent or incompatible is exactly when somebody runs them.
//!
//! The dispatch matrix and the module inventory are compared against each
//! other, not each against itself, so a family added to one and not the other
//! is a finding.

use slingshot_command_line::application::{
    DispatchRefusal, MAINTENANCE_LEAVES, OBSERVATION_LEAVES, Service, needs_complete_target,
    require_dispatchable, service_for,
};
use slingshot_command_line::invocation::{
    Invocation, LOCAL_LEAVES, METADATA_ONLY_LEAVES, Selection, requires_operation_key,
};
use slingshot_domain::command::catalog::CommandCatalog;

/// Where the dispatch matrix lives.
const FIXTURES: &str = "tests/fixtures/application-dispatch";

/// Where the module inventory the scaffold fixed lives.
const SCAFFOLD_FIXTURE: &str = "tests/fixtures/command-line-module-scaffold/leaves.txt";

/// The parent the command family leaves are declared by.
const COMMAND_FAMILY: &str = "commands";

/// Returns every row of the dispatch matrix.
fn matrix() -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(format!("{FIXTURES}/leaves.jsonl"))
        .expect("the matrix is readable");
    text.lines().map(|line| serde_json::from_str(line).expect("each line is one row")).collect()
}

/// Returns the command families the matrix names, in order.
fn families() -> Vec<String> {
    let text = std::fs::read_to_string(format!("{FIXTURES}/command-families.txt"))
        .expect("the families are readable");
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Returns one invocation of `leaf`, carrying whatever it insists on.
fn invocation(leaf: &str) -> Invocation {
    Invocation {
        arguments: std::collections::BTreeMap::new(),
        detached: false,
        operation_key: requires_operation_key(leaf).then(|| "operation-one".to_owned()),
        output: None,
        selection: Selection::default(),
        verb: leaf.to_owned(),
    }
}

/// Returns how one service is spelled in the matrix.
fn service_spelling(service: Service) -> &'static str {
    match service {
        Service::Metadata => "metadata",
        Service::ConfigurationCheck => "configuration-check",
        Service::DaemonLifecycle => "daemon-lifecycle",
        Service::OperationSubmission => "operation-submission",
        Service::OperationObservation => "operation-observation",
        Service::OperationMaintenance => "operation-maintenance",
    }
}

#[test]
fn every_local_leaf_reaches_the_service_the_matrix_names() {
    for row in matrix() {
        let leaf = row["leaf"].as_str().expect("a leaf");
        let service =
            service_for(&invocation(leaf)).unwrap_or_else(|refusal| panic!("{leaf}: {refusal}"));
        assert_eq!(
            service_spelling(service),
            row["service"].as_str().expect("a service"),
            "{leaf}"
        );
    }
}

#[test]
fn every_leaf_this_build_offers_routes_somewhere_and_nothing_falls_through() {
    for leaf in LOCAL_LEAVES {
        service_for(&invocation(leaf))
            .unwrap_or_else(|refusal| panic!("{leaf} routes nowhere: {refusal}"));
    }
    for descriptor in CommandCatalog::published().descriptors() {
        let leaf = descriptor.wire_name.as_str();
        assert_eq!(
            service_for(&invocation(leaf)).unwrap_or_else(|refusal| panic!("{leaf}: {refusal}")),
            Service::OperationSubmission,
            "{leaf}: a command reaches the daemon by being submitted, like every other"
        );
    }
    assert_eq!(
        service_for(&invocation("teleport")),
        Err(DispatchRefusal::Unroutable { named: "teleport".to_owned() }),
        "and a leaf nobody routes is a defect here rather than a surprise at a terminal"
    );
}

#[test]
fn no_leaf_is_claimed_by_two_services() {
    let mut claimed: Vec<(&str, Service)> = Vec::new();
    for leaf in LOCAL_LEAVES {
        let service = service_for(&invocation(leaf)).expect("it routes");
        assert!(!claimed.iter().any(|(held, _)| held == leaf), "{leaf} is routed more than once");
        claimed.push((leaf, service));
    }
    for leaf in OBSERVATION_LEAVES {
        assert!(LOCAL_LEAVES.contains(leaf), "{leaf} is a local leaf the parser knows");
    }
    for leaf in MAINTENANCE_LEAVES {
        assert!(LOCAL_LEAVES.contains(leaf), "{leaf} is a local leaf the parser knows");
    }
    let overlap = OBSERVATION_LEAVES.iter().any(|leaf| MAINTENANCE_LEAVES.contains(leaf));
    assert!(!overlap, "the two tables are disjoint, so no leaf reaches two services");
}

#[test]
fn provenance_is_checked_once_and_the_three_unversioned_services_survive_it() {
    let catalog = CommandCatalog::published();
    let every: Vec<String> = LOCAL_LEAVES
        .iter()
        .map(|leaf| (*leaf).to_owned())
        .chain(catalog.descriptors().iter().map(|descriptor| descriptor.wire_name.clone()))
        .collect();
    for leaf in &every {
        let asked = invocation(leaf);
        let service = service_for(&asked).expect("it routes");
        let refused = require_dispatchable(&asked, false);
        if service.is_versioned() {
            assert_eq!(
                refused,
                Err(DispatchRefusal::ProvenanceRefused),
                "{leaf}: a gate per service is a gate somebody eventually forgets"
            );
        } else {
            assert_eq!(
                refused,
                Ok(service),
                "{leaf}: a daemon being absent or incompatible is exactly when this is run"
            );
        }
        assert_eq!(require_dispatchable(&asked, true), Ok(service), "{leaf}");
    }
}

#[test]
fn only_the_services_that_talk_to_a_versioned_daemon_are_versioned() {
    for service in [Service::Metadata, Service::ConfigurationCheck, Service::DaemonLifecycle] {
        assert!(!service.is_versioned(), "{service:?} must keep working without a daemon");
    }
    for service in
        [Service::OperationSubmission, Service::OperationObservation, Service::OperationMaintenance]
    {
        assert!(service.is_versioned(), "{service:?} speaks a versioned protocol");
    }
}

#[test]
fn the_target_a_leaf_needs_is_read_from_one_table_and_not_two() {
    for leaf in METADATA_ONLY_LEAVES {
        assert!(!needs_complete_target(&invocation(leaf)), "{leaf} needs no target");
    }
    for descriptor in CommandCatalog::published().descriptors() {
        assert!(
            needs_complete_target(&invocation(&descriptor.wire_name)),
            "{}: it acts against an author",
            descriptor.wire_name
        );
    }
}

#[test]
fn the_dispatch_matrix_and_the_module_inventory_name_the_same_eight_families() {
    let inventory = std::fs::read_to_string(SCAFFOLD_FIXTURE).expect("the scaffold is readable");
    let mut declared: Vec<(usize, String)> = inventory
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let mut parts = line.split('|');
            let path = parts.next()?.to_owned();
            let parent = parts.next()?;
            let position = parts.next()?;
            if parent != COMMAND_FAMILY {
                return None;
            }
            let name = path.rsplit('/').next()?.strip_suffix(".rs")?.to_owned();
            Some((position.parse().ok()?, name))
        })
        .collect();
    declared.sort_by_key(|(position, _)| *position);
    let ordered: Vec<String> = declared.into_iter().map(|(_, name)| name).collect();
    assert_eq!(
        ordered,
        families(),
        "a family added to one fixture and not the other is a finding rather than a detail"
    );
}
