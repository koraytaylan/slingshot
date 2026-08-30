//! The committed schemas and the byte contract beside them.
//!
//! Two contracts are checked here and they are deliberately separate. A JSON
//! Schema validator answers questions about a decoded tree; it cannot see a
//! byte-order mark, a member order, a nonminimal integer, or the lexical order
//! of a set-like array. Those are the byte contract's, and this test never
//! reports a schema pass as evidence of any of them.
//!
//! Regeneration is compared rather than performed: a difference is a
//! compatibility change for somebody to look at, not a file to rewrite.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use slingshot_domain::command::canonical_json::{
    ArrayOrderInventory, CANONICAL_JSON_FORMAT, CanonicalFailure, DECLARED_COMPARATORS,
    PRESERVE_COMPARATOR, canonical_digest, require_array_order, require_canonical_bytes,
    write_canonical,
};
use slingshot_domain::command::command_identity::{CommandContract, INITIAL_COMMAND_VERSION};
use slingshot_domain::command::schema::{
    CANONICAL_CONTRACT_ANNOTATION, COMMAND_WIRE_NAMES, SCHEMA_DIALECT, SCHEMA_MANIFEST_FORMAT,
    SchemaRole, canonical_contract_digest, command_schema, schema_file_name, schema_identifier,
    schema_manifest,
};

/// The committed byte contract.
const CONTRACT: &str = include_str!("../../../schemas/command-canonical-json-1.json");

/// Independently authored byte and order vectors.
const BYTE_VECTORS: &str = include_str!("../../../schemas/command-canonical-json-vectors.json");

/// Independently authored digest vectors.
const DIGEST_VECTORS: &str = include_str!("../../../schemas/command-schema-digest-vectors.json");

