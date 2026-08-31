//! Writing an outcome for a person reading a terminal.
//!
//! The module map assigns this leaf the human rendering of every closed result
//! and failure. It is separate from the machine rendering because the two have
//! different obligations: this one may summarize, and the other may not.
