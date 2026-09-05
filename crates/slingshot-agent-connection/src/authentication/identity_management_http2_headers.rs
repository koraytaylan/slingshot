//! IMS canonical decoded-section accounting, independent of author policy.

use crate::{
    author_hypertext_transfer_protocol_policy::HeadBounds,
    selected_author_hpack_block::DecodedHeaderSink,
    selected_author_http2_headers::DecodedHeadRefusal,
};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode as Code, ProfileAuthenticationContract,
};

/// One bounded IMS section. Status handling/media/trailer refusal still belong
/// to the response assembler, after complete section accounting has succeeded.
pub struct IdentityManagementDecodedSection {
    /// Present only for an initial response head, not a trailer section.
    pub status: Option<u16>,
    /// Exact ordered decoded fields; duplicates remain distinct charges.
    pub fields: Vec<(String, String)>,
}
impl core::fmt::Debug for IdentityManagementDecodedSection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("IdentityManagementDecodedSection([redacted])")
    }
}
struct Field {
    name: Vec<u8>,
    value: Vec<u8>,
    in_value: bool,
}

/// Incremental IMS HPACK destination: status charges only the fixed canonical
/// status line, while ordinary fields charge name + value + three delimiters.
pub struct IdentityManagementHeadReader {
    bounds: HeadBounds,
    charge: u64,
    field_charge: u64,
    trailers: bool,
    status: Option<u16>,
    fields: Vec<(String, String)>,
    current: Option<Field>,
    failure: Option<Code>,
}
impl core::fmt::Debug for IdentityManagementHeadReader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("IdentityManagementHeadReader([redacted])")
    }
}
impl IdentityManagementHeadReader {
    /// Starts an initial response head using only IMS manifest limits.
    pub fn response() -> Self {
        Self::new(false)
    }
    /// Starts a trailer section, including an explicitly empty one. The caller
    /// must refuse its presence after successful accounting, not discard it.
    pub fn trailers() -> Self {
        Self::new(true)
    }
    fn new(trailers: bool) -> Self {
        let limits = &ProfileAuthenticationContract::embedded().limits;
        Self {
            bounds: HeadBounds {
                field_bytes: limits.maximum_identity_management_response_header_bytes,
                field_count: limits.maximum_identity_management_response_header_count,
                head_bytes: limits.maximum_identity_management_response_head_bytes,
            },
            charge: if trailers {
                limits.identity_management_response_trailer_charge_bytes
            } else {
                limits.identity_management_response_head_status_charge_bytes
            },
            field_charge: limits.identity_management_response_field_charge_bytes,
            trailers,
            status: None,
            fields: Vec::new(),
            current: None,
            failure: None,
        }
    }
    /// Stable classification of the first refusal, with no remote text.
    pub fn failure_code(&self) -> Option<Code> {
        self.failure
    }
    fn refuse(&mut self, code: Code) -> Result<(), DecodedHeadRefusal> {
        self.failure.get_or_insert(code);
        Err(DecodedHeadRefusal)
    }
    fn usable(&self) -> Result<(), DecodedHeadRefusal> {
        if self.failure.is_some() { Err(DecodedHeadRefusal) } else { Ok(()) }
    }
    fn charge(&mut self, count: u64) -> Result<(), DecodedHeadRefusal> {
        match self.charge.checked_add(count).filter(|charge| *charge <= self.bounds.head_bytes) {
            Some(charge) => {
                self.charge = charge;
                Ok(())
            }
            None => self.refuse(Code::IdentityManagementResponseHeadLimitExceeded),
        }
    }
}
impl DecodedHeaderSink for IdentityManagementHeadReader {
    type Output = IdentityManagementDecodedSection;
    fn begin_field(&mut self) -> Result<(), DecodedHeadRefusal> {
        self.usable()?;
        if self.current.is_some() {
            return self.refuse(Code::IdentityManagementTransportFailed);
        }
        self.current = Some(Field { name: Vec::new(), value: Vec::new(), in_value: false });
        Ok(())
    }
    fn name_byte(&mut self, byte: u8) -> Result<(), DecodedHeadRefusal> {
        self.usable()?;
        let Some(field) = &self.current else {
            return self.refuse(Code::IdentityManagementTransportFailed);
        };
        let first = field.name.is_empty();
        let pseudo = if first { byte == b':' } else { field.name[0] == b':' };
        if field.in_value
            || !(byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || b"!#$%&'*+-.^_`|~".contains(&byte)
                || first && byte == b':')
        {
            return self.refuse(Code::IdentityManagementTransportFailed);
        }
        if pseudo {
            if self.trailers
                || self.status.is_some()
                || !self.fields.is_empty()
                || b":status".get(field.name.len()) != Some(&byte)
            {
                return self.refuse(Code::IdentityManagementTransportFailed);
            }
        } else {
            if !self.trailers && self.status.is_none() {
                return self.refuse(Code::IdentityManagementTransportFailed);
            }
            if field.name.len() as u64 >= self.bounds.field_bytes
                || self.fields.len() as u64 >= self.bounds.field_count
            {
                return self.refuse(Code::IdentityManagementResponseHeadLimitExceeded);
            }
            if first {
                self.charge(self.field_charge)?;
            }
            self.charge(1)?;
        }
        self.current.as_mut().unwrap().name.push(byte);
        Ok(())
    }
    fn begin_value(&mut self) -> Result<(), DecodedHeadRefusal> {
        self.usable()?;
        let Some(field) = &mut self.current else {
            return self.refuse(Code::IdentityManagementTransportFailed);
        };
        if field.in_value
            || field.name.is_empty()
            || field.name[0] == b':' && field.name != b":status"
        {
            return self.refuse(Code::IdentityManagementTransportFailed);
        }
        field.in_value = true;
        Ok(())
    }
    fn value_byte(&mut self, byte: u8) -> Result<(), DecodedHeadRefusal> {
        self.usable()?;
        let Some(field) = &self.current else {
            return self.refuse(Code::IdentityManagementTransportFailed);
        };
        if !field.in_value || byte < 32 || byte == 127 || field.value.is_empty() && byte == b' ' {
            return self.refuse(Code::IdentityManagementTransportFailed);
        }
        if field.name == b":status" {
            if !byte.is_ascii_digit() || field.value.len() >= 3 {
                return self.refuse(Code::IdentityManagementTransportFailed);
            }
        } else {
            if (field.name.len() + field.value.len()) as u64 >= self.bounds.field_bytes {
                return self.refuse(Code::IdentityManagementResponseHeadLimitExceeded);
            }
            self.charge(1)?;
        }
        self.current.as_mut().unwrap().value.push(byte);
        Ok(())
    }
    fn end_field(&mut self) -> Result<(), DecodedHeadRefusal> {
        self.usable()?;
        let Some(field) = self.current.take() else {
            return self.refuse(Code::IdentityManagementTransportFailed);
        };
        if !field.in_value || field.value.last() == Some(&b' ') {
            return self.refuse(Code::IdentityManagementTransportFailed);
        }
        if field.name == b":status" {
            if field.value.len() != 3 {
                return self.refuse(Code::IdentityManagementTransportFailed);
            }
            let status = u16::from(field.value[0] - b'0') * 100
                + u16::from(field.value[1] - b'0') * 10
                + u16::from(field.value[2] - b'0');
            if !(100..600).contains(&status) {
                return self.refuse(Code::IdentityManagementTransportFailed);
            }
            self.status = Some(status);
        } else {
            if [
                b"connection".as_slice(),
                b"proxy-connection",
                b"keep-alive",
                b"transfer-encoding",
                b"te",
                b"upgrade",
            ]
            .contains(&field.name.as_slice())
            {
                return self.refuse(Code::IdentityManagementTransportFailed);
            }
            let Ok(name) = String::from_utf8(field.name) else {
                return self.refuse(Code::IdentityManagementTransportFailed);
            };
            let Ok(value) = String::from_utf8(field.value) else {
                return self.refuse(Code::IdentityManagementTransportFailed);
            };
            self.fields.push((name, value));
        }
        Ok(())
    }
    fn finish(mut self) -> Result<Self::Output, DecodedHeadRefusal> {
        self.usable()?;
        if self.current.is_some() || !self.trailers && self.status.is_none() {
            return Err(DecodedHeadRefusal);
        }
        self.charge(0)?;
        Ok(IdentityManagementDecodedSection { status: self.status, fields: self.fields })
    }
}

#[cfg(test)]
#[path = "identity_management_http2_headers_tests.rs"]
mod tests;
