//! Everything a run can be observed through, gathered in one place.
//!
//! A secret escapes through whichever channel nobody was watching, so the
//! useful thing is not another careful check of one stream but a list of every
//! stream there is. What a capture holds is deliberately more than any single
//! suite needs: arguments, both streams, files a run wrote, the environment it
//! was given, and whatever it recorded as diagnostics.
//!
//! # Searching is not the same as reading
//!
//! A value can leave in an encoding nobody typed. The search here is over the
//! forms a value actually takes on the way somewhere - as it was written, in
//! either case, hex, percent-escaped, base64, and escaped as it would appear
//! inside a string - because a credential that leaks base64-encoded has leaked.

use std::collections::BTreeMap;

/// Everything one run can be observed through.
#[derive(Debug, Default, Clone)]
pub struct ObservableCapture {
    /// What each named channel held.
    channels: BTreeMap<String, String>,
}

impl ObservableCapture {
    /// Returns a capture holding nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one channel's contents.
    #[must_use]
    pub fn holding(mut self, channel: &str, contents: impl Into<String>) -> Self {
        self.channels.insert(channel.to_owned(), contents.into());
        self
    }

    /// Returns which channels this capture holds.
    #[must_use]
    pub fn channels(&self) -> Vec<&str> {
        self.channels.keys().map(String::as_str).collect()
    }

    /// Returns every channel one value appears in, in every encoding searched.
    #[must_use]
    pub fn exposures(&self, value: &str) -> Vec<Exposure> {
        let mut found = Vec::new();
        for (channel, contents) in &self.channels {
            for (encoding, written) in every_encoding(value) {
                let held = if encoding == UPPERCASE {
                    contents.to_uppercase().contains(&written)
                } else {
                    contents.contains(&written)
                };
                if held {
                    found
                        .push(Exposure { channel: channel.clone(), encoding: encoding.to_owned() });
                }
            }
        }
        found
    }
}

/// Where one value was found, and how it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exposure {
    /// Which channel it was found in.
    pub channel: String,
    /// Which encoding it was written in.
    pub encoding: String,
}

/// The encoding whose comparison is made in one case.
const UPPERCASE: &str = "uppercase";

/// Returns one value in every encoding this capture searches for.
#[must_use]
pub fn every_encoding(value: &str) -> Vec<(&'static str, String)> {
    vec![
        ("raw", value.to_owned()),
        (UPPERCASE, value.to_uppercase()),
        ("hexadecimal", value.bytes().map(|byte| format!("{byte:02x}")).collect()),
        ("percent", percent_of(value)),
        ("base64", base64_of(value)),
        ("json-escaped", json_escaped_of(value)),
    ]
}

/// Returns one value with everything but the unreserved characters escaped.
fn percent_of(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            let held = char::from(byte);
            if held.is_ascii_alphanumeric() || "-._~".contains(held) {
                held.to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

/// The alphabet base64 is written in.
const BASE64_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// How many input bytes one base64 group holds.
const BASE64_GROUP_BYTES: usize = 3;

/// How many characters one base64 group produces.
const BASE64_GROUP_CHARACTERS: usize = 4;

/// How many bits one base64 character carries.
const BASE64_CHARACTER_BITS: u32 = 6;

/// The mask that keeps one base64 character.
const BASE64_CHARACTER_MASK: u32 = 0b0011_1111;

/// How many bits one byte carries.
const BYTE_BITS: u32 = 8;

/// Returns one value in base64, without padding.
fn base64_of(value: &str) -> String {
    let mut written = String::new();
    for group in value.as_bytes().chunks(BASE64_GROUP_BYTES) {
        let mut held = 0_u32;
        for (position, byte) in group.iter().enumerate() {
            held |= u32::from(*byte)
                << (BYTE_BITS * u32::try_from(BASE64_GROUP_BYTES - 1 - position).unwrap_or(0));
        }
        for position in 0..(group.len() + 1).min(BASE64_GROUP_CHARACTERS) {
            let shift = BASE64_CHARACTER_BITS
                * u32::try_from(BASE64_GROUP_CHARACTERS - 1 - position).unwrap_or(0);
            let index = ((held >> shift) & BASE64_CHARACTER_MASK) as usize;
            written.push(char::from(BASE64_ALPHABET[index]));
        }
    }
    written
}

/// Returns one value as it would appear inside a string.
fn json_escaped_of(value: &str) -> String {
    let written = serde_json::to_string(value).unwrap_or_default();
    written.trim_matches('"').to_owned()
}
