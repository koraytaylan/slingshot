//! Admission, replay, conflict, transition, and recovery, proved on committed rows.
//!
//! The property under test throughout is that idempotency is a fact about the
//! database rather than about anything a caller remembers. So every assertion
//! either races two connections at one row, or reopens the file and asks again.
//! A suite that only ever spoke to one live repository would prove that the code
//! is self-consistent, which is not the question.
//!
//! Two identities are deliberately close together in the fixtures: the same
//! deployment reached through two opaque authentication principals. They digest
//! differently, so they are two partitions, and no replay may cross between
//! them. That pair is the one an implementation is most likely to get wrong,
//! because everything else about the two requests is identical.

mod admission;
mod fixtures;
mod lifecycle;
mod recovery;
