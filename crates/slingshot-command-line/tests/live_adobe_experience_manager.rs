//! The harness that reaches a real author, and everything that stops it.
//!
//! Two properties matter more than anything else here and the suite is arranged
//! around them. The first is that nothing happens without somebody saying so:
//! an ordinary run of this suite reaches no configuration, no credential, no
//! daemon, and no network, and the enablement decision is made from the parsed
//! invocation alone so there is nowhere for an accident to hide. The second is
//! that what a verification may run is the registry's own answer rather than a
//! list kept here, and that the column deciding retries is never read as the
//! column deciding access.
//!
//! The twelve rows are restated in a fixture written independently of the
//! registry. That is the point: two descriptions that were produced from one
//! source would agree no matter what either said.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;
use slingshot_command_line::application::{Service, service_for};
use slingshot_command_line::command_line::normalized;
use slingshot_command_line::configuration_check::ResolvedFacts;
use slingshot_command_line::exit_classification::{LOCAL_FAILURE, SUCCESS, UNAVAILABLE, USAGE};
use slingshot_command_line::invocation::{
    CONTENT_ROOT_OPTION, ENABLE_LIVE_AUTHOR_OPTION, Invocation, LIVE_AUTHOR_LEAF, LOCAL_LEAVES,
    OutputForm, PATH_OPTION, PHRASE_OPTION, ParseRefusal, Selection, leaves_taking, parse,
    required_options, requires_operation_key,
};
use slingshot_command_line::live_adobe_experience_manager::{
    Admission, CONTENT_TREE, ConfigurationConformance, DurationCategory, EXERCISES, Enablement,
    Evidence, LiveRefusal, OPTIONAL_EXERCISES, OfferedCapability, ResultClassification,
    SUBMITTED_COMMANDS, VERIFICATION_PHRASE, admissible, admission_for, exercise_invocation,
    live_report, refused, require_admissible, require_agreement,
};
use slingshot_domain::command::catalog::{
    AccessClassification, CommandCatalog, DestructiveClassification,
    IntrinsicIdempotencyClassification,
};
use slingshot_domain::command::schema::{SchemaRole, canonical_contract_digest};
use slingshot_domain::profile::AdobeExperienceManagerDeployment;
use slingshot_domain::selected_command_contract_identity::SelectedCommandContractIdentity;

/// Where the fixtures live.
const FIXTURES: &str = "tests/fixtures/live-adobe-experience-manager";

/// The profile the fixtures name.
const PROFILE: &str = "local";

/// The environment the fixtures name.
const ENVIRONMENT: &str = "author";

/// The content root the fixtures verify under.
const ROOT: &str = "/content/site/en";

/// How many commands the registry holds.
const REGISTRY_ROWS: usize = 12;

/// How many of them a verification may run.
const ADMISSIBLE_ROWS: usize = 9;

/// How many of them it refuses.
const REFUSED_ROWS: usize = 3;

/// How many claims a conformance trace makes.
const CONFORMANCE_CLAIMS: usize = 9;

/// How many ways an offered identity is drifted, one field at a time.
const DRIFTED_FIELDS: usize = 4;

/// How many fields a report carries.
const REPORT_FIELDS: usize = 9;

/// The operation key the fixtures supply.
const KEY: &str = "command-line-0";

/// Returns one fixture's lines, without its comments.
fn fixture_rows(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURES).join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{name} could not be read: {failure}"))
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str(line).expect("every row reads"))
        .collect()
}

/// Returns the invocation `words` parse into.
fn invocation(words: &[&str]) -> Invocation {
    parse(&words.iter().map(|word| (*word).to_owned()).collect::<Vec<String>>())
        .expect("the words parse")
}

/// Returns an enabled invocation naming `root`.
fn enabled_with(root: &str) -> Invocation {
    invocation(&[
        LIVE_AUTHOR_LEAF,
        "--profile",
        PROFILE,
        "--environment",
        ENVIRONMENT,
        ENABLE_LIVE_AUTHOR_OPTION,
        CONTENT_ROOT_OPTION,
        root,
    ])
}

