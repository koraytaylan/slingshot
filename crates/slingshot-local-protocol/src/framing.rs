//! Length-prefixed framing for the local control path.
//!
//! One frame is the foundation contract's fixed-width unsigned payload length
//! in network byte order, followed by exactly one JavaScript Object Notation
//! value. Parsing and rendering are pure over byte slices: this module holds no
//! socket, no clock, and no allocation beyond the declared frame limit, so a
//! server can apply the manifest deadlines without parsing a frame twice.

use crate::foundation_contract::FramingLimits;

/// Byte values that may follow a payload value without changing its meaning.
const INSIGNIFICANT_BYTES: &[u8] = b" \t\r\n";

/// Bytes one backslash escape sequence occupies.
const ESCAPE_SEQUENCE_LENGTH: usize = 2;

/// Reason a frame could not be read or rendered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FramingFailure {
    /// The declared payload length is beyond the frame limit.
    #[error("the declared payload of {declared} bytes is beyond the {limit}-byte frame limit")]
    PayloadTooLarge {
        /// Length the prefix declared.
        declared: u64,
        /// Largest payload the contract allows.
        limit: u64,
    },
    /// The payload is not valid text.
    #[error("the payload is not valid text: {0}")]
    NotText(String),
    /// The payload nests containers more deeply than the contract allows.
    #[error("the payload nests {depth} containers, beyond the limit of {limit}")]
    NestingTooDeep {
        /// Depth the payload reached.
        depth: u32,
        /// Deepest nesting the contract allows.
        limit: u32,
    },
    /// A container in the payload holds more entries than the contract allows.
    #[error("a payload container holds {items} entries, beyond the limit of {limit}")]
    CollectionTooLarge {
        /// Entries the container held.
        items: u32,
        /// Most entries the contract allows.
        limit: u32,
    },
    /// The payload ends inside a string, an array, or an object.
    #[error("the payload ends inside an unfinished container or string")]
    Unfinished,
    /// Bytes follow the payload value that are neither whitespace nor part of it.
    #[error("the payload carries a trailing value")]
    TrailingValue,
}

/// How much of one frame a buffer holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameProgress {
    /// The buffer holds no byte of a frame.
    Empty,
    /// The buffer holds part of the length prefix.
    PartialPrefix {
        /// Bytes of the prefix already received.
        received: usize,
    },
    /// The buffer holds the whole prefix and part of the payload.
    PartialPayload {
        /// Payload bytes already received.
        received: usize,
        /// Payload bytes the prefix declared.
        declared: usize,
    },
    /// The buffer holds one whole frame.
    Complete {
        /// Payload bytes the prefix declared.
        declared: usize,
    },
}

/// Reads the declared payload length out of a complete prefix.
fn declared_length(prefix: &[u8]) -> u64 {
    prefix.iter().fold(0_u64, |accumulated, byte| (accumulated << u8::BITS) | u64::from(*byte))
}

/// Reports how much of one frame `buffer` holds.
///
/// The report never parses the payload, so a server can decide which deadline
/// applies without reading the same bytes twice.
///
/// # Errors
///
/// Returns [`FramingFailure::PayloadTooLarge`] when the prefix declares more
/// bytes than the contract allows.
pub fn progress(limits: &FramingLimits, buffer: &[u8]) -> Result<FrameProgress, FramingFailure> {
    let prefix_length = limits.length_prefix_bytes as usize;
    if buffer.is_empty() {
        return Ok(FrameProgress::Empty);
    }
    if buffer.len() < prefix_length {
        return Ok(FrameProgress::PartialPrefix { received: buffer.len() });
    }
    let declared = declared_length(&buffer[..prefix_length]);
    if declared > u64::from(limits.maximum_payload_bytes) {
        return Err(FramingFailure::PayloadTooLarge {
            declared,
            limit: u64::from(limits.maximum_payload_bytes),
        });
    }
    let declared = declared as usize;
    let received = buffer.len() - prefix_length;
    if received < declared {
        Ok(FrameProgress::PartialPayload { received, declared })
    } else {
        Ok(FrameProgress::Complete { declared })
    }
}

