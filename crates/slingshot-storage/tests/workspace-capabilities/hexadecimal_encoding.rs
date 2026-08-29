//! Probe for the hexadecimal-encoding capability.
//!
//! Requires lowercase rendering, exact-length decoding back to the same bytes,
//! and refusal of an odd-length or non-hexadecimal input.

#[test]
fn bytes_render_as_lowercase_hexadecimal_and_read_back_exactly() {
    let bytes = [0x00_u8, 0x0f, 0xa1, 0xff];
    let rendered = hex::encode(bytes);
    assert_eq!(rendered, "000fa1ff");
    assert_eq!(hex::decode(&rendered).expect("the text decodes"), bytes);
    assert!(hex::decode("0f0").is_err(), "an odd-length input must be refused");
    assert!(hex::decode("zz").is_err(), "a non-hexadecimal input must be refused");
}
