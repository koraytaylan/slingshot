//! Assertions for what one mapping entry is.
//!
//! The status-code rule is proved in both directions, because both mistakes are
//! easy to make and neither is visible afterwards: a redirect with no status
//! leaves the client with nothing to act on, and a map with one suggests a
//! redirect that never happens.

use slingshot_domain::command::command_identity::CommandContract;
use slingshot_domain::command::repository_path::RepositoryPath;
use slingshot_domain::command::resource_mapping_entry::{
    RESOURCE_MAPPING_KIND_COUNT, RequestAddress, ResourceMappingEntry, ResourceMappingFailure,
    ResourceMappingKind, ResourceMappingPattern,
};

/// A status a redirecting entry answers with.
const FOUND: u16 = 302;

/// Returns one limit by name.
fn limit(name: &str) -> usize {
    usize::try_from(CommandContract::embedded().limit(name)).expect("the bound fits")
}

/// Returns one legal path.
fn path(value: &str) -> RepositoryPath {
    RepositoryPath::parse(value).expect("a legal path")
}

/// Returns one legal pattern.
fn pattern(value: &str) -> ResourceMappingPattern {
    ResourceMappingPattern::parse(value).expect("a legal pattern")
}

#[test]
fn a_pattern_is_opaque_text_within_its_bound() {
    for accepted in ["^example\\.test/", "localhost.8080", "(.*)"] {
        assert!(ResourceMappingPattern::parse(accepted).is_ok(), "{accepted} was refused");
    }
    let exact = "a".repeat(limit("maximum_resource_mapping_pattern_bytes"));
    assert!(ResourceMappingPattern::parse(&exact).is_ok(), "the bound itself was refused");
    assert!(ResourceMappingPattern::parse(&format!("{exact}a")).is_err());
    assert!(ResourceMappingPattern::parse("").is_err());
    assert!(ResourceMappingPattern::parse("exa\u{0}mple").is_err());
}

#[test]
fn a_request_address_is_rooted_or_schemed_and_never_relative() {
    for accepted in ["/en/report.html", "https://example.test/en", "http://localhost:4502/"] {
        assert!(RequestAddress::parse(accepted).is_ok(), "{accepted} was refused");
    }
    for refused in ["", "en/report.html", "https://", "/en report.html", "/en\u{0}report"] {
        assert!(RequestAddress::parse(refused).is_err(), "{refused:?} was accepted");
    }
    let exact = format!("/{}", "a".repeat(limit("maximum_request_address_bytes") - "/".len()));
    assert!(RequestAddress::parse(&exact).is_ok(), "the bound itself was refused");
    assert!(RequestAddress::parse(&format!("{exact}a")).is_err());
}

#[test]
fn a_status_code_belongs_to_a_redirect_and_to_no_other_kind() {
    assert!(
        ResourceMappingEntry::new(
            path("/etc/map/https/example.test"),
            ResourceMappingKind::Redirect,
            pattern("^example\\.test/"),
            vec!["https://example.test/".to_owned()],
            Some(FOUND),
        )
        .is_ok()
    );
    assert_eq!(
        ResourceMappingEntry::new(
            path("/etc/map/https/example.test"),
            ResourceMappingKind::Redirect,
            pattern("^example\\.test/"),
            vec!["https://example.test/".to_owned()],
            None,
        ),
        Err(ResourceMappingFailure::StatusCodeMisplaced)
    );
    assert_eq!(
        ResourceMappingEntry::new(
            path("/etc/map/https/example.test"),
            ResourceMappingKind::Map,
            pattern("^example\\.test/"),
            vec!["/content/example/".to_owned()],
            Some(FOUND),
        ),
        Err(ResourceMappingFailure::StatusCodeMisplaced)
    );
}

#[test]
fn a_replacement_list_is_accepted_at_its_bound_and_refused_one_past_it() {
    let bound = limit("maximum_resource_mapping_replacements");
    let build = |count: usize| {
        ResourceMappingEntry::new(
            path("/etc/map/https/example.test"),
            ResourceMappingKind::Map,
            pattern("^example\\.test/"),
            (0..count).map(|index| format!("/content/{index}")).collect(),
            None,
        )
    };
    assert!(build(bound).is_ok(), "the bound itself was refused");
    assert_eq!(build(bound + 1), Err(ResourceMappingFailure::TooManyReplacements));
}

#[test]
fn every_kind_round_trips_and_a_fifth_is_refused() {
    assert_eq!(ResourceMappingKind::every().len(), RESOURCE_MAPPING_KIND_COUNT);
    for kind in ResourceMappingKind::every() {
        let written = serde_json::to_string(&kind).expect("a kind serializes");
        assert_eq!(
            serde_json::from_str::<ResourceMappingKind>(&written).expect("a kind parses"),
            kind
        );
    }
    assert!(serde_json::from_str::<ResourceMappingKind>("\"forward\"").is_err());
    assert!(ResourceMappingKind::Redirect.redirects());
    assert!(!ResourceMappingKind::Map.redirects());
}
