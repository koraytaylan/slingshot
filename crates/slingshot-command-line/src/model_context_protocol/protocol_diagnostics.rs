//! Where this server says anything that is not a protocol message.
//!
//! Standard output carries protocol messages and nothing else, whatever happens
//! to the diagnostic stream, because a diagnostic on standard output corrupts
//! every client parsing it.
