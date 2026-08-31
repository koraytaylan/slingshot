//! Reading and writing framed messages on the standard streams.
//!
//! Bounded, and byte-clean. A message larger than the transport admits is refused
//! rather than partially read, and nothing but a message is ever written.
