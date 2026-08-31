//! Pages of rows that are not anchored anywhere in the repository.
//!
//! A bundle, a component, a mapping entry, a workflow model, a job queue, a
//! replication agent, and a group member are all things an author can list and
//! none of them has a repository path to be ordered by. Plan 0003 already
//! decided what a page of results is - a bounded window, an opaque resumption
//! token, a strict order nobody may reinterpret - and this leaf reuses that
//! decision for rows keyed by text rather than replacing it with a second one.
//!
//! Order is over bytes, not over anything a locale would decide. Two deployments
//! that disagree about collation would otherwise resume a listing in different
//! places, and a resumption that lands in a different place is a listing that
//! silently skips rows.
//!
//! The ascending-distinct rule is here too, because a requested set of states is
//! the same rule applied to a request rather than to an answer: a caller that
//! asks for `active` twice, or asks in an order the wire would rewrite, is
//! sending a document whose bytes nobody can reproduce.

/// Why a listing result is not one this contract can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ListingResultFailure {
    /// Two rows carry the same key, or a later one sorts before an earlier.
    #[error("listing rows are strictly ascending by their key bytes")]
    NotStrictlyAscending,
    /// A result echoes a request other than the one it answers.
    #[error("a listing result echoes the request it answers")]
    NotThisRequest,
    /// A requested set is empty, repeats a member, or is out of order.
    #[error("a requested set is nonempty, distinct, and ascending")]
    NotAscendingDistinct,
    /// A requested set names more members than the contract allows.
    #[error("a requested set is within the bound its contract declares")]
    TooManyRequested,
}

/// Requires every key to sort strictly after the one before it.
///
/// # Errors
///
/// Returns [`ListingResultFailure::NotStrictlyAscending`] when a key repeats or
/// sorts before its predecessor.
pub fn require_strictly_ascending_text<'key>(
    keys: impl IntoIterator<Item = &'key str>,
) -> Result<(), ListingResultFailure> {
    let mut previous: Option<&str> = None;
    for key in keys {
        if let Some(earlier) = previous
            && earlier.as_bytes() >= key.as_bytes()
        {
            return Err(ListingResultFailure::NotStrictlyAscending);
        }
        previous = Some(key);
    }
    Ok(())
}

/// Requires a requested set to be nonempty, ascending, distinct, and bounded.
///
/// Ascending rather than sorted on the caller's behalf: sorting would accept two
/// documents that mean the same thing and serialize differently, and the byte
/// contract this family is held to has no room for two.
///
/// # Errors
///
/// Returns [`ListingResultFailure::NotAscendingDistinct`] when the set is empty,
/// repeats a member, or is out of order, and
/// [`ListingResultFailure::TooManyRequested`] when it names more members than
/// `bound` allows.
pub fn require_ascending_distinct<Member: Ord>(
    members: &[Member],
    bound: u64,
) -> Result<(), ListingResultFailure> {
    if u64::try_from(members.len()).unwrap_or(u64::MAX) > bound {
        return Err(ListingResultFailure::TooManyRequested);
    }
    if members.is_empty() {
        return Err(ListingResultFailure::NotAscendingDistinct);
    }
    if members.windows(ADJACENT_PAIR).any(|pair| pair[0] >= pair[1]) {
        return Err(ListingResultFailure::NotAscendingDistinct);
    }
    Ok(())
}

/// Members one adjacency comparison looks at.
const ADJACENT_PAIR: usize = 2;

/// Declares one nonempty ascending set of a closed state, over its own bound.
///
/// Four families ask "which states do you mean" and each would otherwise write
/// the same wrapper: the same validation, the same accessors, the same
/// deserializer that validates before a value exists. The shape is written once
/// here and each family supplies its member type and the limit that bounds it.
macro_rules! requested_states {
    ($(#[$attribute:meta])* $name:ident, $member:ty, $limit:literal) => {
        $(#[$attribute])*
        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
        #[serde(transparent)]
        pub struct $name {
            /// The states, ascending and distinct.
            states: Vec<$member>,
        }

        impl $name {
            /// Returns the set these states describe.
            ///
            /// # Errors
            ///
            /// Returns
            /// [`ListingResultFailure::NotAscendingDistinct`][not-ascending]
            /// when the set is empty, repeats a member, or is out of order, and
            /// [`ListingResultFailure::TooManyRequested`][too-many] when it
            /// names more members than the contract allows.
            ///
            /// [not-ascending]: crate::command::operational_listing::ListingResultFailure::NotAscendingDistinct
            /// [too-many]: crate::command::operational_listing::ListingResultFailure::TooManyRequested
            pub fn new(
                states: Vec<$member>,
            ) -> Result<Self, $crate::command::operational_listing::ListingResultFailure> {
                let bound = $crate::command::command_identity::CommandContract::embedded()
                    .limit($limit);
                $crate::command::operational_listing::require_ascending_distinct(&states, bound)?;
                Ok(Self { states })
            }

            /// Returns the states, ascending.
            #[must_use]
            pub fn states(&self) -> &[$member] {
                &self.states
            }

            /// Reports whether this set asks about `state`.
            #[must_use]
            pub fn contains(&self, state: $member) -> bool {
                self.states.contains(&state)
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<Source: serde::Deserializer<'de>>(
                deserializer: Source,
            ) -> Result<Self, Source::Error> {
                use serde::de::Error as _;
                Self::new(Vec::<$member>::deserialize(deserializer)?).map_err(Source::Error::custom)
            }
        }
    };
}

pub(crate) use requested_states;
