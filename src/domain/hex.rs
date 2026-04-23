//! Hand-written hex parse and format.
//!
//! Pure, no I/O, trivially testable. See spec for exact rules:
//! tokens split on ASCII whitespace, case-insensitive, concatenated then
//! split into pairs, odd length is an error, empty payload is an error.

use crate::domain::error::DomainError;

/// Parse a slice of whitespace-separated hex tokens into raw bytes.
///
/// Rules:
/// * empty input -> `EmptyPayload`,
/// * any non-hex character -> `InvalidHex`,
/// * odd total digit count -> `InvalidHex("odd number of hex digits")`.
pub fn parse_hex_tokens(tokens: &[&str]) -> Result<Vec<u8>, DomainError>
{
    if tokens.is_empty()
    {
        return Err(DomainError::EmptyPayload);
    }

    let mut joined = String::new();
    for t in tokens
    {
        for c in t.chars()
        {
            if !c.is_ascii_hexdigit()
            {
                return Err(DomainError::InvalidHex(format!(
                    "non-hex character: {c:?}"
                )));
            }
            joined.push(c);
        }
    }

    if joined.is_empty()
    {
        return Err(DomainError::EmptyPayload);
    }

    if !joined.len().is_multiple_of(2)
    {
        return Err(DomainError::InvalidHex(
            "odd number of hex digits".to_string(),
        ));
    }

    let mut out = Vec::with_capacity(joined.len() / 2);
    let bytes  = joined.as_bytes();
    let mut i  = 0_usize;
    while i < bytes.len()
    {
        let pair = std::str::from_utf8(&bytes[i..i + 2]).map_err(|_|
        {
            DomainError::InvalidHex("non-ascii hex pair".to_string())
        })?;
        let byte = u8::from_str_radix(pair, 16).map_err(|e|
        {
            DomainError::InvalidHex(format!("parse pair {pair:?}: {e}"))
        })?;
        out.push(byte);
        i += 2;
    }

    Ok(out)
}

/// Format bytes as space-separated uppercase hex, e.g. `49 35 00 11`.
pub fn format_hex_spaced(bytes: &[u8]) -> String
{
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate()
    {
        if i > 0
        {
            s.push(' ');
        }
        // 02X: uppercase, zero-padded, two digits.
        s.push_str(&format!("{b:02X}"));
    }
    s
}