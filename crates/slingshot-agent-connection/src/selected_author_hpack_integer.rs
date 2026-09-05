//! Incremental HPACK prefix integers with immutable per-use value bounds.
//! RFC 7541 section 5.1 permits implementation limits on values and octets;
//! this reader supports u64 values and at most ten continuation octets.

/// Invalid prefix, overflow, exceeded usage bound or malformed reader state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the author HPACK integer is invalid")]
pub struct IntegerRefusal;

/// One prefix integer. An error permanently prevents retrieving a value.
pub struct PrefixedInteger {
    maximum: u64,
    value: u64,
    shift: u32,
    complete: bool,
    poisoned: bool,
    limit_exceeded: bool,
}

impl core::fmt::Debug for PrefixedInteger {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("PrefixedInteger([redacted])")
    }
}

impl PrefixedInteger {
    /// Consumes the representation's first octet. Bits outside the prefix
    /// belong to the enclosing HPACK instruction and are not integer bits.
    pub fn start(first: u8, prefix_bits: u8, maximum: u64) -> Result<Self, IntegerRefusal> {
        if !(1..=8).contains(&prefix_bits) {
            return Err(IntegerRefusal);
        }
        let mask = (1_u16 << prefix_bits) - 1;
        let value = u64::from(u16::from(first) & mask);
        if value > maximum {
            return Err(IntegerRefusal);
        }
        Ok(Self { maximum, value, shift: 0, complete: value < u64::from(mask), poisoned: false, limit_exceeded: false })
    }

    /// Distinguishes a declared value above its usage bound from malformed syntax.
    pub(crate) fn limit_exceeded(&self) -> bool { self.limit_exceeded }

    /// Returns a complete value only, never a prefix interpreted as a length.
    pub fn value(&self) -> Option<u64> {
        (self.complete && !self.poisoned).then_some(self.value)
    }

    /// Consumes one continuation octet without buffering the encoded input.
    pub fn push(&mut self, byte: u8) -> Result<Option<u64>, IntegerRefusal> {
        if self.poisoned || self.complete {
            self.poisoned = true;
            return Err(IntegerRefusal);
        }
        self.poisoned = true;
        let scale = 1_u64.checked_shl(self.shift).ok_or(IntegerRefusal)?;
        let increment = u64::from(byte & 0x7f).checked_mul(scale).ok_or(IntegerRefusal)?;
        let value = self.value.checked_add(increment).ok_or(IntegerRefusal)?;
        if value > self.maximum {
            self.limit_exceeded = true;
            return Err(IntegerRefusal);
        }
        self.value = value;
        self.complete = byte & 0x80 == 0;
        if !self.complete {
            self.shift =
                self.shift.checked_add(7).filter(|shift| *shift < 64).ok_or(IntegerRefusal)?;
        }
        self.poisoned = false;
        Ok(self.value())
    }

    /// Refuses truncated, overflowed or otherwise previously refused integers.
    pub fn finish(self) -> Result<u64, IntegerRefusal> {
        self.value().ok_or(IntegerRefusal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(value: u64, prefix: u8) -> Vec<u8> {
        let mask = (1_u64 << prefix) - 1;
        if value < mask {
            return vec![value as u8];
        }
        let mut bytes = vec![mask as u8];
        let mut remaining = value - mask;
        while remaining >= 128 {
            bytes.push((remaining as u8 & 0x7f) | 0x80);
            remaining >>= 7;
        }
        bytes.push(remaining as u8);
        bytes
    }

    #[test]
    fn rfc_examples_and_prefix_boundaries_decode_incrementally() {
        assert_eq!(PrefixedInteger::start(10, 5, 10).unwrap().finish().unwrap(), 10);
        let mut integer = PrefixedInteger::start(31, 5, 1337).unwrap();
        assert_eq!(integer.value(), None);
        assert_eq!(integer.push(154).unwrap(), None);
        assert_eq!(integer.push(10).unwrap(), Some(1337));
        assert_eq!(integer.finish().unwrap(), 1337);
        for prefix in 1..=8 {
            let mask = (1_u64 << prefix) - 1;
            for value in
                [0, mask - 1, mask, mask + 1, 127, 128, 255, 256, u32::MAX as u64, u64::MAX]
            {
                let bytes = encoded(value, prefix);
                let mut integer = PrefixedInteger::start(bytes[0], prefix, value).unwrap();
                for byte in &bytes[1..] {
                    integer.push(*byte).unwrap();
                }
                assert_eq!(integer.finish().unwrap(), value);
            }
        }
        assert_eq!(PrefixedInteger::start(0b1110_1010, 5, 10).unwrap().finish().unwrap(), 10);
    }

    #[test]
    fn overflow_usage_bounds_truncation_and_extra_octets_fail_closed() {
        for prefix in [0, 9, 255] {
            assert!(PrefixedInteger::start(0, prefix, u64::MAX).is_err());
        }
        assert!(PrefixedInteger::start(10, 5, 9).is_err());
        let mut bounded = PrefixedInteger::start(31, 5, 31).unwrap();
        assert!(bounded.push(1).is_err());
        assert_eq!(bounded.value(), None);
        assert!(bounded.push(0).is_err());
        assert!(bounded.finish().is_err());
        assert!(PrefixedInteger::start(31, 5, u64::MAX).unwrap().finish().is_err());
        let mut complete = PrefixedInteger::start(1, 5, 1).unwrap();
        assert!(complete.push(0).is_err());
        assert!(complete.finish().is_err());
        for final_byte in [2, 0x80, 0xff] {
            let mut overflow = PrefixedInteger::start(31, 5, u64::MAX).unwrap();
            for _ in 0..9 {
                overflow.push(0x80).unwrap();
            }
            assert!(overflow.push(final_byte).is_err());
            assert_eq!(overflow.value(), None);
        }
        let mut additive_overflow = PrefixedInteger::start(31, 5, u64::MAX).unwrap();
        for _ in 0..9 {
            additive_overflow.push(0xff).unwrap();
        }
        assert!(additive_overflow.push(1).is_err());
    }

    #[test]
    fn bounded_nonminimal_encodings_are_not_silently_rewritten() {
        let mut integer = PrefixedInteger::start(31, 5, 31).unwrap();
        integer.push(0x80).unwrap();
        assert_eq!(integer.push(0).unwrap(), Some(31));
        assert_eq!(format!("{integer:?}"), "PrefixedInteger([redacted])");
        assert_eq!(integer.finish().unwrap(), 31);
    }
}
