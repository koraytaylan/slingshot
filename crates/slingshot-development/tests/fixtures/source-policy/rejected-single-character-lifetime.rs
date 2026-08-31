//! A lifetime that is one letter.

/// One borrowed row.
#[derive(Debug)]
pub struct Row<'a> {
    /// What it borrows.
    pub held: &'a str,
}
