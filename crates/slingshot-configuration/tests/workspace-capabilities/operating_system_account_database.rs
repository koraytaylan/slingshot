//! Probe for the operating-system-account-database capability.
//!
//! Requires reading the account-database entry of an effective user identifier
//! and obtaining that account's absolute home directory, without consulting an
//! environment variable that a caller could set.

use uzers::os::unix::UserExt;

#[test]
fn the_account_database_answers_for_the_effective_user() {
    let identifier = uzers::get_effective_uid();
    let account = uzers::get_user_by_uid(identifier).expect("the effective user has an account");
    assert_eq!(account.uid(), identifier);
    let home = account.home_dir();
    assert!(home.is_absolute(), "{} is not absolute", home.display());
}