/// Returns the enablement an ordinary enabled invocation carries.
fn enablement() -> Enablement {
    Enablement::read(&enabled_with(ROOT)).expect("this invocation enables a verification")
}

/// Returns which refusal one failure is.
fn refusal_name(failure: &LiveRefusal) -> &'static str {
    enablement_refusal_name(failure).unwrap_or_else(|| agreement_refusal_name(failure))
}

/// Returns the name of one refusal that stops a verification before it starts.
fn enablement_refusal_name(failure: &LiveRefusal) -> Option<&'static str> {
    let named = match failure {
        LiveRefusal::NotEnabled => "NotEnabled",
        LiveRefusal::SelectionAbsent(_) => "SelectionAbsent",
        LiveRefusal::ContentRootUnusable { .. } => "ContentRootUnusable",
        LiveRefusal::ContentRootElsewhere(_) => "ContentRootElsewhere",
        LiveRefusal::NotAdmissible { .. } => "NotAdmissible",
        LiveRefusal::UnknownCommand(_) => "UnknownCommand",
        _ => return None,
    };
    Some(named)
}

/// Returns the name of one refusal about what the agent turned out to be.
fn agreement_refusal_name(failure: &LiveRefusal) -> &'static str {
    match failure {
        LiveRefusal::ContractUnavailable { .. } => "ContractUnavailable",
        LiveRefusal::IdentityDrift { .. } => "IdentityDrift",
        LiveRefusal::CanonicalContractDrift { .. } => "CanonicalContractDrift",
        LiveRefusal::CanonicalContractAnnotationAbsent { .. } => {
            "CanonicalContractAnnotationAbsent"
        }
        LiveRefusal::ConformanceNotAttested(_) => "ConformanceNotAttested",
        _ => "unreachable",
    }
}

// ------------------------------------------------- nothing without being told

/// Returns the invocation `leaf` names with no options at all.
fn bare(leaf: &str) -> Invocation {
    Invocation {
        arguments: BTreeMap::new(),
        detached: false,
        operation_key: requires_operation_key(leaf).then(|| KEY.to_owned()),
        output: None,
        selection: Selection::default(),
        verb: leaf.to_owned(),
    }
}

#[test]
fn without_the_enabling_option_no_verification_is_enabled() {
    let mut asked = bare(LIVE_AUTHOR_LEAF);
    asked.selection.profile = Some(PROFILE.to_owned());
    assert_eq!(
        Enablement::read(&asked),
        Err(LiveRefusal::NotEnabled),
        "reaching somebody's author is something a caller asks for out loud"
    );
}

#[test]
fn the_parser_refuses_the_leaf_without_both_of_the_options_it_needs() {
    let words = [LIVE_AUTHOR_LEAF, "--profile", PROFILE, "--environment", ENVIRONMENT];
    let refusal = parse(&words.iter().map(|word| (*word).to_owned()).collect::<Vec<String>>())
        .expect_err("both options are required");
    assert!(
        matches!(refusal, ParseRefusal::RequiredOptionMissing { .. }),
        "{refusal} names the option that is missing"
    );
    assert_eq!(
        required_options(LIVE_AUTHOR_LEAF),
        &[ENABLE_LIVE_AUTHOR_OPTION, CONTENT_ROOT_OPTION],
        "the two the leaf cannot do without"
    );
}

#[test]
fn an_enabled_verification_still_needs_a_profile_and_an_environment() {
    let mut asked = enabled_with(ROOT);
    asked.selection = Selection { environment: Some(ENVIRONMENT.to_owned()), profile: None };
    assert_eq!(Enablement::read(&asked), Err(LiveRefusal::SelectionAbsent("profile")));
    asked.selection = Selection { environment: None, profile: Some(PROFILE.to_owned()) };
    assert_eq!(Enablement::read(&asked), Err(LiveRefusal::SelectionAbsent("environment")));
}

