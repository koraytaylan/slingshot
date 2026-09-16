//! Authority parsing for canonical profile addresses.

use super::{PORT_SEPARATOR, ProfileAuthenticationContract, ProfileDocumentFailure, narrow_limit};

/// Largest port number a base address may name.
const MAXIMUM_PORT: u32 = 65_535;

/// Opening byte of a bracketed internet-protocol version six host.
const BRACKET_OPEN: char = '[';

/// Closing byte of a bracketed internet-protocol version six host.
const BRACKET_CLOSE: char = ']';

/// Splits one authority into its lowercase host and its non-default port.
pub(super) fn split_authority(
    authority: &str,
    scheme: &str,
    contract: &ProfileAuthenticationContract,
) -> Result<(String, Option<u16>), ProfileDocumentFailure> {
    let refuse = || ProfileDocumentFailure::value("base_address");
    let (host, port_text) = if authority.starts_with(BRACKET_OPEN) {
        let close = authority.find(BRACKET_CLOSE).ok_or_else(refuse)?;
        let (bracketed, remainder) = authority.split_at(close + 1);
        let port = if remainder.is_empty() {
            None
        } else {
            Some(remainder.strip_prefix(PORT_SEPARATOR).ok_or_else(refuse)?)
        };
        (bracketed, port)
    } else {
        match authority.split_once(PORT_SEPARATOR) {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    let host = host.to_ascii_lowercase();
    if !is_usable_host(&host, narrow_limit(contract.limits.maximum_tier_host_bytes)) {
        return Err(refuse());
    }
    let default = if scheme == contract.literals.schemes[0] {
        contract.literals.scheme_default_ports.http
    } else {
        contract.literals.scheme_default_ports.https
    };
    let port = match port_text {
        None => None,
        Some(text) => {
            let port = parse_port(text)?;
            if port == default {
                return Err(refuse());
            }
            Some(port)
        }
    };
    Ok((host, port))
}

/// Parses a digits-only port, refusing a leading zero or an out-of-range value.
fn parse_port(text: &str) -> Result<u16, ProfileDocumentFailure> {
    let refuse = || ProfileDocumentFailure::value("base_address");
    if text.is_empty()
        || !text.bytes().all(|byte| byte.is_ascii_digit())
        || (text.len() > 1 && text.starts_with('0'))
    {
        return Err(refuse());
    }
    let port: u32 = text.parse().map_err(|_| refuse())?;
    if port == 0 || port > MAXIMUM_PORT {
        return Err(refuse());
    }
    u16::try_from(port).map_err(|_| refuse())
}

/// Reports whether one lowercase host is usable and unambiguous.
fn is_usable_host(host: &str, maximum_bytes: usize) -> bool {
    if host.is_empty() || host.len() > maximum_bytes {
        return false;
    }
    if let Some(literal) =
        host.strip_prefix(BRACKET_OPEN).and_then(|rest| rest.strip_suffix(BRACKET_CLOSE))
    {
        return literal.parse::<std::net::Ipv6Addr>().is_ok()
            && literal
                .chars()
                .all(|character| character.is_ascii_hexdigit() || character == PORT_SEPARATOR);
    }
    if host.starts_with('.') || host.ends_with('.') {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.chars().all(|character| character.is_ascii_alphanumeric() || character == '-')
    })
}
