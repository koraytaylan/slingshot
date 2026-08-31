//! Making sure two commands do not stage one artifact at once.
//!
//! The module map assigns this leaf the exclusion around a staging file. Two
//! processes writing one partial download would produce a file that is neither
//! of them, and the failure would look like corruption rather than a race.
