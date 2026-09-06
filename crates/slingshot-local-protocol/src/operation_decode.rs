//! Bounded operation decoding before any target, repository, or executor access.

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

use crate::foundation_contract::FoundationContract;
use crate::message::OperationEnvelope;

/// A public-safe refusal; payload members and parser excerpts never escape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the operation envelope is malformed or exceeds its declared bounds")]
pub struct InvalidOperationEnvelope;

/// Reads the version without interpreting another version's request vocabulary.
///
/// # Errors
///
/// Rejects malformed, over-bound, or duplicate-member JSON before returning a
/// version. A valid future request need not match today's request enum.
pub fn protocol_version(
    contract: &FoundationContract,
    payload: &[u8],
) -> Result<u32, InvalidOperationEnvelope> {
    let text = crate::framing::read_payload(&contract.framing, payload)
        .map_err(|_| InvalidOperationEnvelope)?;
    serde_json::from_str::<UniqueMembers>(text).map_err(|_| InvalidOperationEnvelope)?;
    #[derive(serde::Deserialize)]
    struct Version {
        operation_protocol_version: u32,
    }
    serde_json::from_str::<Version>(text)
        .map(|header| header.operation_protocol_version)
        .map_err(|_| InvalidOperationEnvelope)
}

/// Reads an operation envelope without silently replacing duplicate members.
///
/// Structural bounds are checked before recursive deserialization. The first
/// pass checks every object, including untyped command arguments, before the
/// second pass constructs the closed protocol shape.
///
/// # Errors
///
/// Returns a static refusal for invalid JSON, duplicates, invalid envelope
/// fields, noncanonical digests, or an exceeded foundation bound.
pub fn decode(
    contract: &FoundationContract,
    payload: &[u8],
) -> Result<OperationEnvelope, InvalidOperationEnvelope> {
    let text = crate::framing::read_payload(&contract.framing, payload)
        .map_err(|_| InvalidOperationEnvelope)?;
    serde_json::from_str::<UniqueMembers>(text).map_err(|_| InvalidOperationEnvelope)?;
    let envelope: OperationEnvelope =
        serde_json::from_str(text).map_err(|_| InvalidOperationEnvelope)?;
    envelope.require_well_formed().map_err(|_| InvalidOperationEnvelope)?;
    if envelope.request_identifier.len() > contract.names.request_identifier_bytes as usize {
        return Err(InvalidOperationEnvelope);
    }
    Ok(envelope)
}

/// Discards scalar values while retaining each open object's decoded keys.
struct UniqueMembers;

impl<'de> Deserialize<'de> for UniqueMembers {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueMembersVisitor)
    }
}

struct UniqueMembersVisitor;

impl<'de> Visitor<'de> for UniqueMembersVisitor {
    type Value = UniqueMembers;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value with unique object members")
    }

    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(UniqueMembers)
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(UniqueMembers)
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(UniqueMembers)
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(UniqueMembers)
    }
    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(UniqueMembers)
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(UniqueMembers)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        while sequence.next_element::<UniqueMembers>()?.is_some() {}
        Ok(UniqueMembers)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut object: A) -> Result<Self::Value, A::Error> {
        let mut keys = std::collections::BTreeSet::new();
        while let Some(key) = object.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(serde::de::Error::custom("duplicate object member"));
            }
            object.next_value::<UniqueMembers>()?;
        }
        Ok(UniqueMembers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST_HEX_CHARACTERS: usize = 64;

    fn envelope(command: &str) -> String {
        format!(
            r#"{{"author_target_identity_digest":"{digest}","daemon_runtime_contract_digest":"{digest}","operation_protocol_version":1,"request":{{"request":"execute","command":{command},"operation_identifier":"operation","workflow_correlation_identifier":null}},"request_identifier":"request","selected_environment_revision":"{digest}"}}"#,
            digest = "a".repeat(DIGEST_HEX_CHARACTERS),
        )
    }

    #[test]
    fn accepts_all_scalar_shapes_and_arbitrary_member_order() {
        let payload = envelope(r#"{"z":[null,true,false,-1,2,1.5,"text"],"a":{}}"#);
        assert!(decode(&FoundationContract::embedded(), payload.as_bytes()).is_ok());
    }

    #[test]
    fn rejects_duplicates_even_inside_untyped_commands_and_arrays() {
        for command in [r#"{"a":1,"a":2}"#, r#"[{"a":1,"\u0061":2}]"#] {
            assert_eq!(
                decode(&FoundationContract::embedded(), envelope(command).as_bytes()),
                Err(InvalidOperationEnvelope)
            );
        }
        let payload = envelope("{}").replace(
            "\"operation_protocol_version\":1",
            "\"operation_protocol_version\":1,\"operation_protocol_version\":2",
        );
        assert!(decode(&FoundationContract::embedded(), payload.as_bytes()).is_err());
    }

    #[test]
    fn refuses_invalid_shapes_and_bounds_without_echoing_input() {
        let mut contract = FoundationContract::embedded();
        for payload in [
            envelope("{}")
                .replace("\"request_identifier\":\"request\"", "\"request_identifier\":\"\""),
            envelope("{}") + "null",
            envelope("{\"secret\":}"),
        ] {
            let error = decode(&contract, payload.as_bytes()).unwrap_err();
            assert!(!error.to_string().contains("secret"));
        }
        contract.framing.maximum_payload_bytes = 1;
        assert!(decode(&contract, envelope("{}").as_bytes()).is_err());
        contract = FoundationContract::embedded();
        contract.framing.maximum_nesting_depth = 2;
        assert!(decode(&contract, envelope("[{}]").as_bytes()).is_err());
    }

    #[test]
    fn request_identifier_accepts_its_exact_byte_bound_only() {
        let contract = FoundationContract::embedded();
        for (length, accepted) in [
            (contract.names.request_identifier_bytes as usize, true),
            (contract.names.request_identifier_bytes as usize + 1, false),
        ] {
            let payload = envelope("{}").replace(
                "\"request_identifier\":\"request\"",
                &format!("\"request_identifier\":\"{}\"", "r".repeat(length)),
            );
            assert_eq!(decode(&contract, payload.as_bytes()).is_ok(), accepted);
        }
    }
}
