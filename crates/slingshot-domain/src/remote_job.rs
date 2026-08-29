//! Durable remote-job vocabulary.
//!
//! The module map assigns this module the agent job identifier, the agent
//! job state, the job event sequence, and the event stream cursor. Storage
//! persists these domain values and the wire protocols convert to them, so
//! the vocabulary stays inward of both. This commit declares the module
//! alone.