#[test]
fn the_two_live_options_belong_to_this_leaf_and_to_no_other() {
    for option in [ENABLE_LIVE_AUTHOR_OPTION, CONTENT_ROOT_OPTION] {
        assert_eq!(leaves_taking(option), vec![LIVE_AUTHOR_LEAF.to_owned()], "{option}");
        for leaf in LOCAL_LEAVES.iter().filter(|held| **held != LIVE_AUTHOR_LEAF) {
            let words = [*leaf, option, ROOT];
            let held = parse(&words.iter().map(|word| (*word).to_owned()).collect::<Vec<String>>());
            assert!(held.is_err(), "{leaf} takes {option}, and nothing but the live leaf may");
        }
    }
}

#[test]
fn every_declared_content_root_is_accepted_or_refused_exactly_as_declared() {
    let declared = fixture_rows("content-roots.jsonl");
    assert!(!declared.is_empty());
    for row in declared {
        let named = row["named"].as_str().expect("a root is named");
        let mut asked = enabled_with(ROOT);
        asked.arguments.insert(CONTENT_ROOT_OPTION.to_owned(), named.to_owned());
        let held = Enablement::read(&asked);
        match row["refusal"].as_str() {
            None => {
                let enabled = held.unwrap_or_else(|failure| panic!("{named}: {failure}"));
                assert_eq!(enabled.content_root.as_text(), named);
            }
            Some(expected) => {
                let failure = held.expect_err(&format!("{named} was accepted"));
                assert_eq!(refusal_name(&failure), expected, "{named}: {failure}");
            }
        }
    }
}

#[test]
fn a_root_outside_the_content_tree_is_refused_however_ordinary_it_looks() {
    let mut asked = enabled_with(ROOT);
    asked.arguments.insert(CONTENT_ROOT_OPTION.to_owned(), "/etc/slingshot".to_owned());
    assert_eq!(
        Enablement::read(&asked),
        Err(LiveRefusal::ContentRootElsewhere("/etc/slingshot".to_owned())),
        "authored content lives under {CONTENT_TREE}, and a mount point does not"
    );
}

// ------------------------------------------------------- read-only by the row

#[test]
fn the_twelve_rows_are_exactly_what_an_independent_reading_of_them_says() {
    let catalog = CommandCatalog::published();
    let declared = fixture_rows("registry-rows.jsonl");
    assert_eq!(declared.len(), REGISTRY_ROWS, "the registry holds twelve rows");
    assert_eq!(catalog.descriptors().len(), REGISTRY_ROWS);
    for row in declared {
        let wire_name = row["wire_name"].as_str().expect("a name");
        let descriptor = catalog.find(wire_name).unwrap_or_else(|| panic!("{wire_name} is a row"));
        let access = match descriptor.access {
            AccessClassification::Read => "read",
            AccessClassification::Write => "write",
        };
        let destructive = match descriptor.destructive {
            DestructiveClassification::Destructive => "destructive",
            DestructiveClassification::NonDestructive => "non_destructive",
        };
        assert_eq!(access, row["access"].as_str().expect("an access"), "{wire_name}");
        assert_eq!(
            destructive,
            row["destructive"].as_str().expect("a destructiveness"),
            "{wire_name}"
        );
        assert_eq!(
            descriptor.intrinsic_idempotency.idempotent_hint(),
            row["intrinsically_idempotent"].as_bool().expect("an idempotency"),
            "{wire_name}"
        );
        let admissible = admission_for(descriptor) == Admission::Admissible;
        assert_eq!(admissible, row["admissible"].as_bool().expect("an admission"), "{wire_name}");
    }
}

#[test]
fn nine_rows_are_admissible_and_the_three_writes_are_refused_before_any_dispatch() {
    let catalog = CommandCatalog::published();
    assert_eq!(admissible(&catalog).len(), ADMISSIBLE_ROWS);
    let refused_rows = refused(&catalog);
    assert_eq!(refused_rows.len(), REFUSED_ROWS);
    for descriptor in &refused_rows {
        assert_eq!(descriptor.access, AccessClassification::Write, "{}", descriptor.wire_name);
        let failure = require_admissible(&catalog, &descriptor.wire_name)
            .expect_err("a write is refused here, not at the author");
        assert_eq!(refusal_name(&failure), "NotAdmissible");
        assert!(failure.to_string().contains(&descriptor.wire_name), "the command is named");
    }
    for descriptor in admissible(&catalog) {
        require_admissible(&catalog, &descriptor.wire_name).expect("a read is admissible");
    }
}

