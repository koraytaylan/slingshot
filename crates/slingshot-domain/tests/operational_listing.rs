//! Assertions for pages and requested sets that are keyed by text.
//!
//! Byte order is the whole point of the ordering assertion. A pair that differs
//! only after a multi-byte scalar orders one way by bytes and could order the
//! other way under a collation, and a listing that resumed under a collation
//! would skip whatever fell between the two.

use slingshot_domain::command::operational_listing::{
    ListingResultFailure, require_ascending_distinct, require_strictly_ascending_text,
};

/// A bound large enough that the item rule is what refuses a set.
const ROOMY: u64 = 8;

#[test]
fn an_empty_page_and_a_single_row_are_both_pages() {
    assert_eq!(require_strictly_ascending_text(Vec::<&str>::new()), Ok(()));
    assert_eq!(require_strictly_ascending_text(vec!["only"]), Ok(()));
}

#[test]
fn rows_are_strictly_ascending_and_a_repeat_is_not_ascending() {
    assert_eq!(require_strictly_ascending_text(vec!["alpha", "beta", "gamma"]), Ok(()));
    assert_eq!(
        require_strictly_ascending_text(vec!["alpha", "alpha"]),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
    assert_eq!(
        require_strictly_ascending_text(vec!["beta", "alpha"]),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
}

#[test]
fn the_order_is_over_bytes_rather_than_over_anything_a_locale_decides() {
    // "Z" sorts after "A" by byte and before "a"; a collation would disagree.
    assert_eq!(require_strictly_ascending_text(vec!["Z", "a"]), Ok(()));
    assert_eq!(
        require_strictly_ascending_text(vec!["a", "Z"]),
        Err(ListingResultFailure::NotStrictlyAscending)
    );
    // A scalar outside ASCII orders by its encoded bytes, so "é" follows "z".
    assert_eq!(require_strictly_ascending_text(vec!["z", "\u{e9}"]), Ok(()));
}

#[test]
fn a_requested_set_is_nonempty_distinct_and_ascending() {
    assert_eq!(require_ascending_distinct(&["active", "resolved"], ROOMY), Ok(()));
    assert_eq!(
        require_ascending_distinct::<&str>(&[], ROOMY),
        Err(ListingResultFailure::NotAscendingDistinct)
    );
    assert_eq!(
        require_ascending_distinct(&["active", "active"], ROOMY),
        Err(ListingResultFailure::NotAscendingDistinct)
    );
    assert_eq!(
        require_ascending_distinct(&["resolved", "active"], ROOMY),
        Err(ListingResultFailure::NotAscendingDistinct)
    );
}

#[test]
fn a_requested_set_is_accepted_at_its_bound_and_refused_one_member_past_it() {
    let bound = usize::try_from(ROOMY).expect("the bound fits");
    let exact: Vec<String> = (0..bound).map(|index| format!("state-{index}")).collect();
    assert_eq!(require_ascending_distinct(&exact, ROOMY), Ok(()));
    let mut beyond = exact;
    beyond.push(format!("state-{bound}"));
    assert_eq!(
        require_ascending_distinct(&beyond, ROOMY),
        Err(ListingResultFailure::TooManyRequested)
    );
}

#[test]
fn the_item_bound_is_checked_before_the_order_is() {
    let bound = usize::try_from(ROOMY).expect("the bound fits");
    let unordered: Vec<String> =
        (0..=bound).map(|index| format!("state-{}", bound - index)).collect();
    assert_eq!(
        require_ascending_distinct(&unordered, ROOMY),
        Err(ListingResultFailure::TooManyRequested),
        "an oversized set was refused for its order instead of its size"
    );
}
