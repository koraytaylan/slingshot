//! HPACK static and bounded dynamic tables for one selected-author connection.
//! RFC 7541 sections 2.3 and 4 define indexing, entry size and eviction. This
//! table does not validate HTTP field semantics or authorize response evidence.

use std::collections::VecDeque;

/// HTTP/2's initial decoder table limit; the product must advertise this same
/// limit. Peer SETTINGS constrain our encoder, never widen this decoder table.
pub const MAXIMUM_DYNAMIC_TABLE_BYTES: u64 = 4096;

/// HPACK Appendix A's ordered static table. Index zero is deliberately absent.
const STATIC: [(&[u8], &[u8]); 61] = [
    (b":authority", b""),
    (b":method", b"GET"),
    (b":method", b"POST"),
    (b":path", b"/"),
    (b":path", b"/index.html"),
    (b":scheme", b"http"),
    (b":scheme", b"https"),
    (b":status", b"200"),
    (b":status", b"204"),
    (b":status", b"206"),
    (b":status", b"304"),
    (b":status", b"400"),
    (b":status", b"404"),
    (b":status", b"500"),
    (b"accept-charset", b""),
    (b"accept-encoding", b"gzip, deflate"),
    (b"accept-language", b""),
    (b"accept-ranges", b""),
    (b"accept", b""),
    (b"access-control-allow-origin", b""),
    (b"age", b""),
    (b"allow", b""),
    (b"authorization", b""),
    (b"cache-control", b""),
    (b"content-disposition", b""),
    (b"content-encoding", b""),
    (b"content-language", b""),
    (b"content-length", b""),
    (b"content-location", b""),
    (b"content-range", b""),
    (b"content-type", b""),
    (b"cookie", b""),
    (b"date", b""),
    (b"etag", b""),
    (b"expect", b""),
    (b"expires", b""),
    (b"from", b""),
    (b"host", b""),
    (b"if-match", b""),
    (b"if-modified-since", b""),
    (b"if-none-match", b""),
    (b"if-range", b""),
    (b"if-unmodified-since", b""),
    (b"last-modified", b""),
    (b"link", b""),
    (b"location", b""),
    (b"max-forwards", b""),
    (b"proxy-authenticate", b""),
    (b"proxy-authorization", b""),
    (b"range", b""),
    (b"referer", b""),
    (b"refresh", b""),
    (b"retry-after", b""),
    (b"server", b""),
    (b"set-cookie", b""),
    (b"strict-transport-security", b""),
    (b"transfer-encoding", b""),
    (b"user-agent", b""),
    (b"vary", b""),
    (b"via", b""),
    (b"www-authenticate", b""),
];

/// Invalid table index, capacity update or size arithmetic, without wire text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the author HPACK table reference is invalid")]
pub struct TableRefusal;

/// Borrowed field octets. They still require the decoded HTTP response gate.
pub struct IndexedField<'table> {
    name: &'table [u8],
    value: &'table [u8],
}

impl IndexedField<'_> {
    /// Decoded name to feed through the incremental field-name checks.
    pub fn name(&self) -> &[u8] {
        self.name
    }
    /// Decoded value to feed through the incremental field-value checks.
    pub fn value(&self) -> &[u8] {
        self.value
    }
}

impl core::fmt::Debug for IndexedField<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("IndexedField([redacted])")
    }
}

struct Entry {
    name: Box<[u8]>,
    value: Box<[u8]>,
    charged_bytes: u64,
}

/// Per-connection dynamic entries, newest first, bounded before copying bytes.
pub struct HeaderTable {
    entries: VecDeque<Entry>,
    charged_bytes: u64,
    maximum: u64,
}

impl core::fmt::Debug for HeaderTable {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("HeaderTable([redacted])")
    }
}

impl Default for HeaderTable {
    fn default() -> Self {
        Self::new()
    }
}

impl HeaderTable {
    /// Starts an empty decoder table at the fixed advertised capacity.
    pub fn new() -> Self {
        Self { entries: VecDeque::new(), charged_bytes: 0, maximum: MAXIMUM_DYNAMIC_TABLE_BYTES }
    }

