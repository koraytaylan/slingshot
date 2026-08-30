//! Assertions for repository addresses.
//!
//! One address must have one spelling. Two spellings of the same node would
//! give one thing two identities everywhere an identity is derived from a path,
//! and a spelling that resolves elsewhere would let a caller reach content the
//! command was never pointed at. Every refused vector is one of those two
//! problems written down.
//!
//! The roles are separate types, so the vectors also prove that a property name
//! is not a path segment, a created child is not a namespaced name, and a
//! resource type is not a repository path - each refused where it does not
//! belong rather than quietly accepted.

use std::path::PathBuf;

use serde::Deserialize;
use slingshot_domain::command::component_resource_type::ComponentResourceType;
use slingshot_domain::command::repository_path::{
    ComponentName, PageName, PrimaryNodeTypeName, PropertyName, RelativePropertyPath,
    RepositoryName, RepositoryPath, RepositoryPathSegment, RepositoryPropertyPath,
    RepositoryRelativePath,
};

/// Fixture holding every accepted and refused spelling.
const VECTOR_FIXTURE: &str = "tests/fixtures/commands/repository-path.jsonl";

/// One spelling and what the grammar does with it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    /// Whether the role accepts the spelling.
    accepted: bool,
    /// Why the vector exists.
    note: String,
    /// Role the spelling is offered to.
    role: String,
    /// The spelling itself.
    spelling: String,
}

/// Returns every committed vector.
fn vectors() -> Vec<Vector> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(VECTOR_FIXTURE);
    let text = std::fs::read_to_string(&path).expect("the vectors read");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("the vector parses"))
        .collect()
}

/// Reports whether one role accepts one spelling, and round-trips it if so.
fn accepts(role: &str, spelling: &str) -> bool {
    match role {
        "repository_name" => round_trips(RepositoryName::parse(spelling), spelling),
        "repository_path_segment" => round_trips(RepositoryPathSegment::parse(spelling), spelling),
        "repository_path" => round_trips(RepositoryPath::parse(spelling), spelling),
        "repository_relative_path" => {
            round_trips(RepositoryRelativePath::parse(spelling), spelling)
        }
        "property_name" => round_trips(PropertyName::parse(spelling), spelling),
        "relative_property_path" => round_trips(RelativePropertyPath::parse(spelling), spelling),
        "page_name" => round_trips(PageName::parse(spelling), spelling),
        "primary_node_type_name" => round_trips(PrimaryNodeTypeName::parse(spelling), spelling),
        "component_resource_type" => round_trips(ComponentResourceType::parse(spelling), spelling),
        other => panic!("the fixture names the unknown role {other}"),
    }
}

/// Reports whether one accepted value renders back exactly as it was given.
fn round_trips<Value: ::core::fmt::Display, Failure>(
    parsed: Result<Value, Failure>,
    spelling: &str,
) -> bool {
    parsed.is_ok_and(|value| value.to_string() == spelling)
}

#[test]
fn every_committed_spelling_is_accepted_or_refused_as_the_vector_says() {
    let mut disagreements = Vec::new();
    for vector in vectors() {
        if accepts(&vector.role, &vector.spelling) != vector.accepted {
            disagreements.push(format!(
                "{} {:?} should be {}: {}",
                vector.role,
                vector.spelling,
                if vector.accepted { "accepted" } else { "refused" },
                vector.note
            ));
        }
    }
    assert_eq!(disagreements, Vec::<String>::new());
}

#[test]
fn every_accepted_address_round_trips_through_canonical_json() {
    for spelling in ["/", "/content", "/content/rail[2]", "/content/jcr:content"] {
        let path = RepositoryPath::parse(spelling).expect("the path is valid");
        let rendered = serde_json::to_string(&path).expect("the path renders");
        assert_eq!(rendered, format!("{spelling:?}"), "the path is not one JSON string");
        let read: RepositoryPath = serde_json::from_str(&rendered).expect("the path reads back");
        assert_eq!(read, path);
    }
    for spelling in ["/content/jcr:title", "jcr:content/jcr:title"] {
        let property = RepositoryPropertyPath::parse(spelling).expect("the address is valid");
        let rendered = serde_json::to_string(&property).expect("the address renders");
        assert_eq!(rendered, format!("{spelling:?}"));
        let read: RepositoryPropertyPath =
            serde_json::from_str(&rendered).expect("the address reads back");
        assert_eq!(read, property);
    }
    assert!(
        serde_json::from_str::<RepositoryPath>("\"/content/\"").is_err(),
        "a refused spelling read back as a path"
    );
}

