//! Deciding which profile and environment a command speaks to.
//!
//! The module map assigns this leaf the resolution from what a caller named to
//! the one selected target a daemon serves. It is a decision made once per
//! invocation, because a command that resolved its target twice could reach two
//! different authors from one line.