    /// Resolves static indexes 1..=61 and dynamic indexes starting at 62.
    /// Invalid indexes never become empty/default fields.
    pub fn lookup(&self, index: u64) -> Result<IndexedField<'_>, TableRefusal> {
        if index == 0 {
            return Err(TableRefusal);
        }
        if index <= STATIC.len() as u64 {
            let (name, value) = STATIC[index as usize - 1];
            return Ok(IndexedField { name, value });
        }
        let offset = usize::try_from(index - STATIC.len() as u64 - 1).map_err(|_| TableRefusal)?;
        let entry = self.entries.get(offset).ok_or(TableRefusal)?;
        Ok(IndexedField { name: &entry.name, value: &entry.value })
    }

    /// Applies a decoded HPACK table-size update, evicting oldest entries.
    /// The enclosing parser must restrict updates to the start of a block.
    pub fn resize(&mut self, maximum: u64) -> Result<(), TableRefusal> {
        if maximum > MAXIMUM_DYNAMIC_TABLE_BYTES {
            return Err(TableRefusal);
        }
        self.maximum = maximum;
        self.evict_to(maximum);
        Ok(())
    }

    /// Adds one incrementally indexed field, retaining duplicates as separate
    /// entries. An entry larger than capacity clears the table without storing
    /// it, as HPACK specifies; this is not a malformed-header refusal.
    pub fn insert(&mut self, name: &[u8], value: &[u8]) -> Result<(), TableRefusal> {
        let size = u64::try_from(name.len())
            .ok()
            .and_then(|name| {
                u64::try_from(value.len()).ok().and_then(|value| name.checked_add(value))
            })
            .and_then(|size| size.checked_add(32))
            .ok_or(TableRefusal)?;
        if size > self.maximum {
            self.evict_to(0);
            return Ok(());
        }
        self.evict_to(self.maximum - size);
        self.entries.push_front(Entry {
            name: name.into(),
            value: value.into(),
            charged_bytes: size,
        });
        self.charged_bytes += size;
        Ok(())
    }

    pub(crate) fn clear(&mut self) {
        self.evict_to(0);
    }

    fn evict_to(&mut self, maximum: u64) {
        while self.charged_bytes > maximum {
            let entry =
                self.entries.pop_back().expect("charged entries account for every table byte");
            self.charged_bytes -= entry.charged_bytes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_fields_still_require_the_decoded_response_policy() {
        use crate::selected_author_http2_headers::{DecodedHeadReader, DecodedHeadRefusal};
        fn feed(
            reader: &mut DecodedHeadReader,
            field: IndexedField<'_>,
        ) -> Result<(), DecodedHeadRefusal> {
            reader.begin_field()?;
            for byte in field.name() {
                reader.name_byte(*byte)?;
            }
            reader.begin_value()?;
            for byte in field.value() {
                reader.value_byte(*byte)?;
            }
            reader.end_field()
        }
        let mut table = HeaderTable::new();
        table.insert(b"content-type", b"application/json").unwrap();
        let mut reader = DecodedHeadReader::new();
        feed(&mut reader, table.lookup(8).unwrap()).unwrap();
        feed(&mut reader, table.lookup(62).unwrap()).unwrap();
        let (status, fields) = reader.finish().unwrap().into_parts();
        assert_eq!(status, 200);
        assert_eq!(fields["content-type"], "application/json");
        // Static table presence does not authorize a request pseudo-field in
        // a response, or a redirect prohibited by the selected-author policy.
        for index in [2, 4, 11] {
            assert!(feed(&mut DecodedHeadReader::new(), table.lookup(index).unwrap()).is_err());
        }
        table.insert(b"connection", b"close").unwrap();
        let mut reader = DecodedHeadReader::new();
        feed(&mut reader, table.lookup(8).unwrap()).unwrap();
        assert!(feed(&mut reader, table.lookup(62).unwrap()).is_err());
        assert!(reader.finish().is_err());
    }

    #[test]
    fn static_indexes_are_pinned_and_missing_indexes_refuse() {
        let table = HeaderTable::new();
        for (index, name, value) in [
            (1, b":authority".as_slice(), b"".as_slice()),
            (8, b":status", b"200"),
            (14, b":status", b"500"),
            (16, b"accept-encoding", b"gzip, deflate"),
            (31, b"content-type", b""),
            (61, b"www-authenticate", b""),
        ] {
            let field = table.lookup(index).unwrap();
            assert_eq!((field.name(), field.value()), (name, value));
        }
        for index in 1..=61 {
            assert!(!table.lookup(index).unwrap().name().is_empty());
        }
        for index in [0, 62, u64::MAX] {
            assert!(table.lookup(index).is_err());
        }
    }

    #[test]
    fn newest_first_duplicates_and_oldest_eviction_follow_entry_size() {
        let mut table = HeaderTable::new();
        table.resize(68).unwrap();
        table.insert(b"a", b"1").unwrap();
        table.insert(b"b", b"2").unwrap();
        assert_eq!(table.charged_bytes, 68);
        assert_eq!(table.lookup(62).unwrap().name(), b"b");
        assert_eq!(table.lookup(63).unwrap().name(), b"a");
        table.insert(b"b", b"2").unwrap();
        assert_eq!(table.entries.len(), 2);
        assert_eq!(table.lookup(63).unwrap().name(), b"b");
        assert!(table.lookup(64).is_err());
        table.resize(34).unwrap();
        assert_eq!(table.charged_bytes, 34);
        assert!(table.lookup(63).is_err());
        table.resize(33).unwrap();
        assert!(table.lookup(62).is_err());
    }

    #[test]
    fn oversized_entries_clear_without_insertion_and_zero_capacity_stays_empty() {
        let mut table = HeaderTable::new();
        table.resize(34).unwrap();
        table.insert(b"a", b"1").unwrap();
        table.insert(b"a", b"12").unwrap();
        assert_eq!(table.charged_bytes, 0);
        assert!(table.lookup(62).is_err());
        table.resize(0).unwrap();
        table.insert(b"a", b"1").unwrap();
        assert!(table.entries.is_empty());
        table.resize(34).unwrap();
        table.insert(b"a", b"1").unwrap();
        assert_eq!(table.lookup(62).unwrap().value(), b"1");
        assert!(table.resize(MAXIMUM_DYNAMIC_TABLE_BYTES + 1).is_err());
        assert_eq!(table.maximum, 34);
        assert_eq!(table.lookup(62).unwrap().value(), b"1");
    }

    #[test]
    fn full_capacity_is_bounded_and_private_fields_are_not_debug_output() {
        let mut table = HeaderTable::new();
        table.insert(b"x", &vec![b'y'; 4096 - 33]).unwrap();
        assert_eq!(table.charged_bytes, 4096);
        assert_eq!(table.entries.len(), 1);
        assert_eq!(format!("{table:?}"), "HeaderTable([redacted])");
        assert_eq!(format!("{:?}", table.lookup(62).unwrap()), "IndexedField([redacted])");
        for _ in 0..1000 {
            table.insert(b"x", b"y").unwrap();
        }
        assert_eq!(table.entries.len(), 4096 / 34);
        assert_eq!(table.charged_bytes, (4096 / 34) * 34);
        assert!(table.lookup(62 + table.entries.len() as u64).is_err());
    }
}