/// Returns the committed bytes of one schema artifact.
fn committed(name: &str) -> String {
    std::fs::read_to_string(format!("{}/../../schemas/commands/{name}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|failure| panic!("{name} is committed: {failure}"))
}

#[test]
fn every_command_has_one_committed_schema_in_each_role() {
    assert_eq!(COMMAND_WIRE_NAMES.len(), 12, "twelve commands, and no thirteenth");
    let mut seen = std::collections::BTreeSet::new();
    for wire_name in COMMAND_WIRE_NAMES {
        assert!(seen.insert(*wire_name), "{wire_name} is named twice");
        for role in SchemaRole::both() {
            let name = schema_file_name(wire_name, role);
            let written =
                write_canonical(&command_schema(wire_name, role)).expect("a schema is canonical");
            assert_eq!(committed(&name), written, "{name} differs from what the producer writes");
        }
    }
    let mut names: Vec<&str> = COMMAND_WIRE_NAMES.to_vec();
    names.sort_unstable();
    assert_eq!(names, COMMAND_WIRE_NAMES, "the catalog order is the ascending one");
}

#[test]
fn regeneration_is_byte_stable_and_never_rewrites_a_committed_file() {
    for wire_name in COMMAND_WIRE_NAMES {
        for role in SchemaRole::both() {
            let once = write_canonical(&command_schema(wire_name, role)).expect("canonical");
            let again = write_canonical(&command_schema(wire_name, role)).expect("canonical");
            assert_eq!(once, again, "{wire_name} is not byte stable across runs");
        }
    }
    let manifest = write_canonical(&schema_manifest()).expect("canonical");
    assert_eq!(committed("command-schema-1.json"), manifest);
    assert_eq!(
        write_canonical(&schema_manifest()).expect("canonical"),
        manifest,
        "the manifest is byte stable too"
    );
}

#[test]
fn every_root_declares_the_dialect_the_version_and_the_byte_contract() {
    for wire_name in COMMAND_WIRE_NAMES {
        for role in SchemaRole::both() {
            let schema = command_schema(wire_name, role);
            assert_eq!(schema["$schema"], Value::from(SCHEMA_DIALECT), "{wire_name}");
            let identifier = schema["$id"].as_str().expect("an identifier");
            assert_eq!(identifier, schema_identifier(wire_name, role));
            assert!(
                identifier.ends_with(INITIAL_COMMAND_VERSION),
                "the version is the final segment: {identifier}"
            );
            assert!(
                !identifier.chars().any(|character| character.is_whitespace()
                    || character.is_control()
                    || "/?#".contains(character)),
                "the identifier needs no second escaping convention: {identifier}"
            );
            assert_eq!(
                schema[CANONICAL_CONTRACT_ANNOTATION],
                Value::from(canonical_contract_digest()),
                "{wire_name}: the byte contract is bound into this role's digest"
            );
        }
    }
}

#[test]
fn a_version_change_would_change_both_role_digests() {
    for wire_name in COMMAND_WIRE_NAMES {
        for role in SchemaRole::both() {
            let mut altered = command_schema(wire_name, role);
            altered["$id"] =
                Value::from(format!("{}-changed", altered["$id"].as_str().expect("an identifier")));
            let original =
                canonical_digest(&write_canonical(&command_schema(wire_name, role)).expect("ok"));
            let changed = canonical_digest(&write_canonical(&altered).expect("ok"));
            assert_ne!(original, changed, "{wire_name}: the identifier is inside the digest");
        }
    }
}

#[test]
fn the_manifest_records_every_digest_it_depends_on() {
    let manifest = schema_manifest();
    assert_eq!(manifest["format"], Value::from(SCHEMA_MANIFEST_FORMAT));
    assert_eq!(manifest["command_semantic_contract_version"], Value::from(INITIAL_COMMAND_VERSION));
    assert_eq!(
        manifest["canonical_json_contract_sha256"],
        Value::from(canonical_contract_digest())
    );
    let limits = canonical_digest(
        &write_canonical(
            &serde_json::from_str::<Value>(CommandContract::embedded_manifest()).expect("a value"),
        )
        .expect("canonical"),
    );
    assert_eq!(manifest["command_contract_limits_sha256"], Value::from(limits));
    for wire_name in COMMAND_WIRE_NAMES {
        for role in SchemaRole::both() {
            let recorded = manifest["schemas"][wire_name][role.as_text()]
                .as_str()
                .unwrap_or_else(|| panic!("{wire_name} has a {} digest", role.as_text()));
            let computed =
                canonical_digest(&write_canonical(&command_schema(wire_name, role)).expect("ok"));
            assert_eq!(recorded, computed, "{wire_name}");
            assert_eq!(recorded.len(), 64, "a digest is sixty-four characters");
            assert!(
                recorded
                    .chars()
                    .all(|character| character.is_ascii_hexdigit()
                        && !character.is_ascii_uppercase()),
                "and lowercase hexadecimal"
            );
        }
    }
}

#[test]
fn the_two_role_digests_of_one_command_are_never_the_same() {
    for wire_name in COMMAND_WIRE_NAMES {
        let arguments = canonical_digest(
            &write_canonical(&command_schema(wire_name, SchemaRole::Arguments)).expect("ok"),
        );
        let result = canonical_digest(
            &write_canonical(&command_schema(wire_name, SchemaRole::Result)).expect("ok"),
        );
        assert_ne!(arguments, result, "{wire_name}: a role swap would go unnoticed");
    }
}

#[test]
fn the_byte_contract_is_itself_canonical_and_closed() {
    let value = require_canonical_bytes(CONTRACT.as_bytes())
        .expect("the committed contract is canonical bytes");
    assert_eq!(value["format"], Value::from(CANONICAL_JSON_FORMAT));
    let comparators = value["comparators"].as_object().expect("a comparator inventory");
    let declared: Vec<&str> = comparators.keys().map(String::as_str).collect();
    let mut expected: Vec<&str> = DECLARED_COMPARATORS.to_vec();
    expected.sort_unstable();
    assert_eq!(declared, expected, "the file and the module declare the same comparators");
    let arrays = value["arrays"].as_object().expect("an array inventory");
    for wire_name in COMMAND_WIRE_NAMES {
        let command =
            arrays.get(*wire_name).unwrap_or_else(|| panic!("{wire_name} has an array inventory"));
        for role in SchemaRole::both() {
            let pointers = command[role.as_text()].as_object().expect("a pointer map");
            for (pointer, comparator) in pointers {
                let comparator = comparator.as_str().expect("a comparator name");
                assert!(
                    DECLARED_COMPARATORS.contains(&comparator),
                    "{wire_name}{pointer}: {comparator} is not declared"
                );
            }
        }
    }
}

#[test]
fn an_unknown_comparator_is_refused_rather_than_read_as_preserve() {
    let mut pointers = BTreeMap::new();
    pointers.insert("/matches".to_owned(), "descending".to_owned());
    assert_eq!(
        ArrayOrderInventory::new(pointers),
        Err(CanonicalFailure::ComparatorUnknown { pointer: "/matches".to_owned() }),
        "silently preserving an unrecognized comparator would leave a set unchecked"
    );
    let inventory = ArrayOrderInventory::new(BTreeMap::new()).expect("an empty inventory");
    assert_eq!(
        inventory.comparator("/anything"),
        PRESERVE_COMPARATOR,
        "an absent pointer preserves, because sequences are the common case"
    );
}

#[test]
fn the_byte_contract_refuses_what_a_schema_cannot_see() {
    /// One canonical document to mutate one fault at a time.
    const CANONICAL: &str = r#"{"a":1,"b":"x"}"#;

    assert!(require_canonical_bytes(CANONICAL.as_bytes()).is_ok(), "the unmutated document");
    let faults: &[(&str, &[u8], CanonicalFailure)] = &[
        ("a byte-order mark", b"\xef\xbb\xbf{\"a\":1}", CanonicalFailure::ByteOrderMark),
        ("leading whitespace", b" {\"a\":1}", CanonicalFailure::InsignificantWhitespace),
        ("trailing whitespace", b"{\"a\":1}\n", CanonicalFailure::InsignificantWhitespace),
        ("two values", b"{\"a\":1}{\"b\":2}", CanonicalFailure::NotOneValue),
        ("members out of order", b"{\"b\":2,\"a\":1}", CanonicalFailure::MembersNotAscending),
        ("a member twice", b"{\"a\":1,\"a\":2}", CanonicalFailure::MemberRepeated),
        ("invalid UTF-8", b"{\"a\":\"\xff\"}", CanonicalFailure::NotUnicode),
    ];
    for (note, bytes, expected) in faults {
        assert_eq!(
            require_canonical_bytes(bytes),
            Err(expected.clone()),
            "{note}: a schema validator would never have seen this"
        );
    }
    for (note, bytes) in [
        ("whitespace inside", br#"{"a": 1}"#.as_slice()),
        ("a leading zero", br#"{"a":01}"#.as_slice()),
        ("an escaped scalar that needs no escape", br#"{"a":"\u0041"}"#.as_slice()),
        ("an uppercase control escape", br#"{"a":"\u000A"}"#.as_slice()),
        ("a control written some other way", br#"{"a":"\n"}"#.as_slice()),
    ] {
        assert!(require_canonical_bytes(bytes).is_err(), "{note}");
    }
}

#[test]
fn a_set_like_array_is_checked_and_a_sequence_is_left_alone() {
    let mut pointers = BTreeMap::new();
    pointers.insert("/tags".to_owned(), "utf8_ascending_unique".to_owned());
    pointers.insert("/matches".to_owned(), "repository_path_utf8_ascending_unique".to_owned());
    let inventory = ArrayOrderInventory::new(pointers).expect("a legal inventory");

    let ordered = json!({"filters": ["b", "a"], "tags": ["a", "b"]});
    assert_eq!(
        require_array_order(&ordered, &inventory),
        Ok(()),
        "the sequence keeps its order and the set is in order"
    );
    for (note, document) in [
        ("a set out of order", json!({"tags": ["b", "a"]})),
        ("a set repeating a value", json!({"tags": ["a", "a"]})),
    ] {
        assert_eq!(
            require_array_order(&document, &inventory),
            Err(CanonicalFailure::ArrayNotOrdered { pointer: "/tags".to_owned() }),
            "{note}"
        );
    }
    let matches = json!({"matches": [
        {"repository_path": "/content/b", "title": "A"},
        {"repository_path": "/content/a", "title": "B"},
    ]});
    assert_eq!(
        require_array_order(&matches, &inventory),
        Err(CanonicalFailure::ArrayNotOrdered { pointer: "/matches".to_owned() }),
        "objects order by the path they carry, not by the rest of their members"
    );
}

#[test]
fn every_independent_byte_vector_is_judged_at_its_own_layer() {
    let vectors: Value = serde_json::from_str(BYTE_VECTORS).expect("one value");
    let rows = vectors["raw_bytes"].as_array().expect("a byte corpus");
    assert!(rows.len() >= 38, "every fault the byte contract owns, and several it does not");
    let mut layers = std::collections::BTreeMap::new();
    for row in rows {
        let spelling = row["spelling"].as_str().expect("a spelling");
        let note = row["note"].as_str().expect("a note");
        let accepted = row["accepted"].as_bool().expect("a verdict");
        let layer = row["layer"].as_str().expect("a layer");
        *layers.entry(layer.to_owned()).or_insert(0_u32) += 1;
        let outcome = require_canonical_bytes(spelling.as_bytes());
        if accepted {
            assert!(outcome.is_ok(), "{note}: refused as {outcome:?}");
            continue;
        }
        match layer {
            "canonical_bytes" => {
                assert!(outcome.is_err(), "{note}: the byte contract accepted a fault it owns")
            }
            "standard_schema" | "typed_semantics" => assert!(
                outcome.is_ok(),
                "{note}: this fault belongs to a later layer, and reporting it here would \
                 let a byte pass stand in for a schema or a grammar"
            ),
            other => panic!("{note}: {other} is not a layer this test knows"),
        }
    }
    assert!(layers.contains_key("standard_schema"), "the corpus reaches past the byte layer");
    assert!(layers.contains_key("typed_semantics"), "and past the schema layer");
}

#[test]
fn every_independent_order_vector_agrees_with_the_comparator_it_names() {
    let vectors: Value = serde_json::from_str(BYTE_VECTORS).expect("one value");
    let rows = vectors["array_order"].as_array().expect("an order corpus");
    assert!(rows.len() >= 14, "every comparator, in order and out");
    for row in rows {
        let note = row["note"].as_str().expect("a note");
        let mut pointers = BTreeMap::new();
        pointers.insert(
            row["pointer"].as_str().expect("a pointer").to_owned(),
            row["comparator"].as_str().expect("a comparator").to_owned(),
        );
        let inventory = ArrayOrderInventory::new(pointers).expect("a declared comparator");
        let document: Value =
            serde_json::from_str(row["document"].as_str().expect("a document")).expect("a value");
        assert_eq!(
            require_array_order(&document, &inventory).is_ok(),
            row["accepted"].as_bool().expect("a verdict"),
            "{note}"
        );
    }
}

#[test]
fn every_independent_digest_vector_produces_its_own_recorded_digest() {
    let vectors: Value = serde_json::from_str(DIGEST_VECTORS).expect("one value");
    let rows = vectors["vectors"].as_array().expect("a digest corpus");
    assert!(rows.len() >= 11, "both range ends, Unicode, a control, and a one-bit neighbour");
    let mut digests = std::collections::BTreeSet::new();
    for row in rows {
        let note = row["note"].as_str().expect("a note");
        let canonical = row["canonical"].as_str().expect("canonical bytes");
        let recorded = row["sha256"].as_str().expect("a digest");
        let value: Value = serde_json::from_str(canonical).expect("a value");
        assert_eq!(
            write_canonical(&value).expect("canonical"),
            canonical,
            "{note}: the recorded bytes are not what this implementation writes"
        );
        assert_eq!(canonical_digest(canonical), recorded, "{note}");
        assert_eq!(recorded.len(), 64, "{note}: sixty-four characters");
        digests.insert(recorded.to_owned());
    }
    assert!(
        digests.len() >= rows.len() - 1,
        "two vectors differing by one bit do not share a digest"
    );
}
