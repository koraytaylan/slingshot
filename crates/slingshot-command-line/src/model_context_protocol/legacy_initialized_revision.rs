//! Answering clients that negotiate the older initialized revision.
//!
//! An older client establishes a session first and speaks a different set of
//! shapes afterwards. Keeping that apart from the current revision is what stops
//! one era's rules from quietly deciding the other's behaviour.
