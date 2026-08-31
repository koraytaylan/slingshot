//! Two asset searches, and the set-valued filters that make them awkward.
//!
//! Media formats and tags are sets on the wire, canonical and ascending,
//! because the set participates in the digest a continuation token is bound to:
//! two spellings of one set would be two queries that could not resume each
//! other. So every accepted permutation of the same values produces the same
//! request, and the suite checks that by permuting them rather than by trusting
//! that something sorts.
//!
//! A repeat is a different matter. Collapsing it would accept a set the caller
//! did not describe, so it is refused rather than deduplicated.
//!
//! The byte-length grammar is canonical unsigned base ten and nothing else. A
//! sign, a leading zero, a fraction, or an exponent is a spelling the
//! repository could never report back, so a request carrying one could never be
//! answered and is refused here. Both endpoints are checked as lengths before
//! the range is checked as a range, because a minimum that is not a length and
//! a minimum above a maximum are different mistakes to fix.

use slingshot_command_line::commands::asset_query::{
    FIND_BY_METADATA, FIND_REFERENCED_BY_PAGE, build, is_canonical_unsigned,
};
use slingshot_command_line::commands::content::RequestRefusal;
use slingshot_command_line::invocation::{
    CONTINUATION_TOKEN_OPTION, Invocation, LIMIT_OPTION, MATCH_ALL_OPTION, MAXIMUM_BYTES_OPTION,
    MEDIA_FORMATS_OPTION, MINIMUM_BYTES_OPTION, OFFSET_OPTION, PATH_OPTION, TAGS_OPTION, parse,
};
use slingshot_domain::command::catalog::{AccessClassification, Command, CommandCatalog};
use slingshot_domain::command::find_assets_by_metadata::TagMatchMode;

/// A repository path these fixtures search under.
const ROOT: &str = "/content/dam/site";

/// The page whose references one search reads.
const PAGE: &str = "/content/site/en/home";

/// Two media formats, in ascending order.
const ASCENDING_FORMATS: &str = "image/jpeg,image/png";

/// The same two, permuted.
const PERMUTED_FORMATS: &str = "image/png,image/jpeg";

/// The same format twice.
const REPEATED_FORMATS: &str = "image/png,image/png";

/// Two tags, ascending.
const TAGS: &str = "site:brand,site:campaign";

/// The largest length a repository reports.
const LARGEST_LENGTH: &str = "9223372036854775807";

/// One past it.
const BEYOND_LARGEST: &str = "9223372036854775808";

/// One length, smaller than the other.
const SMALLER_LENGTH: &str = "10";

/// One length, larger than the other.
const LARGER_LENGTH: &str = "4096";

/// Spellings the canonical grammar refuses.
const REFUSED_SPELLINGS: &[&str] =
    &["+1", "01", "1.0", "1e3", "-1", "", " 1", "1 ", "0x10", "1_000"];

/// Returns the invocation `words` parse into.
fn invocation(words: &[&str]) -> Invocation {
    parse(&words.iter().map(|word| (*word).to_owned()).collect::<Vec<String>>())
        .expect("the words parse")
}

/// Returns the metadata search one set of formats produces.
fn with_formats(formats: &str) -> Result<Command, RequestRefusal> {
    build(&invocation(&[FIND_BY_METADATA, PATH_OPTION, ROOT, MEDIA_FORMATS_OPTION, formats]))
}

#[test]
fn every_permutation_of_one_set_produces_one_request() {
    let ascending = with_formats(ASCENDING_FORMATS).expect("two formats");
    let permuted = with_formats(PERMUTED_FORMATS).expect("the same two, typed the other way");
    assert_eq!(
        ascending, permuted,
        "the set is in the digest a continuation token is bound to, so two spellings would be \
         two queries that could not resume each other"
    );
    let Command::FindAssetsByMetadata(request) = ascending else { panic!("one variant") };
    let stated: Vec<&str> = request
        .media_formats
        .as_ref()
        .expect("two were named")
        .values()
        .iter()
        .map(|format| format.as_text())
        .collect();
    assert_eq!(stated, vec!["image/jpeg", "image/png"], "and the request is the ascending one");
}

#[test]
fn a_repeated_member_is_refused_rather_than_collapsed() {
    assert_eq!(
        with_formats(REPEATED_FORMATS),
        Err(RequestRefusal::ValueUnusable { named: MEDIA_FORMATS_OPTION.to_owned() }),
        "collapsing it would accept a set the caller did not describe"
    );
}

#[test]
fn tags_carry_a_match_mode_only_when_tags_were_named() {
    let bare =
        build(&invocation(&[FIND_BY_METADATA, PATH_OPTION, ROOT])).expect("a root is enough");
    let Command::FindAssetsByMetadata(request) = bare else { panic!("one variant") };
    assert_eq!(request.tags, None);
    assert_eq!(
        request.tag_match_mode, None,
        "a match mode without tags would be an answer to a question nobody asked"
    );

    let any = build(&invocation(&[FIND_BY_METADATA, PATH_OPTION, ROOT, TAGS_OPTION, TAGS]))
        .expect("two tags");
    let Command::FindAssetsByMetadata(request) = any else { panic!("one variant") };
    assert_eq!(request.tag_match_mode, Some(TagMatchMode::Any));

    let all = build(&invocation(&[
        FIND_BY_METADATA,
        PATH_OPTION,
        ROOT,
        TAGS_OPTION,
        TAGS,
        MATCH_ALL_OPTION,
    ]))
    .expect("two tags and a mode");
    let Command::FindAssetsByMetadata(request) = all else { panic!("one variant") };
    assert_eq!(request.tag_match_mode, Some(TagMatchMode::All));
}

