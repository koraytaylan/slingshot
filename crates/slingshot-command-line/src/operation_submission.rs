//! Handing one built request to the daemon and learning its identity.
//!
//! The module map assigns this leaf the submission call and the operation
//! identifier it comes back with. Submission is separated from observation
//! because a caller may submit and leave, and the identifier is what makes
//! coming back possible.
