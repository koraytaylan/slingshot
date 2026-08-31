//! Reading a property document a caller supplies for a mutation.
//!
//! The module map assigns this leaf the bounded parse of the properties a
//! create or add command applies. It is separate from the commands that use it
//! because both apply the same document shape, and two parsers would eventually
//! disagree about one of them.