#[test]
fn idempotency_is_never_read_as_an_access_decision() {
    let catalog = CommandCatalog::published();
    for descriptor in catalog.descriptors() {
        let before = admission_for(descriptor);
        let mut flipped = descriptor.clone();
        flipped.intrinsic_idempotency = match descriptor.intrinsic_idempotency {
            IntrinsicIdempotencyClassification::IntrinsicallyIdempotent => {
                IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent
            }
            IntrinsicIdempotencyClassification::NotIntrinsicallyIdempotent => {
                IntrinsicIdempotencyClassification::IntrinsicallyIdempotent
            }
        };
        assert_eq!(
            admission_for(&flipped),
            before,
            "{}: whether a retry is safe says nothing about whether a run may happen",
            descriptor.wire_name
        );
    }
}

#[test]
fn the_idempotency_column_does_not_separate_what_this_harness_decides() {
    let catalog = CommandCatalog::published();
    let not_idempotent = |descriptor: &&slingshot_domain::command::catalog::CommandDescriptor| {
        !descriptor.intrinsic_idempotency.idempotent_hint()
    };
    assert!(
        admissible(&catalog).iter().any(not_idempotent),
        "an admissible row that is not intrinsically idempotent"
    );
    assert!(
        refused(&catalog).iter().all(not_idempotent),
        "and every refused row is not intrinsically idempotent either"
    );
}

#[test]
fn a_read_that_could_replace_what_was_visible_would_be_refused_too() {
    let catalog = CommandCatalog::published();
    let mut invented = catalog.find("query_paths").expect("a read").clone();
    invented.destructive = DestructiveClassification::Destructive;
    assert_ne!(
        admission_for(&invented),
        Admission::Admissible,
        "the label read is not the whole answer, and both columns are consulted"
    );
}

#[test]
fn a_command_the_registry_does_not_hold_is_refused_by_name() {
    let catalog = CommandCatalog::published();
    let failure = require_admissible(&catalog, "teleport").expect_err("no such row");
    assert_eq!(failure, LiveRefusal::UnknownCommand("teleport".to_owned()));
}

// -------------------------------------------------------- the same contract

/// Returns what an agreeing agent offers for `wire_name`.
fn agreeing(wire_name: &str) -> OfferedCapability {
    let digest = canonical_contract_digest();
    let mut annotations = BTreeMap::new();
    for role in SchemaRole::both() {
        annotations.insert(role.as_text().to_owned(), digest.clone());
    }
    OfferedCapability {
        canonical_contract_annotations: annotations,
        identity: SelectedCommandContractIdentity::installed(wire_name)
            .expect("this build holds the contract"),
    }
}

#[test]
fn an_agent_holding_this_build_s_contract_agrees_about_every_admissible_command() {
    for descriptor in admissible(&CommandCatalog::published()) {
        require_agreement(&agreeing(&descriptor.wire_name))
            .unwrap_or_else(|failure| panic!("{}: {failure}", descriptor.wire_name));
    }
}

