//! IMS accounting through the real incremental HPACK decoder.

use super::*;
use crate::selected_author_hpack_block::ResponseBlock;

fn field(
    reader: &mut IdentityManagementHeadReader,
    name: &[u8],
    value: &[u8],
) -> Result<(), DecodedHeadRefusal> {
    reader.begin_field()?;
    for byte in name {
        reader.name_byte(*byte)?;
    }
    reader.begin_value()?;
    for byte in value {
        reader.value_byte(*byte)?;
    }
    reader.end_field()
}

#[test]
fn declared_literal_lengths_keep_limit_refusals_distinct_from_syntax() {
    for huffman in [0, 0x80] {
        for (wire, bound) in [
            (vec![0x88, 0, huffman | 5], 7),
            (vec![0x88, 0, 1, b'x', huffman | 5], 8),
            (vec![0x88, 0, huffman | 127, 1], 130),
            (vec![0x88, 0, 1, b'x', huffman | 127, 1], 132),
            (vec![0x88, 0, huffman | 127, 0], 130),
            (vec![0x88, 0, 1, b'x', huffman | 127, 0], 132),
        ] {
            for split in 0..wire.len() {
                let mut block =
                    ResponseBlock::with_reader(IdentityManagementHeadReader::response(), bound);
                block.push(&wire[..split]).unwrap();
                assert!(block.push(&wire[split..]).is_err());
                assert!(block.encoded_limit_exceeded(), "split={split} wire={wire:?}");
                assert!(block.push(b"ignored").is_err());
                assert!(block.encoded_limit_exceeded());
            }
        }
    }
    let mut exact = ResponseBlock::with_reader(IdentityManagementHeadReader::response(), 6);
    exact.push(&[0x88, 0, 1, b'x', 1, b'y']).unwrap();
    assert!(!exact.encoded_limit_exceeded());
    assert_eq!(exact.finish().unwrap().fields, [("x".to_owned(), "y".to_owned())]);
    let mut wire = vec![0x88, 0, 1, b'x', 127, 0];
    wire.extend_from_slice(&[b'y'; 127]);
    let mut exact =
        ResponseBlock::with_reader(IdentityManagementHeadReader::response(), wire.len() as u64);
    for byte in wire {
        exact.push(&[byte]).unwrap();
    }
    assert!(!exact.encoded_limit_exceeded());
    assert_eq!(exact.finish().unwrap().fields, [("x".to_owned(), "y".repeat(127))]);
    let mut malformed = ResponseBlock::with_reader(IdentityManagementHeadReader::response(), 6);
    assert!(malformed.push(&[0x88, 0, 1, b'x', 0x81, 0xff]).is_err());
    assert!(
        !malformed.encoded_limit_exceeded(),
        "bad Huffman padding is not an encoded length bound"
    );
}

#[test]
fn canonical_ims_charges_exclude_status_pseudo_field_and_preserve_empty_trailers() {
    let mut reader = IdentityManagementHeadReader::response();
    reader.bounds = HeadBounds { field_bytes: 2, field_count: 1, head_bytes: 10 };
    field(&mut reader, b":status", b"200").unwrap();
    field(&mut reader, b"x", b"y").unwrap();
    assert_eq!(reader.charge, 5 + 1 + 1 + 3);
    let section = reader.finish().unwrap();
    assert_eq!(section.status, Some(200));
    assert_eq!(section.fields, [("x".to_owned(), "y".to_owned())]);
    let mut trailer = IdentityManagementHeadReader::trailers();
    trailer.bounds = HeadBounds { field_bytes: 2, field_count: 1, head_bytes: 6 };
    field(&mut trailer, b"x", b"y").unwrap();
    assert_eq!(trailer.charge, 1 + 1 + 1 + 3);
    assert_eq!(trailer.finish().unwrap().status, None);
    let empty = IdentityManagementHeadReader::trailers().finish().unwrap();
    assert!(empty.fields.is_empty() && empty.status.is_none());
    assert_eq!(format!("{empty:?}"), "IdentityManagementDecodedSection([redacted])");
}

#[test]
fn next_field_byte_count_and_aggregate_fail_before_storage_and_stay_poisoned() {
    for bounds in [
        HeadBounds { field_bytes: 1, field_count: 1, head_bytes: 10 },
        HeadBounds { field_bytes: 2, field_count: 0, head_bytes: 10 },
        HeadBounds { field_bytes: 2, field_count: 1, head_bytes: 9 },
    ] {
        let mut reader = IdentityManagementHeadReader::response();
        reader.bounds = bounds;
        field(&mut reader, b":status", b"200").unwrap();
        assert!(field(&mut reader, b"x", b"y").is_err());
        assert_eq!(reader.failure_code(), Some(Code::IdentityManagementResponseHeadLimitExceeded));
        assert!(reader.fields.is_empty());
        assert!(reader.current.as_ref().unwrap().value.is_empty());
        assert!(reader.begin_field().is_err());
        assert!(reader.finish().is_err());
    }
}

