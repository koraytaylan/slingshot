//! Probe for the Rust source-syntax capability.
//!
//! Requires parsing a whole source file into a syntax tree, visiting it to
//! classify an unchecked block by syntax rather than by token text, and
//! reporting a parse failure instead of accepting malformed input.

use syn::visit::Visit;

/// Counts unchecked blocks and functions by syntax classification.
#[derive(Default)]
struct UncheckedCounter {
    blocks: usize,
    functions: usize,
}

impl<'ast> Visit<'ast> for UncheckedCounter {
    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        self.blocks += 1;
        syn::visit::visit_expr_unsafe(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if matches!(node.sig.safety, syn::Safety::Unsafe(_)) {
            self.functions += 1;
        }
        syn::visit::visit_item_fn(self, node);
    }
}

#[test]
fn a_source_file_is_classified_by_syntax_rather_than_by_token_text() {
    let source = r#"
        //! A module that mentions the word unsafe only in prose.
        const NOTE: &str = "unsafe";

        fn checked() -> usize {
            NOTE.len()
        }

        unsafe fn unchecked_function() {}

        fn holds_an_unchecked_block() {
            unsafe {
                unchecked_function();
            }
        }
    "#;
    let parsed = syn::parse_file(source).expect("the source file parses");
    let mut counter = UncheckedCounter::default();
    counter.visit_file(&parsed);
    assert_eq!(counter.functions, 1, "one unchecked function is declared");
    assert_eq!(counter.blocks, 1, "one unchecked block is written");
    assert_eq!(parsed.items.len(), 4, "the file declares four items");

    let malformed = syn::parse_file("fn broken( {").expect_err("malformed source is refused");
    assert!(!malformed.to_string().is_empty(), "the failure explains itself");
}
