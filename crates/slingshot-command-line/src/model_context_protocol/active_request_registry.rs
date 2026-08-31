//! Which requests are in flight, and what each of them holds.
//!
//! One request is one entry: the identifier the client sent, the operation key it
//! was admitted under, and whatever a reconnect has to be able to find again.
//! Holding that here rather than in the transport is what lets a reconnected
//! client be answered about work it started before the connection dropped.
