//! What one command invocation is, before anything acts on it.
//!
//! The module map assigns this leaf the parsed shape of a command line: the
//! verb, its arguments, and the options that apply to every verb. Parsing is
//! separated from execution because an invocation that cannot be built is a
//! usage problem, and one that can is a request nothing has refused yet.
