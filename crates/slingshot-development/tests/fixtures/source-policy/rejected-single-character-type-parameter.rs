//! A generic parameter that is one letter.

/// Holds one value of whatever kind.
#[derive(Debug)]
pub struct Holder<T> {
    /// The value.
    pub held: T,
}