#[test]
fn the_byte_grammar_is_canonical_unsigned_base_ten_and_nothing_else() {
    for spelling in REFUSED_SPELLINGS {
        assert!(
            !is_canonical_unsigned(spelling),
            "{spelling:?} is a spelling the repository could never report back"
        );
        assert_eq!(
            build(&invocation(&[
                FIND_BY_METADATA,
                PATH_OPTION,
                ROOT,
                MINIMUM_BYTES_OPTION,
                spelling
            ])),
            Err(RequestRefusal::ValueUnusable { named: MINIMUM_BYTES_OPTION.to_owned() }),
            "{spelling:?}"
        );
    }
    for spelling in ["0", "1", SMALLER_LENGTH, LARGER_LENGTH, LARGEST_LENGTH] {
        assert!(is_canonical_unsigned(spelling), "{spelling} is canonical");
    }
}

#[test]
fn a_length_beyond_what_a_repository_reports_is_refused() {
    assert!(
        is_canonical_unsigned(BEYOND_LARGEST),
        "the spelling is canonical, so the refusal is about the domain rather than the syntax"
    );
    assert_eq!(
        build(&invocation(&[
            FIND_BY_METADATA,
            PATH_OPTION,
            ROOT,
            MAXIMUM_BYTES_OPTION,
            BEYOND_LARGEST
        ])),
        Err(RequestRefusal::ValueUnusable { named: MAXIMUM_BYTES_OPTION.to_owned() })
    );
    let largest = build(&invocation(&[
        FIND_BY_METADATA,
        PATH_OPTION,
        ROOT,
        MAXIMUM_BYTES_OPTION,
        LARGEST_LENGTH,
    ]))
    .expect("the largest length a repository reports is a length");
    let Command::FindAssetsByMetadata(request) = largest else { panic!("one variant") };
    assert!(request.maximum_byte_length.is_some());
}

#[test]
fn an_inverted_range_is_refused_only_after_both_ends_are_lengths() {
    let inverted = build(&invocation(&[
        FIND_BY_METADATA,
        PATH_OPTION,
        ROOT,
        MINIMUM_BYTES_OPTION,
        LARGER_LENGTH,
        MAXIMUM_BYTES_OPTION,
        SMALLER_LENGTH,
    ]))
    .expect_err("a minimum above a maximum matches nothing");
    assert_eq!(inverted, RequestRefusal::ValueUnusable { named: MINIMUM_BYTES_OPTION.to_owned() });

    let malformed = build(&invocation(&[
        FIND_BY_METADATA,
        PATH_OPTION,
        ROOT,
        MINIMUM_BYTES_OPTION,
        "1.0",
        MAXIMUM_BYTES_OPTION,
        SMALLER_LENGTH,
    ]))
    .expect_err("that is not a length at all");
    assert_eq!(
        malformed,
        RequestRefusal::ValueUnusable { named: MINIMUM_BYTES_OPTION.to_owned() },
        "and a value that is not a length is a different mistake from a range that runs down"
    );

    let equal = build(&invocation(&[
        FIND_BY_METADATA,
        PATH_OPTION,
        ROOT,
        MINIMUM_BYTES_OPTION,
        SMALLER_LENGTH,
        MAXIMUM_BYTES_OPTION,
        SMALLER_LENGTH,
    ]))
    .expect("both ends are inclusive, so one length is a range of one");
    assert!(matches!(equal, Command::FindAssetsByMetadata(_)));
}

#[test]
fn the_page_reference_search_takes_a_page_and_a_window_and_nothing_else() {
    let built = build(&invocation(&[
        FIND_REFERENCED_BY_PAGE,
        PATH_OPTION,
        PAGE,
        OFFSET_OPTION,
        "0",
        LIMIT_OPTION,
        "25",
    ]))
    .expect("a page and a window");
    let Command::FindAssetsReferencedByPage(request) = built else { panic!("one variant") };
    assert_eq!(request.page_path.as_text(), PAGE);
    assert!(request.result_window.is_some());
}

#[test]
fn both_searches_share_the_window_rule_the_other_discovery_commands_use() {
    for leaf in [FIND_BY_METADATA, FIND_REFERENCED_BY_PAGE] {
        let refusal = build(&invocation(&[
            leaf,
            PATH_OPTION,
            ROOT,
            OFFSET_OPTION,
            "0",
            LIMIT_OPTION,
            "10",
            CONTINUATION_TOKEN_OPTION,
            "opaque-token",
        ]))
        .expect_err("a token already carries the window it was issued under");
        assert_eq!(
            refusal,
            RequestRefusal::ValueUnusable { named: CONTINUATION_TOKEN_OPTION.to_owned() },
            "{leaf}"
        );
    }
}

#[test]
fn both_searches_are_read_only_and_offer_no_publisher_option() {
    let catalog = CommandCatalog::published();
    for leaf in [FIND_BY_METADATA, FIND_REFERENCED_BY_PAGE] {
        let descriptor = catalog.find(leaf).expect("the registry publishes it");
        assert_eq!(descriptor.access, AccessClassification::Read, "{leaf} changes nothing");
        assert!(!descriptor.intrinsic_idempotency.requires_operation_key(), "{leaf} needs no key");
    }
    let source = std::fs::read_to_string("src/commands/asset_query.rs").expect("it is readable");
    for absent in ["--publisher", "--tier", "publish", "dispatcher"] {
        assert!(
            !source.contains(absent),
            "an asset search reaches the author, and offering {absent} would suggest otherwise"
        );
    }
}
