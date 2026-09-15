//! What a tool call's arguments turn into, for every tool the server offers.
//!
//! A catalog that names seventy-two tools is only useful if a call naming one
//! of them can reach something. The failure this file exists to catch is the
//! one where the catalog is advertised and every call is answered with a local
//! failure, because the translation from a call's arguments to the thing that
//! runs them only ever worked for the handful of tools somebody tried by hand.
//!
//! The claim is made per tool rather than in aggregate: every registry command's
//! declared schema members build the command that schema describes, and every
//! control's declared members map to the options its leaf reads. A tool that
//! cannot be reached is named, because "sixty-eight of seventy-two" is not a
//! useful thing for a reader to be told.

use serde_json::{Value, json};

use slingshot_command_line::command_line::tool_invocation;
use slingshot_command_line::invocation::Selection;
use slingshot_command_line::model_context_protocol::tool_catalog::{
    EVERY_CONTROL, KeyPresence, Provenance, ToolDescriptor, derive,
};
use slingshot_domain::command::catalog::{Command, CommandCatalog};

/// Returns every tool this server offers.
fn tools() -> Vec<ToolDescriptor> {
    derive(&Provenance::recomputed()).expect("this build's provenance agrees with itself")
}

/// Returns one minimal value a declared schema admits.
///
/// Drawn from the member's own declaration rather than from its name, so a
/// value this produces is one the schema says is legal: an enum gets its first
/// spelling, an array gets as many items as it requires, and a path gets a
/// spelling matching the pattern its schema states. A member whose value the
/// schema cannot express - a Base64 payload with canonical padding, a
/// nonempty ascending set - is given the smallest thing of that shape, and the
/// command's own constructor is what finally says whether it is usable.
fn value_for(member: &str, schema: &Value) -> Value {
    if let Some(spellings) = schema.get("enum").and_then(Value::as_array) {
        return spellings.first().cloned().unwrap_or_else(|| json!("a-usable-value"));
    }
    if let Some(constant) = schema.get("const") {
        return constant.clone();
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(alternatives) = schema.get(key).and_then(Value::as_array) {
            if let Some(first) = alternatives.first() {
                return value_for(member, first);
            }
        }
    }
    match schema.get("type").and_then(Value::as_str).unwrap_or("string") {
        "boolean" => Value::Bool(false),
        "integer" | "number" => json!(1),
        "array" => {
            let least = schema.get("minItems").and_then(Value::as_u64).unwrap_or(0);
            let least = usize::try_from(least.max(1)).unwrap_or(1);
            let item = schema.get("items").cloned().unwrap_or_else(|| json!({ "type": "string" }));
            Value::Array((0..least).map(|_| value_for(member, &item)).collect())
        }
        "object" => {
            let required =
                schema.get("required").and_then(Value::as_array).cloned().unwrap_or_default();
            let properties =
                schema.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
            let mut built = serde_json::Map::new();
            for held in required.iter().filter_map(Value::as_str) {
                let declared =
                    properties.get(held).cloned().unwrap_or_else(|| json!({ "type": "string" }));
                built.insert(held.to_owned(), value_for(held, &declared));
            }
            Value::Object(built)
        }
        _ => {
            let pattern = schema.get("pattern").and_then(Value::as_str).unwrap_or_default();
            if pattern.starts_with("^/") {
                return json!("/content");
            }
            let spelled = match member {
                "media_type" => "text/plain",
                "encoded_content" | "payload" => "",
                "property_path" => "property",
                "repository_path" | "page_path" | "asset_path" | "source_path"
                | "fragment_path" | "template_path" | "component_path" => "/content",
                "request_address" | "request_authority" => "http://127.0.0.1:8080",
                _ => "a-usable-value",
            };
            json!(spelled)
        }
    }
}

