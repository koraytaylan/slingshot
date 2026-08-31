//! How large an answer may be before it is published rather than inlined.
//!
//! The bound belongs in one place: an answer that is too large is externalized
//! with an address rather than truncated, because a truncated answer is not a
//! smaller one.
