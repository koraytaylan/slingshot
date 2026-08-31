//! Writing an outcome for something that will parse it.
//!
//! The module map assigns this leaf the byte-stable rendering of every closed
//! result and failure. Byte-stable is the requirement rather than merely valid:
//! a consumer diffing two runs must see a difference only where one exists.
