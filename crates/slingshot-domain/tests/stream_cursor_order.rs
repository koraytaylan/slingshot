//! Decimal boundaries and total-order laws for persisted cursor comparisons.

use core::cmp::Ordering;
use slingshot_domain::stream_cursor_order::compare;

#[test]
fn decimal_positions_preserve_numeric_order_and_exact_identity() {
    for (earlier, later) in [
        ("0:0", "7:1"),
        ("7:9", "7:10"),
        ("7:99", "7:100"),
        ("7:999999999999999999", "7:1000000000000000000"),
        ("7:18446744073709551614", "7:18446744073709551615"),
        ("9:100", "10:1"),
        ("cursor-009", "cursor-010"),
    ] {
        assert_eq!(compare(earlier, later), Ordering::Less);
        assert_eq!(compare(later, earlier), Ordering::Greater);
        assert_eq!(compare(earlier, earlier), Ordering::Equal);
    }
}

#[test]
fn mixed_and_noncanonical_spellings_preserve_total_order_laws() {
    let cursors = [
        "",
        "0:0",
        "7:9",
        "7:10",
        "07:9",
        "7:09",
        "+7:9",
        "7:18446744073709551616",
        "7:9:1",
        "cursor-009",
        "cursor-010",
        "z",
    ];
    for left in cursors {
        for middle in cursors {
            assert_eq!(compare(left, middle).reverse(), compare(middle, left));
            assert_eq!(compare(left, middle) == Ordering::Equal, left == middle);
            for right in cursors {
                if compare(left, middle).is_le() && compare(middle, right).is_le() {
                    assert!(compare(left, right).is_le());
                }
            }
        }
    }
}
