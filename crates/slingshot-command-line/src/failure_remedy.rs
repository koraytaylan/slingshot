//! What a caller can do next about one failure category.
//!
//! A terminal failure carries its category and nothing else, and a category
//! says what went wrong rather than what to try. A caller that reads
//! `root_not_found` and nothing more stops, when the anchor it guessed is
//! usually one listing away from the one that exists. The remedy is written
//! beside the failure so that the next step is in the answer itself.
//!
//! Only categories with a step the caller can take carry one. A category the
//! table does not name answers nothing, so a remedy is never invented for a
//! failure nobody has thought about.

/// Every category that has a remedy, with the remedy, in ascending order.
pub const REMEDIES: &[(&str, &str)] = &[
    (
        "discovery_budget_exceeded",
        "the search would examine more nodes than one call may; start it from a deeper \
         root_path, or name a primary_node_type and property predicates so fewer nodes \
         qualify, or walk the tree one level at a time with list_child_pages",
    ),
    (
        "root_access_denied",
        "the caller's account may not read root_path; anchor the call at a path it can read, \
         or use a credential whose account is in a group that can",
    ),
    (
        "root_not_found",
        "nothing this caller can read is at root_path, so the name is probably not the one the \
         repository uses; list the children of its parent with list_child_pages or \
         list_child_nodes and continue from the child that matches - a site often nests a \
         locale as /<country>/<language> rather than as one <language>-<country> segment",
    ),
];

/// Returns the remedy for `category`, when it has one.
#[must_use]
pub fn remedy(category: &str) -> Option<&'static str> {
    REMEDIES.iter().find(|(named, _)| *named == category).map(|(_, remedy)| *remedy)
}