#[test]
fn informational_and_redirect_statuses_survive_decoding_for_ims_precedence() {
    for status in [b"100", b"101", b"103", b"200", b"302", b"401", b"500"] {
        let mut block = ResponseBlock::with_reader(IdentityManagementHeadReader::response(), 1024);
        let mut wire = vec![0x08, 3]; // literal indexed name :status
        wire.extend_from_slice(status);
        block.push(&wire).unwrap();
        let section = block.finish().unwrap();
        assert_eq!(section.status.unwrap().to_string().as_bytes(), status);
    }
}

#[test]
fn duplicate_indexed_fields_and_huffman_fragments_use_the_ims_gate() {
    let wire = [0x88, 0x40, 1, b'x', 1, b'a', 0xbe, 0x0f, 47, 1, b'b'];
    for split in 0..=wire.len() {
        let mut block = ResponseBlock::with_reader(IdentityManagementHeadReader::response(), 1024);
        block.push(&wire[..split]).unwrap();
        block.push(&wire[split..]).unwrap();
        assert_eq!(block.reader().charge, 5 + 3 * (1 + 1 + 3));
        assert_eq!(
            block.finish().unwrap().fields,
            [
                ("x".to_owned(), "a".to_owned()),
                ("x".to_owned(), "a".to_owned()),
                ("x".to_owned(), "b".to_owned())
            ]
        );
    }
    let wire = [
        0x88, 0x40, 0x88, 0x25, 0xa8, 0x49, 0xe9, 0x5b, 0xa9, 0x7d, 0x7f, 0x89, 0x25, 0xa8, 0x49,
        0xe9, 0x5b, 0xb8, 0xe8, 0xb4, 0xbf,
    ];
    for maximum in [29, 30] {
        let mut reader = IdentityManagementHeadReader::response();
        reader.bounds.head_bytes = maximum;
        let mut block = ResponseBlock::with_reader(reader, 1024);
        if maximum == 29 {
            assert!(block.push(&wire).is_err());
            assert_eq!(
                block.reader().failure_code(),
                Some(Code::IdentityManagementResponseHeadLimitExceeded)
            );
        } else {
            for byte in wire {
                block.push(&[byte]).unwrap();
            }
            assert_eq!(
                block.finish().unwrap().fields,
                [("custom-key".to_owned(), "custom-value".to_owned())]
            );
        }
    }
}

#[test]
fn compressed_limit_and_syntax_failures_are_distinct_and_terminal() {
    let mut block = ResponseBlock::with_reader(IdentityManagementHeadReader::response(), 0);
    assert!(block.push(&[0x88]).is_err());
    assert!(block.encoded_limit_exceeded());
    assert!(block.push(&[]).is_err());
    let mut block = ResponseBlock::with_reader(IdentityManagementHeadReader::response(), 100);
    assert!(block.push(&[0x80]).is_err()); // forbidden HPACK index zero
    assert!(!block.encoded_limit_exceeded());
    assert_eq!(block.reader().failure_code(), None);
    assert!(block.finish().is_err());
}

#[test]
fn trailer_accounting_retains_the_connection_table_and_resets_section_charges() {
    let mut head = ResponseBlock::with_reader(IdentityManagementHeadReader::response(), 1024);
    head.push(&[0x88, 0x40, 1, b'x', 1, b'y']).unwrap();
    let (decoded, mut trailer) =
        head.finish_with_next_reader(IdentityManagementHeadReader::trailers(), 1).unwrap();
    assert_eq!(decoded.status, Some(200));
    trailer.push(&[0xbe]).unwrap(); // Same connection's dynamic index 62.
    assert_eq!(trailer.reader().charge, 1 + 1 + 1 + 3);
    let decoded = trailer.finish().unwrap();
    assert_eq!(decoded.status, None);
    assert_eq!(decoded.fields, [("x".to_owned(), "y".to_owned())]);
}

#[test]
fn pseudo_header_order_and_connection_specific_fields_fail_without_text() {
    for (name, value) in [
        (b"connection".as_slice(), b"close".as_slice()),
        (b"X", b"y"),
        (b":status", b"200"),
        (b"x", b"a\tb"),
        (b"x", b"y\r"),
        (b"upgrade", b"h2c"),
    ] {
        let mut reader = IdentityManagementHeadReader::response();
        field(&mut reader, b":status", b"200").unwrap();
        assert!(field(&mut reader, name, value).is_err());
        assert_eq!(reader.failure_code(), Some(Code::IdentityManagementTransportFailed));
        assert_eq!(format!("{reader:?}"), "IdentityManagementHeadReader([redacted])");
    }
    assert!(field(&mut IdentityManagementHeadReader::response(), b"x", b"y").is_err());
    assert!(field(&mut IdentityManagementHeadReader::trailers(), b":status", b"200").is_err());
    let mut reader = IdentityManagementHeadReader::response();
    field(&mut reader, b":status", b"200").unwrap();
    field(&mut reader, b"trailer", b"x").unwrap();
    field(&mut reader, b"alt-svc", b"ignored").unwrap();
    assert_eq!(reader.finish().unwrap().fields.len(), 2, "route checkpoints must see these fields");
}