#[test]
fn walking_up_and_down_stays_inside_the_grammar() {
    let root = RepositoryPath::parse("/").expect("the root is valid");
    assert!(root.is_root());
    assert_eq!(root.parent(), None, "the root has no parent");

    let name = RepositoryName::parse("content").expect("the name is valid");
    let content = root.creatable_child(&name).expect("the child is addressable");
    assert_eq!(content.as_text(), "/content");
    assert_eq!(content.parent(), Some(root.clone()), "the child's parent is the root");

    let sibling = RepositoryPathSegment::parse("rail[2]").expect("the segment is valid");
    let addressed = content.address_child(&sibling).expect("the child is addressable");
    assert_eq!(addressed.as_text(), "/content/rail[2]");
    assert_eq!(addressed.parent(), Some(content.clone()));
    assert_eq!(addressed.segments().len(), 2, "the path names two segments");
    assert_eq!(addressed.segments()[1].name().as_text(), "rail");

    let qualified = RepositoryName::parse("jcr:content").expect("the name is valid");
    assert!(
        PageName::parse(qualified.as_text()).is_err(),
        "a namespaced name was accepted as a creatable child"
    );
    assert!(
        content.creatable_child(&RepositoryName::parse("rail").expect("valid")).is_ok(),
        "a plain name is creatable"
    );
}

#[test]
fn a_role_refuses_a_value_that_belongs_to_another_one() {
    let path = "/content/site";
    assert!(RepositoryPath::parse(path).is_ok());
    assert!(RepositoryRelativePath::parse(path).is_err(), "an absolute path is not relative");
    assert!(PropertyName::parse(path).is_err(), "a path is not a property name");
    assert!(ComponentResourceType::parse("jcr:content").is_err(), "a name is not a resource type");
    assert!(
        RepositoryName::parse("wknd/components/text").is_err(),
        "a resource type is not a name"
    );
    assert!(ComponentName::parse("cq:Page").is_err(), "a node type is not a component name");
    assert!(PrimaryNodeTypeName::parse("landing-page").is_err(), "a page name is not a node type");
}

#[test]
fn every_named_bound_accepts_its_own_length_and_refuses_one_more() {
    use slingshot_domain::command::command_identity::CommandContract;

    let contract = CommandContract::embedded();
    for (bound, role) in [
        ("maximum_repository_name_bytes", "repository_name"),
        ("maximum_page_name_bytes", "page_name"),
        ("maximum_component_resource_type_bytes", "component_resource_type"),
    ] {
        let width = usize::try_from(contract.limit(bound)).expect("the bound fits");
        let exact = "a".repeat(width);
        assert!(accepts(role, &exact), "{role} refused its own bound");
        assert!(!accepts(role, &format!("{exact}a")), "{role} accepted one byte over");
    }
    let exact = path_of_exactly(
        usize::try_from(contract.limit("maximum_repository_path_bytes")).expect("it fits"),
    );
    assert!(accepts("repository_path", &exact), "a path refused its own bound");
    assert!(!accepts("repository_path", &format!("{exact}a")), "a path accepted one byte over");
}

/// Returns an absolute path of exactly `width` bytes.
///
/// The bytes are spread over several segments, because one segment long enough
/// to fill a whole path would break the name bound first and prove nothing
/// about the path bound.
fn path_of_exactly(width: usize) -> String {
    /// Bytes one segment occupies, separator included.
    const SEGMENT_WIDTH: usize = 64;

    let whole = width / SEGMENT_WIDTH;
    let remainder = width % SEGMENT_WIDTH;
    let mut path: String =
        (0..whole).map(|_| format!("/{}", "a".repeat(SEGMENT_WIDTH - 1))).collect();
    if remainder > 0 {
        path.push_str(&format!("/{}", "a".repeat(remainder - 1)));
    }
    assert_eq!(path.len(), width, "the built path is not the width asked for");
    path
}

#[test]
fn a_path_names_no_more_segments_than_the_contract_allows() {
    use slingshot_domain::command::command_identity::CommandContract;

    let maximum =
        usize::try_from(CommandContract::embedded().limit("maximum_repository_path_segments"))
            .expect("the bound fits");
    let exact: String = (0..maximum).map(|_| "/a").collect();
    assert!(RepositoryPath::parse(&exact).is_ok(), "a path refused its own segment bound");
    assert!(
        RepositoryPath::parse(&format!("{exact}/a")).is_err(),
        "a path accepted one segment over"
    );
}
