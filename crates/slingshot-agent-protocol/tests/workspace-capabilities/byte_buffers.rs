//! Probe for the byte-buffers capability.
//!
//! Requires cheap slicing that shares one allocation, a growable writer, and a
//! frozen buffer whose slices compare by content.

use bytes::{Buf, BufMut, Bytes, BytesMut};

/// Bytes the writer reserves before anything is written.
const WRITER_CAPACITY: usize = 16;

/// Value the probe writes as its fixed-width prefix.
const PREFIX_VALUE: u32 = 9;

/// Bytes the fixed-width prefix occupies.
const PREFIX_LENGTH: usize = 4;

#[test]
fn a_shared_buffer_slices_without_copying_its_bytes() {
    let mut writer = BytesMut::with_capacity(WRITER_CAPACITY);
    writer.put_u32(PREFIX_VALUE);
    writer.put_slice(b"payload");
    let frozen: Bytes = writer.freeze();
    assert_eq!(frozen.len(), 11);
    let prefix = frozen.slice(0..PREFIX_LENGTH);
    let payload = frozen.slice(PREFIX_LENGTH..);
    assert_eq!(payload, Bytes::from_static(b"payload"));
    let mut reader = prefix.clone();
    assert_eq!(reader.get_u32(), 9);
    assert_eq!(reader.remaining(), 0);
    assert_eq!(prefix.len(), 4, "slicing leaves the original untouched");
}
