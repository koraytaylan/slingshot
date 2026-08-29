//! Probe for the unique-identifiers capability.
//!
//! Requires random version-four identifiers, their canonical hyphenated text,
//! parsing back from that text, and serialization support.

use uuid::Uuid;

#[test]
fn a_random_identifier_round_trips_through_its_canonical_text() {
    let identifier = Uuid::new_v4();
    assert_eq!(identifier.get_version_num(), 4);
    let rendered = identifier.hyphenated().to_string();
    assert_eq!(rendered.len(), 36);
    let parsed = Uuid::parse_str(&rendered).expect("the canonical text parses");
    assert_eq!(parsed, identifier);
    assert_ne!(Uuid::new_v4(), identifier, "two draws differ");
    let serialized = serde_json::to_string(&identifier).expect("the identifier serializes");
    assert_eq!(serialized, format!("\"{rendered}\""));
}