/// Renders one payload as a frame.
///
/// # Errors
///
/// Returns [`FramingFailure::PayloadTooLarge`] when the payload is beyond the
/// contract's frame limit.
pub fn render(limits: &FramingLimits, payload: &[u8]) -> Result<Vec<u8>, FramingFailure> {
    let limit = u64::from(limits.maximum_payload_bytes);
    let declared = payload.len() as u64;
    if declared > limit {
        return Err(FramingFailure::PayloadTooLarge { declared, limit });
    }
    let prefix_length = limits.length_prefix_bytes as usize;
    let mut frame = Vec::with_capacity(prefix_length + payload.len());
    for position in (0..prefix_length).rev() {
        let shift = u32::try_from(position).unwrap_or_default() * u8::BITS;
        frame.push(((declared >> shift) & u64::from(u8::MAX)) as u8);
    }
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// One container the structural scan is inside.
#[derive(Debug, Clone, Copy)]
struct OpenContainer {
    /// Entries the container holds so far.
    items: u32,
    /// Whether any byte of the current entry has been seen.
    entry_started: bool,
}

/// Tracks the deepest nesting and the largest container of one payload.
#[derive(Debug, Default)]
struct StructuralScan {
    /// Containers currently open, innermost last.
    open: Vec<OpenContainer>,
    /// Deepest nesting reached.
    depth: u32,
    /// Largest container seen.
    items: u32,
    /// Whether any value has been closed at the top level.
    completed: bool,
    /// Whether a byte has been seen outside every container.
    top_level_started: bool,
}

impl StructuralScan {
    /// Records the start of a container.
    fn open_container(&mut self) {
        self.mark_entry();
        self.open.push(OpenContainer { items: 0, entry_started: false });
        self.depth = self.depth.max(self.open.len() as u32);
    }

    /// Records the end of a container.
    fn close_container(&mut self) -> Result<(), FramingFailure> {
        let closed = self.open.pop().ok_or(FramingFailure::TrailingValue)?;
        let items = closed.items + u32::from(closed.entry_started);
        self.items = self.items.max(items);
        if self.open.is_empty() {
            self.completed = true;
        }
        Ok(())
    }

    /// Records that the current entry has content.
    fn mark_entry(&mut self) {
        match self.open.last_mut() {
            Some(container) => container.entry_started = true,
            None => self.top_level_started = true,
        }
    }

    /// Records an entry separator.
    fn separate_entry(&mut self) {
        if let Some(container) = self.open.last_mut() {
            container.items += 1;
            container.entry_started = false;
        }
    }
}

/// Advances the scan past one string literal and returns the next offset.
fn skip_string(payload: &[u8], start: usize) -> Result<usize, FramingFailure> {
    let mut position = start + 1;
    while position < payload.len() {
        match payload[position] {
            b'\\' => position += ESCAPE_SEQUENCE_LENGTH,
            b'"' => return Ok(position + 1),
            _ => position += 1,
        }
    }
    Err(FramingFailure::Unfinished)
}

/// Reports the deepest nesting and the largest container of one payload.
fn scan(payload: &[u8]) -> Result<StructuralScan, FramingFailure> {
    let mut scan = StructuralScan::default();
    let mut position = 0_usize;
    while position < payload.len() {
        let byte = payload[position];
        match byte {
            b'"' => {
                scan.mark_entry();
                position = skip_string(payload, position)?;
                if scan.open.is_empty() {
                    scan.completed = true;
                }
                continue;
            }
            b'{' | b'[' => scan.open_container(),
            b'}' | b']' => scan.close_container()?,
            b',' | b':' => scan.separate_entry(),
            byte if INSIGNIFICANT_BYTES.contains(&byte) => {}
            _ => scan.mark_entry(),
        }
        position += 1;
    }
    if scan.open.is_empty() { Ok(scan) } else { Err(FramingFailure::Unfinished) }
}

/// Reads one frame payload, refusing every bound the contract declares.
///
/// The payload must be valid text holding exactly one value, nested no deeper
/// and holding no larger a container than the contract allows, with nothing but
/// insignificant bytes after it.
///
/// # Errors
///
/// Returns the [`FramingFailure`] that names the first bound the payload
/// breaks.
pub fn read_payload<'payload>(
    limits: &FramingLimits,
    payload: &'payload [u8],
) -> Result<&'payload str, FramingFailure> {
    if payload.len() > limits.maximum_payload_bytes as usize {
        return Err(FramingFailure::PayloadTooLarge {
            declared: payload.len() as u64,
            limit: u64::from(limits.maximum_payload_bytes),
        });
    }
    let text = std::str::from_utf8(payload)
        .map_err(|failure| FramingFailure::NotText(failure.to_string()))?;
    let measured = scan(payload)?;
    if measured.depth > limits.maximum_nesting_depth {
        return Err(FramingFailure::NestingTooDeep {
            depth: measured.depth,
            limit: limits.maximum_nesting_depth,
        });
    }
    if measured.items > limits.maximum_collection_items {
        return Err(FramingFailure::CollectionTooLarge {
            items: measured.items,
            limit: limits.maximum_collection_items,
        });
    }
    Ok(text)
}