/// One way an offered identity can differ from the installed one.
type Drift = (&'static str, fn(&mut SelectedCommandContractIdentity));

#[test]
fn drift_in_any_one_of_the_five_identity_fields_is_refused_with_that_field_named() {
    let drifts: [Drift; DRIFTED_FIELDS] = [
        ("semantic version", |identity| {
            identity.command_semantic_contract_version.push('1');
        }),
        ("limits digest", |identity| identity.command_contract_limits_digest.push('1')),
        ("argument schema", |identity| identity.argument_schema_digest.push('1')),
        ("result schema", |identity| identity.result_schema_digest.push('1')),
    ];
    for (field, drift) in drifts {
        let mut offered = agreeing("query_paths");
        drift(&mut offered.identity);
        assert_eq!(
            require_agreement(&offered),
            Err(LiveRefusal::IdentityDrift { command: "query_paths".to_owned(), field }),
            "{field} drifted and was not named"
        );
    }
}

#[test]
fn a_capability_naming_a_command_this_build_does_not_hold_is_refused_by_name() {
    let mut offered = agreeing("query_paths");
    offered.identity.command_wire_name = "teleport".to_owned();
    let failure = require_agreement(&offered).expect_err("this build holds no such contract");
    assert_eq!(refusal_name(&failure), "ContractUnavailable");
    assert!(failure.to_string().contains("teleport"), "the command is named");
}

#[test]
fn each_role_annotation_is_authenticated_separately_from_the_identity() {
    for role in SchemaRole::both() {
        let mut absent = agreeing("query_paths");
        absent.canonical_contract_annotations.remove(role.as_text());
        assert_eq!(
            require_agreement(&absent),
            Err(LiveRefusal::CanonicalContractAnnotationAbsent {
                command: "query_paths".to_owned(),
                role: role.as_text(),
            })
        );

        let mut drifted = agreeing("query_paths");
        drifted
            .canonical_contract_annotations
            .insert(role.as_text().to_owned(), "0".repeat(canonical_contract_digest().len()));
        let failure = require_agreement(&drifted).expect_err("the annotation drifted");
        assert_eq!(refusal_name(&failure), "CanonicalContractDrift");
        assert!(failure.to_string().contains(role.as_text()), "the role is named");
    }
}

#[test]
fn an_annotation_that_drifts_is_caught_even_when_every_digest_still_matches() {
    let mut offered = agreeing("load_content_as_json");
    let role = SchemaRole::both()[0];
    offered
        .canonical_contract_annotations
        .insert(role.as_text().to_owned(), "1".repeat(canonical_contract_digest().len()));
    assert!(
        require_agreement(&offered).is_err(),
        "a digest match under another byte contract is a coincidence, not agreement"
    );
}

// ------------------------------------------------------ what a run exercises

#[test]
fn every_command_a_verification_submits_is_one_it_is_allowed_to_run() {
    let catalog = CommandCatalog::published();
    assert!(!SUBMITTED_COMMANDS.is_empty());
    for command in SUBMITTED_COMMANDS {
        require_admissible(&catalog, command).unwrap_or_else(|failure| panic!("{failure}"));
    }
}

#[test]
fn every_exercise_is_named_once_and_the_two_lists_are_disjoint() {
    let mut every: Vec<&str> = EXERCISES.iter().chain(OPTIONAL_EXERCISES).copied().collect();
    let declared = every.len();
    every.sort_unstable();
    every.dedup();
    assert_eq!(every.len(), declared, "an exercise is named twice");
    for optional in OPTIONAL_EXERCISES {
        assert!(!EXERCISES.contains(optional), "{optional} is both required and optional");
    }
}

#[test]
fn an_exercise_reads_under_the_root_it_was_given_and_nowhere_else() {
    let enabled = enablement();
    for command in SUBMITTED_COMMANDS {
        let asked = exercise_invocation(command, &enabled, KEY);
        assert_eq!(asked.verb, *command);
        assert_eq!(asked.arguments.get(PATH_OPTION).map(String::as_str), Some(ROOT));
        assert_eq!(asked.selection.profile.as_deref(), Some(PROFILE));
        assert_eq!(asked.selection.environment.as_deref(), Some(ENVIRONMENT));
        assert_eq!(asked.output, Some(OutputForm::Machine));
        assert!(!asked.detached, "a verification waits for the answer it came for");
        assert_eq!(
            asked.operation_key.is_some(),
            requires_operation_key(command),
            "{command}: the registry decides whether a key is needed"
        );
        assert_eq!(
            service_for(&asked).expect("it routes"),
            Service::OperationSubmission,
            "{command}: a verification reaches the author the way anything else does"
        );
    }
}

#[test]
fn the_one_page_query_carries_the_phrase_and_the_others_carry_none() {
    let enabled = enablement();
    let phrase_bearing: Vec<&&str> = SUBMITTED_COMMANDS
        .iter()
        .filter(|command| {
            exercise_invocation(command, &enabled, KEY).arguments.contains_key(PHRASE_OPTION)
        })
        .collect();
    assert_eq!(phrase_bearing.len(), 1, "one page query, and one phrase");
    let asked = exercise_invocation(phrase_bearing[0], &enabled, KEY);
    assert_eq!(asked.arguments.get(PHRASE_OPTION).map(String::as_str), Some(VERIFICATION_PHRASE));
}

// ------------------------------------------------------------- what is claimed

/// Returns a conformance trace that attests everything.
fn conforming() -> ConfigurationConformance {
    ConfigurationConformance {
        bounded_without_partial_handling: true,
        complete_keys_only_enumerations: 1,
        escaped_listing_only_lookup: true,
        hostile_carriers_refused: true,
        metatype_and_redaction_before_value: true,
        persistent_identifier_postchecked: true,
        property_acquisitions: 1,
        reads_of_rejected_values: 0,
        reads_of_each_visible_value: 1,
    }
}

#[test]
fn a_complete_conformance_trace_is_attested() {
    conforming().require_attested().expect("everything is attested");
}

#[test]
fn every_claim_withheld_is_a_claim_the_trace_does_not_make() {
    let withholdings: [fn(&mut ConfigurationConformance); CONFORMANCE_CLAIMS] = [
        |held| held.escaped_listing_only_lookup = false,
        |held| held.persistent_identifier_postchecked = false,
        |held| held.property_acquisitions = 2,
        |held| held.complete_keys_only_enumerations = 2,
        |held| held.bounded_without_partial_handling = false,
        |held| held.hostile_carriers_refused = false,
        |held| held.metatype_and_redaction_before_value = false,
        |held| held.reads_of_rejected_values = 1,
        |held| held.reads_of_each_visible_value = 2,
    ];
    let mut named = Vec::new();
    for withhold in withholdings {
        let mut held = conforming();
        withhold(&mut held);
        let failure = held.require_attested().expect_err("one claim is missing");
        assert_eq!(refusal_name(&failure), "ConformanceNotAttested");
        named.push(failure.to_string());
    }
    let declared = named.len();
    named.sort();
    named.dedup();
    assert_eq!(named.len(), declared, "two withheld claims produced one diagnostic");
}

#[test]
fn acquiring_or_enumerating_too_few_times_is_refused_as_well_as_too_many() {
    let mut held = conforming();
    held.property_acquisitions = 0;
    assert!(held.require_attested().is_err(), "exactly one is not at most one");
    let mut held = conforming();
    held.complete_keys_only_enumerations = 0;
    assert!(held.require_attested().is_err());
    let mut held = conforming();
    held.reads_of_each_visible_value = 0;
    assert!(held.require_attested().is_err(), "a value nobody read is a value nobody saw");
}

// --------------------------------------------------------------- what is said

/// Returns facts one selection resolved to.
fn facts(deployment: AdobeExperienceManagerDeployment) -> ResolvedFacts {
    ResolvedFacts {
        author_target: "https://author.example/".to_owned(),
        deployment,
        environment: ENVIRONMENT.to_owned(),
        profile: PROFILE.to_owned(),
        warned_cleartext_transport: false,
    }
}

#[test]
fn a_report_says_the_nine_things_it_is_for_and_nothing_else() {
    let held = live_report(
        "query_paths",
        KEY,
        &facts(AdobeExperienceManagerDeployment::AdobeExperienceManager65),
        SUCCESS,
    );
    let rendered = held.rendered();
    for named in [
        "agent-event-store-generation",
        "author-only",
        "command",
        "deployment",
        "duration",
        "evidence",
        "operation",
        "result",
        "target",
    ] {
        assert!(rendered.contains(&format!("{named}: ")), "{named} is missing from the report");
    }
    assert_eq!(rendered.lines().count(), REPORT_FIELDS, "these fields, and no tenth");
    assert!(rendered.contains("evidence: live-observation"));
    assert!(rendered.contains("result: succeeded"));
}

#[test]
fn both_deployments_are_reported_as_themselves() {
    let basic = live_report(
        "query_paths",
        KEY,
        &facts(AdobeExperienceManagerDeployment::AdobeExperienceManager65),
        SUCCESS,
    );
    let cloud = live_report(
        "query_paths",
        KEY,
        &facts(AdobeExperienceManagerDeployment::AdobeExperienceManagerCloudService),
        SUCCESS,
    );
    assert_ne!(basic.deployment, cloud.deployment);
    assert_ne!(basic.rendered(), cloud.rendered(), "one deployment is not the other");
}

#[test]
fn an_exit_becomes_the_classification_it_actually_is() {
    let expected = [
        (SUCCESS, ResultClassification::Succeeded),
        (UNAVAILABLE, ResultClassification::Unavailable),
        (USAGE, ResultClassification::Refused),
        (LOCAL_FAILURE, ResultClassification::Refused),
    ];
    for (exit, classification) in expected {
        let held = live_report(
            "query_paths",
            KEY,
            &facts(AdobeExperienceManagerDeployment::AdobeExperienceManager65),
            exit,
        );
        assert_eq!(held.result, classification, "exit {exit}");
    }
}

#[test]
fn one_live_run_says_nothing_about_another_target() {
    let held = live_report(
        "query_paths",
        KEY,
        &facts(AdobeExperienceManagerDeployment::AdobeExperienceManager65),
        SUCCESS,
    );
    assert!(held.covers("https://author.example/"));
    assert!(
        !held.covers("https://author.example:4503/"),
        "the next patch level of the same product is a different target"
    );
    assert!(!held.covers("https://other.example/"));
}

#[test]
fn hermetic_conformance_is_never_evidence_about_a_live_target() {
    let mut held = live_report(
        "query_paths",
        KEY,
        &facts(AdobeExperienceManagerDeployment::AdobeExperienceManager65),
        SUCCESS,
    );
    held.evidence = Evidence::HermeticConformance;
    assert!(
        !held.covers("https://author.example/"),
        "the release gate proves the contract, not somebody's installation"
    );
    assert_eq!(held.duration, DurationCategory::Immediate);
}

#[test]
fn a_report_carries_no_credential_and_no_sentinel() {
    let held = live_report(
        "load_content_as_json",
        KEY,
        &facts(AdobeExperienceManagerDeployment::AdobeExperienceManagerCloudService),
        SUCCESS,
    );
    let rendered = held.rendered().to_lowercase();
    for forbidden in ["password", "secret", "token", "bearer", "authorization", "sentinel", "fake"]
    {
        assert!(!rendered.contains(forbidden), "{forbidden} appears in a report");
    }
}

// ------------------------------------------------------------ one branch only

#[test]
fn the_two_words_an_operator_types_reach_exactly_one_branch() {
    let words: Vec<String> =
        ["verify", "live-author"].iter().map(|word| (*word).to_owned()).collect();
    assert_eq!(normalized(&words), vec![LIVE_AUTHOR_LEAF.to_owned()]);
    let routed = service_for(&enabled_with(ROOT)).expect("it routes");
    assert_eq!(routed, Service::LiveAuthorVerification);
    let claimed: Vec<&&str> = LOCAL_LEAVES
        .iter()
        .filter(|leaf| {
            service_for(&bare(leaf)).is_ok_and(|service| service == Service::LiveAuthorVerification)
        })
        .collect();
    assert_eq!(claimed, vec![&LIVE_AUTHOR_LEAF], "one leaf reaches this service and one only");
}

#[test]
fn the_live_service_reaches_no_versioned_daemon_of_its_own() {
    assert!(
        !Service::LiveAuthorVerification.is_versioned(),
        "the branch itself talks to nothing; each command it runs is versioned on its own"
    );
}
