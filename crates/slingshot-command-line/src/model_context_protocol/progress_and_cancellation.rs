//! What a client is told while work runs, and what asking it to stop means.
//!
//! Progress is a report about durable state rather than a stream this server
//! invents, and a cancellation ends a client's interest rather than the work.
