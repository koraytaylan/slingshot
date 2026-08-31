//! What happens when a caller stops waiting.
//!
//! The module map assigns this leaf the handling of an interrupt while an
//! operation is being watched. Stopping watching is not stopping work, and the
//! distinction is the whole point: the operation keeps its identifier and the
//! caller can come back to it.
