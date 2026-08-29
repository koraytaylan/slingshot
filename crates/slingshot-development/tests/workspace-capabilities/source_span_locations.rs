//! Probe for the source-span-locations capability.
//!
//! Requires the line and column of a parsed element outside a macro expansion,
//! so a policy diagnostic can point at the exact declaration it rejects.

use std::str::FromStr;

use proc_macro2::TokenStream;

#[test]
fn a_parsed_declaration_reports_its_line_and_column() {
    let source = "fn first() {}\nfn second_declaration() {}\n";
    let parsed = syn::parse_file(source).expect("the source parses");
    let second = parsed.items.get(1).expect("the second item exists");
    let syn::Item::Fn(function) = second else { panic!("the second item is a function") };
    let start = syn::spanned::Spanned::span(&function.sig.ident).start();
    assert_eq!(start.line, 2, "the declaration is on the second line");
    assert_eq!(start.column, 3, "the declaration follows the keyword");

    let stream = TokenStream::from_str(source).expect("the source tokenizes");
    let first = stream.into_iter().next().expect("the stream has a token");
    assert_eq!(first.span().start().line, 1);
}
