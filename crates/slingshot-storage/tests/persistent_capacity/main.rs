//! What a namespace may keep, proved by arithmetic and by counting rows.
//!
//! Two halves. The first compares the manifest's formulas against vectors whose
//! operands and results were computed outside this workspace entirely, so
//! agreement is evidence rather than the implementation agreeing with itself.
//! The second exercises the accounting against real rows: at the bound, one
//! past it, concurrently, and across a reopen.
//!
//! Nothing here ever asserts that reaching a limit deletes something, because
//! reaching a limit never does. A namespace at its bound refuses, says what it
//! is holding, and keeps every byte it already has.

mod accounting;
mod arithmetic;
mod fixtures;
