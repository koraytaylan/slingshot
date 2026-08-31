//! Content-addressed artifacts, proved on the bytes and on the handle.
//!
//! Two properties run through the whole suite. The first is that identity never
//! comes from a name: the same five fields derive the same artifact identifier
//! and any different field derives another, including two author targets that
//! differ only by the opaque principal behind them. The second is that a
//! successful read is a statement about bytes that were actually streamed
//! through one verified handle, not about what a path pointed at when someone
//! last looked.
//!
//! The byte fixtures are files rather than generated values, so the digests
//! this suite compares against were computed once, outside the implementation,
//! and cannot drift with it.

mod association;
mod fixtures;
mod identity;
mod installation;
mod verification;
