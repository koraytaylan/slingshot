//! Probe for the byte-buffers capability.
//!
//! Requires cheap slicing that shares one allocation, a growable writer, and a
//! frozen buffer whose slices compare by content.

use bytes::{Buf, BufMut, Bytes, BytesMut};

#[test]
fn a_shared_buffer_slices_without_copying_its_bytes() {
    let mut writer = BytesMut::with_capacity(16);
    writer.put_u32(9);
    writer.put_slice(b"payload");
    let frozen: Bytes = writer.freeze();
    assert_eq!(frozen.len(), 11);
    let prefix = frozen.slice(0..4);
    let payload = frozen.slice(4..);
    assert_eq!(payload, Bytes::from_static(b"payload"));
    let mut reader = prefix.clone();
    assert_eq!(reader.get_u32(), 9);
    assert_eq!(reader.remaining(), 0);
    assert_eq!(prefix.len(), 4, "slicing leaves the original untouched");
}