/// Returns the arguments one call for `tool` would send, from its own schema.
///
/// Only the members the tool requires are filled, so what this produces is the
/// smallest legal call. Everything else the schema declares is optional and
/// left out, which is what a caller doing the least would send.
fn minimal_arguments(tool: &ToolDescriptor) -> Value {
    let declared =
        slingshot_command_line::model_context_protocol::schema_projection::input_schema(tool)
            .expect("every tool declares an input schema");
    let required: Vec<&str> = declared
        .get("required")
        .and_then(Value::as_array)
        .map(|members| members.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let properties =
        declared.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut arguments = serde_json::Map::new();
    for member in required {
        let schema = properties.get(member).cloned().unwrap_or_else(|| json!({ "type": "string" }));
        arguments.insert(member.to_owned(), value_for(member, &schema));
    }
    if tool.operation_key == KeyPresence::Required {
        arguments.insert("operation_key".to_owned(), json!("a-caller-key"));
    }
    Value::Object(arguments)
}

#[test]
fn every_command_tool_builds_the_command_its_own_schema_declares() {
    let offered = tools();
    let commands: Vec<&ToolDescriptor> = offered
        .iter()
        .filter(|held| CommandCatalog::published().find(&held.name).is_some())
        .collect();
    assert_eq!(commands.len(), CommandCatalog::published().descriptors().len());
    let mut unreachable: Vec<String> = Vec::new();
    for tool in commands {
        let arguments = minimal_arguments(tool);
        match tool_invocation(tool, &arguments, &Selection::default()) {
            Ok(invocation) => match invocation.carried_command() {
                Some(command) if command.wire_name() == tool.name => {}
                Some(command) => unreachable.push(format!(
                    "{} built {} instead of itself",
                    tool.name,
                    command.wire_name()
                )),
                None => unreachable.push(format!("{} built no command", tool.name)),
            },
            Err(refusal) => unreachable.push(format!("{}: {refusal}", tool.name)),
        }
    }
    assert!(
        unreachable.is_empty(),
        "these tools advertise a schema their own arguments cannot satisfy:\n{}",
        unreachable.join("\n")
    );
}

#[test]
fn a_command_call_carries_the_commands_members_and_not_a_commands_options() {
    // The distinction the whole translation turns on. A command's declared
    // member is `root_path`; the option that fills it from a command line is
    // `--path`. A call spelled with the member must build the command, and a
    // call spelled with the option must be refused rather than quietly
    // understood, because the schema never declared `path`.
    let offered = tools();
    let query = offered.iter().find(|held| held.name == "query_paths").expect("it is offered");
    let built = tool_invocation(query, &json!({ "root_path": "/content" }), &Selection::default())
        .expect("the member its schema declares builds it");
    assert!(
        matches!(built.carried_command(), Some(Command::QueryPaths(_))),
        "the call became the command its schema describes"
    );

    for (member, value) in [("root_path", json!("/content")), ("path", json!("/content"))] {
        let spelled_as_an_option = json!({ member: value });
        let built = tool_invocation(query, &spelled_as_an_option, &Selection::default());
        match member {
            "root_path" => assert!(built.is_ok(), "the declared member is the one that works"),
            _ => assert!(
                built.is_err(),
                "the option vocabulary is not the schema vocabulary, and {member} is not declared"
            ),
        }
    }
}

#[test]
fn every_control_tool_maps_every_declared_member_to_the_option_its_leaf_reads() {
    let offered = tools();
    let controls: Vec<&ToolDescriptor> = offered
        .iter()
        .filter(|held| CommandCatalog::published().find(&held.name).is_none())
        .collect();
    assert_eq!(controls.len(), EVERY_CONTROL.len(), "every control is covered");
    let mut unreachable: Vec<String> = Vec::new();
    for tool in controls {
        let declared =
            slingshot_command_line::model_context_protocol::schema_projection::input_schema(tool)
                .expect("every control declares an input schema");
        let properties =
            declared.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
        // Every member the schema declares has to reach an option, not only the
        // ones it marks required: a member a caller may send and nothing reads
        // is a spelling the server silently drops.
        for member in properties.keys() {
            if member == "operation_key" || member == "detached" {
                continue;
            }
            if slingshot_command_line::model_context_protocol::schema_projection::control_option(
                tool, member,
            )
            .is_err()
            {
                unreachable.push(format!("{}.{member} has no option to fill", tool.name));
            }
        }
        // And a call built from the schema's own required set has to translate,
        // which is what a conforming caller would send.
        let arguments = minimal_arguments(tool);
        match tool_invocation(tool, &arguments, &Selection::default()) {
            Ok(invocation) if invocation.carried_command().is_none() => {}
            Ok(_) => unreachable.push(format!("{} built a command, and is not one", tool.name)),
            Err(refusal) => unreachable.push(format!("{}: {refusal}", tool.name)),
        }
    }
    assert!(
        unreachable.is_empty(),
        "these controls declare something nothing carries:\n{}",
        unreachable.join("\n")
    );
}

#[test]
fn a_control_whose_leaf_insists_on_something_declares_that_something() {
    // The failure this catches is the one the coverage probe found: a control
    // whose leaf refuses the call because a member the schema never declared is
    // missing. A schema no conforming call can satisfy is worse than no schema,
    // because a caller trusts it.
    for (named, member) in [
        ("operation-restart", "expected_operation_revision"),
        ("operation-artifact", "expected_content_digest"),
        ("maintenance-preview", "before_unix_milliseconds"),
        ("maintenance-apply", "reviewed_manifest_digest"),
    ] {
        let offered = tools();
        let tool = offered.iter().find(|held| held.name == named).expect("the control is offered");
        let declared =
            slingshot_command_line::model_context_protocol::schema_projection::input_schema(tool)
                .expect("it declares an input schema");
        let required: Vec<&str> = declared["required"]
            .as_array()
            .expect("the required set is a list")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(
            required.contains(&member),
            "{named} does not require {member}, and the leaf it answers with refuses without it"
        );
    }
}
