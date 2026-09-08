//! Incremental canonical JSON syntax validation without retaining a value tree.

use std::io::{BufReader, Read};

/// Explicit bounds supplied by the document contract using this reader.
#[derive(Clone, Copy)]
pub struct Bounds {
    /// Maximum complete document length.
    pub bytes: u64,
    /// Maximum spelling of any one string, number or literal, including quotes.
    pub token_bytes: usize,
    /// Maximum nested container count.
    pub depth: usize,
}

/// Maximum nesting accepted by both streaming document readers.
pub const MAXIMUM_READER_DEPTH: usize = 128;

/// Opaque failure: source bytes and filesystem diagnostics never escape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("canonical document verification failed")]
pub struct Refusal;

/// Checks exact canonical spelling, unique ascending object names, one value
/// and EOF. Array order and schema semantics remain the document codec's job.
/// Memory is bounded by container depth times key length plus one scalar token;
/// no array or object value tree is collected.
///
/// # Errors
///
/// Returns [`Refusal`] when the input is not canonical or exceeds any bound.
pub fn require_canonical_reader(source: impl Read, bounds: Bounds) -> Result<(), Refusal> {
    if bounds.depth > MAXIMUM_READER_DEPTH || bounds.depth == 0 || bounds.token_bytes == 0 {
        return Err(Refusal);
    }
    let mut reader = Reader { source: BufReader::new(source), next: None, consumed: 0, bounds };
    reader.value(0)?;
    if reader.peek()?.is_some() {
        return Err(Refusal);
    }
    Ok(())
}

pub(super) struct Reader<Source> {
    source: BufReader<Source>,
    next: Option<u8>,
    consumed: u64,
    bounds: Bounds,
}

impl<Source: Read> Reader<Source> {
    pub(super) fn new(source: Source, bounds: Bounds) -> Self {
        Self { source: BufReader::new(source), next: None, consumed: 0, bounds }
    }

    pub(super) fn peek(&mut self) -> Result<Option<u8>, Refusal> {
        if self.next.is_none() {
            let mut byte = [0];
            if self.source.read(&mut byte).map_err(|_| Refusal)? != 0 {
                self.consumed = self.consumed.checked_add(1).ok_or(Refusal)?;
                if self.consumed > self.bounds.bytes {
                    return Err(Refusal);
                }
                self.next = Some(byte[0]);
            }
        }
        Ok(self.next)
    }

    pub(super) fn take(&mut self) -> Result<u8, Refusal> {
        let byte = self.peek()?.ok_or(Refusal)?;
        self.next = None;
        Ok(byte)
    }

    pub(super) fn expect(&mut self, byte: u8) -> Result<(), Refusal> {
        if self.take()? != byte {
            return Err(Refusal);
        }
        Ok(())
    }

    fn append(&self, token: &mut Vec<u8>, byte: u8) -> Result<(), Refusal> {
        if token.len() >= self.bounds.token_bytes {
            return Err(Refusal);
        }
        token.push(byte);
        Ok(())
    }

    pub(super) fn string(&mut self) -> Result<String, Refusal> {
        self.expect(b'"')?;
        let mut token = vec![b'"'];
        loop {
            let byte = self.take()?;
            self.append(&mut token, byte)?;
            match byte {
                b'"' => break,
                b'\\' => {
                    let escaped = self.take()?;
                    self.append(&mut token, escaped)?;
                }
                _ => {}
            }
        }
        let value = super::canonical_json::require_canonical_bytes(&token).map_err(|_| Refusal)?;
        value.as_str().map(str::to_owned).ok_or(Refusal)
    }

    fn value(&mut self, depth: usize) -> Result<(), Refusal> {
        match self.peek()?.ok_or(Refusal)? {
            b'"' => {
                self.string()?;
            }
            b'{' | b'[' => {
                if depth >= self.bounds.depth {
                    return Err(Refusal);
                }
                let object = self.take()? == b'{';
                let close = if object { b'}' } else { b']' };
                if self.peek()? == Some(close) {
                    self.take()?;
                    return Ok(());
                }
                let mut previous: Option<String> = None;
                loop {
                    if object {
                        let key = self.string()?;
                        if previous.as_ref().is_some_and(|prior| prior.as_bytes() >= key.as_bytes())
                        {
                            return Err(Refusal);
                        }
                        previous = Some(key);
                        self.expect(b':')?;
                    }
                    self.value(depth + 1)?;
                    match self.take()? {
                        byte if byte == close => break,
                        b',' => {}
                        _ => return Err(Refusal),
                    }
                }
            }
            _ => {
                let mut token = Vec::new();
                while let Some(byte) = self.peek()? {
                    if matches!(byte, b',' | b']' | b'}') {
                        break;
                    }
                    let byte = self.take()?;
                    self.append(&mut token, byte)?;
                }
                let value =
                    super::canonical_json::require_canonical_bytes(&token).map_err(|_| Refusal)?;
                if !matches!(
                    value,
                    serde_json::Value::Number(_)
                        | serde_json::Value::Bool(_)
                        | serde_json::Value::Null
                ) {
                    return Err(Refusal);
                }
            }
        }
        Ok(())
    }
}
