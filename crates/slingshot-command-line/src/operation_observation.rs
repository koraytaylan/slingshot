//! Watching one operation without holding it open.
//!
//! The module map assigns this leaf the polling and streaming a caller sees
//! while work runs. Observation is a read: nothing here changes an operation,
//! so a caller that stops watching changes nothing about what is running.
