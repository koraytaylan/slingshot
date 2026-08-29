//! Supervised child disposition.
//!
//! The module map assigns this module the private supervision channel that
//! retains an unreaped child or native process handle until one atomic
//! exit-or-terminate-and-wait disposition, so no cleanup path signals a
//! numeric process identifier. This commit declares the module alone.
