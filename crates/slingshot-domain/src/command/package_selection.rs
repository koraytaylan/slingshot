//! The package selection language, and the quoting that carries it into
//! FileVault.
//!
//! # Matching without a regular-expression engine
//!
//! An expression is a sequence of tokens, each of which is a literal path
//! segment, `*` for exactly one segment, or `(.*)` for zero or more. Nothing
//! else is special: a literal segment containing a dot, a plus, or a bracket is
//! compared byte for byte, because a caller writing a path should not have to
//! know which characters some engine would have treated as syntax.
//!
//! Matching is a bottom-up table with exactly `(tokens + 1) x (segments + 1)`
//! cells, each filled once. There is no recursion, no backtracking, and no
//! engine, so the cost of one expression against one candidate is a number that
//! can be computed in advance and charged - which is what makes the work
//! bounded rather than merely usually fast.
//!
//! # Quoting without trusting a platform
//!
//! FileVault filters carry Java regular expressions, so a path has to be
//! quoted before it becomes one. The quoting is written out here rather than
//! delegated: a literal `\E` inside the path would otherwise close the quoted
//! region early and turn the rest of the path into syntax, so each occurrence
//! is broken up explicitly. The result is then XML-attribute escaped in one
//! pass, and the bytes that escaping produces are never scanned again.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::command_identity::CommandContract;
use crate::command::repository_path::{RepositoryPath, RepositoryPathSegment};

/// Separator between two tokens of an expression.
pub const TOKEN_SEPARATOR: char = '/';

/// Token matching exactly one complete segment.
pub const SINGLE_SEGMENT_TOKEN: &str = "*";

/// Token matching zero or more complete segments.
pub const ANY_SEGMENTS_TOKEN: &str = "(.*)";

/// Opening of the quoted region a Java regular expression uses.
pub const QUOTE_OPENING: &str = r"\Q";

/// Closing of that region.
pub const QUOTE_CLOSING: &str = r"\E";

/// What a node include's expression is anchored with.
pub const NODE_INCLUDE_PREFIX: &str = r"\A\Q";

/// What it ends with.
pub const NODE_INCLUDE_SUFFIX: &str = r"\E\z";

/// What a direct-property exclude ends with.
///
/// One segment after the quoted path and nothing further, so the rule reaches
/// the ancestor's own properties and not its children's.
pub const PROPERTY_EXCLUDE_SUFFIX: &str = r"\E/(?:[^/]+)\z";

/// Expression an empty selection emits, which matches nothing at all.
pub const NEVER_MATCH_EXPRESSION: &str = r"\A(?!)\z";

/// Returns the most tokens one expression may carry.
#[must_use]
pub fn maximum_package_selection_expression_tokens() -> u64 {
    CommandContract::embedded().limit("maximum_package_selection_expression_tokens")
}

/// Returns the largest expression this contract accepts.
#[must_use]
pub fn maximum_package_selection_expression_bytes() -> u64 {
    CommandContract::embedded().limit("maximum_package_selection_expression_bytes")
}

/// Returns the most matcher cells one job may fill.
#[must_use]
pub fn maximum_package_matcher_cells() -> u64 {
    CommandContract::embedded().limit("maximum_package_matcher_cells")
}

/// Reason a selection value could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SelectionFailure {
    /// An expression is not an absolute slash-separated token sequence.
    #[error("a selection expression begins with one solidus and separates its tokens with one")]
    ExpressionNotAbsolute,
    /// A token is neither a literal segment nor one of the two wildcards.
    #[error(
        "a selection token is one path segment, exactly {SINGLE_SEGMENT_TOKEN}, or exactly {ANY_SEGMENTS_TOKEN}"
    )]
    TokenNotRecognized,
    /// An expression carries more tokens than the contract allows.
    #[error("a selection expression carries at most {maximum} tokens", maximum = maximum_package_selection_expression_tokens())]
    TooManyTokens,
    /// An expression is longer than the contract allows.
    #[error("a selection expression is at most {maximum} bytes", maximum = maximum_package_selection_expression_bytes())]
    ExpressionTooLong,
    /// Matching would fill more cells than the contract allows.
    #[error("matching one expression against one candidate fills at most {maximum} cells", maximum = maximum_package_matcher_cells())]
    TooManyCells,
    /// A path carries a scalar no XML document can hold.
    #[error("a path carries only scalar values XML 1.0 can represent")]
    NotRepresentableInXml,
}

/// One token of a selection expression.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SelectionToken {
    /// A literal segment, compared byte for byte.
    Literal(RepositoryPathSegment),
    /// Exactly one segment, whatever it is.
    SingleSegment,
    /// Zero or more segments.
    AnySegments,
}

/// One bounded expression selecting repository paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(into = "String")]
pub struct PackagePathSelectionExpression {
    /// The expression, exactly as it arrived.
    value: String,
    /// Its tokens, in order.
    tokens: Vec<SelectionToken>,
}

impl PackagePathSelectionExpression {
    /// Returns the expression `spelling` names.
    ///
    /// The root expression `/` carries no tokens and matches only the root.
    ///
    /// # Errors
    ///
    /// Returns [`SelectionFailure::ExpressionNotAbsolute`] for a relative,
    /// empty, repeated-separator, or trailing-separator spelling,
    /// [`SelectionFailure::TokenNotRecognized`] for a token that is neither a
    /// segment nor one of the two wildcards, and the two bound failures.
    pub fn parse(spelling: &str) -> Result<Self, SelectionFailure> {
        if u64::try_from(spelling.len()).unwrap_or(u64::MAX)
            > maximum_package_selection_expression_bytes()
        {
            return Err(SelectionFailure::ExpressionTooLong);
        }
        let Some(body) = spelling.strip_prefix(TOKEN_SEPARATOR) else {
            return Err(SelectionFailure::ExpressionNotAbsolute);
        };
        if body.is_empty() {
            return Ok(Self { value: spelling.to_owned(), tokens: Vec::new() });
        }
        let spellings: Vec<&str> = body.split(TOKEN_SEPARATOR).collect();
        if spellings.iter().any(|token| token.is_empty()) {
            return Err(SelectionFailure::ExpressionNotAbsolute);
        }
        if u64::try_from(spellings.len()).unwrap_or(u64::MAX)
            > maximum_package_selection_expression_tokens()
        {
            return Err(SelectionFailure::TooManyTokens);
        }
        let tokens =
            spellings.iter().map(|token| read_token(token)).collect::<Result<Vec<_>, _>>()?;
        Ok(Self { value: spelling.to_owned(), tokens })
    }

    /// Returns the expression, exactly as it arrived.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.value
    }

    /// Returns how many tokens this expression carries.
    #[must_use]
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    /// Returns how many cells matching against `candidate` fills.
    ///
    /// Known before any of them is filled, which is what lets the work be
    /// charged rather than merely observed.
    ///
    /// # Errors
    ///
    /// Returns [`SelectionFailure::TooManyCells`] when the product is above the
    /// named bound or would overflow.
    pub fn cell_count(&self, candidate: &RepositoryPath) -> Result<u64, SelectionFailure> {
        /// The terminal row and column every table carries.
        const TERMINAL: u64 = 1;

        let tokens = u64::try_from(self.tokens.len()).unwrap_or(u64::MAX);
        let segments = u64::try_from(candidate.segments().len()).unwrap_or(u64::MAX);
        let cells = tokens
            .checked_add(TERMINAL)
            .and_then(|rows| segments.checked_add(TERMINAL).map(|columns| (rows, columns)))
            .and_then(|(rows, columns)| rows.checked_mul(columns))
            .ok_or(SelectionFailure::TooManyCells)?;
        if cells > maximum_package_matcher_cells() {
            return Err(SelectionFailure::TooManyCells);
        }
        Ok(cells)
    }

    /// Returns whether `candidate` matches this expression completely.
    ///
    /// Anchored at both ends: a match consumes every token and every segment,
    /// so an expression never matches a prefix of a longer path.
    ///
    /// # Errors
    ///
    /// Returns [`SelectionFailure::TooManyCells`] when the table would be
    /// larger than the contract fills.
    pub fn matches(&self, candidate: &RepositoryPath) -> Result<bool, SelectionFailure> {
        self.cell_count(candidate)?;
        let segments = candidate.segments();
        let spellings: Vec<&str> = segments.iter().map(RepositoryPathSegment::as_text).collect();
        Ok(fill_table(&self.tokens, &spellings))
    }
}

/// Reads one token of an expression.
fn read_token(spelling: &str) -> Result<SelectionToken, SelectionFailure> {
    if spelling == SINGLE_SEGMENT_TOKEN {
        return Ok(SelectionToken::SingleSegment);
    }
    if spelling == ANY_SEGMENTS_TOKEN {
        return Ok(SelectionToken::AnySegments);
    }
    if spelling.contains('*') {
        return Err(SelectionFailure::TokenNotRecognized);
    }
    RepositoryPathSegment::parse(spelling)
        .map(SelectionToken::Literal)
        .map_err(|_| SelectionFailure::TokenNotRecognized)
}

/// Fills the matcher table and returns its final cell.
///
/// Reverse row and column order, so every cell a rule reads has already been
/// computed. `reachable[token][segment]` is true when the tokens from `token`
/// onward consume the segments from `segment` onward exactly.
fn fill_table(tokens: &[SelectionToken], segments: &[&str]) -> bool {
    /// The terminal row and column every table carries.
    const TERMINAL: usize = 1;

    let rows = tokens.len() + TERMINAL;
    let columns = segments.len() + TERMINAL;
    let mut reachable = vec![vec![false; columns]; rows];
    reachable[tokens.len()][segments.len()] = true;
    for token in (0..tokens.len()).rev() {
        for segment in (0..columns).rev() {
            reachable[token][segment] = match &tokens[token] {
                SelectionToken::AnySegments => {
                    reachable[token + 1][segment]
                        || (segment < segments.len() && reachable[token][segment + 1])
                }
                SelectionToken::SingleSegment => {
                    segment < segments.len() && reachable[token + 1][segment + 1]
                }
                SelectionToken::Literal(literal) => {
                    segment < segments.len()
                        && segments[segment] == literal.as_text()
                        && reachable[token + 1][segment + 1]
                }
            };
        }
    }
    reachable[0][0]
}

impl From<PackagePathSelectionExpression> for String {
    fn from(expression: PackagePathSelectionExpression) -> Self {
        expression.value
    }
}

impl<'de> Deserialize<'de> for PackagePathSelectionExpression {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let spelling = String::deserialize(deserializer)?;
        Self::parse(&spelling).map_err(Source::Error::custom)
    }
}

/// Returns the Java regular expression that matches exactly `path`.
///
/// # Errors
///
/// Returns [`SelectionFailure::NotRepresentableInXml`] when the path carries a
/// scalar XML 1.0 cannot hold, because the expression is destined for an XML
/// attribute and a document that cannot be written is worse than a refusal.
pub fn quote_node_include(path: &RepositoryPath) -> Result<String, SelectionFailure> {
    require_representable(path.as_text())?;
    Ok(format!("{NODE_INCLUDE_PREFIX}{}{NODE_INCLUDE_SUFFIX}", break_quoted_region(path.as_text())))
}

/// Returns the expression matching exactly `path`'s own direct properties.
///
/// # Errors
///
/// Returns [`SelectionFailure::NotRepresentableInXml`] on the same ground.
pub fn quote_property_exclude(path: &RepositoryPath) -> Result<String, SelectionFailure> {
    require_representable(path.as_text())?;
    Ok(format!(
        "{NODE_INCLUDE_PREFIX}{}{PROPERTY_EXCLUDE_SUFFIX}",
        break_quoted_region(path.as_text())
    ))
}

/// Breaks every literal quote-closing sequence inside `path`.
///
/// A literal `\E` in the path would close the quoted region early and let the
/// rest of the path be read as syntax. Each occurrence becomes a closing, an
/// escaped `\E`, and a fresh opening, so the sequence survives as two literal
/// characters and the region continues.
fn break_quoted_region(path: &str) -> String {
    path.replace(QUOTE_CLOSING, &format!("{QUOTE_CLOSING}\\\\E{QUOTE_OPENING}"))
}

/// Requires every scalar to be one XML 1.0 can hold.
fn require_representable(path: &str) -> Result<(), SelectionFailure> {
    /// First scalar of the printable range XML 1.0 holds.
    const FIRST_PRINTABLE: char = '\u{20}';
    /// Last scalar before the surrogate block.
    const LAST_BEFORE_SURROGATES: char = '\u{d7ff}';
    /// First scalar after it.
    const FIRST_AFTER_SURROGATES: char = '\u{e000}';
    /// Last scalar of the basic multilingual plane XML 1.0 holds.
    const LAST_BASIC: char = '\u{fffd}';
    /// First scalar of the supplementary planes.
    const FIRST_SUPPLEMENTARY: char = '\u{10000}';
    /// Last scalar there is.
    const LAST_SUPPLEMENTARY: char = '\u{10ffff}';

    let holdable = |character: char| {
        matches!(character, '\u{9}' | '\u{a}' | '\u{d}')
            || matches!(character, FIRST_PRINTABLE..=LAST_BEFORE_SURROGATES)
            || matches!(character, FIRST_AFTER_SURROGATES..=LAST_BASIC)
            || matches!(character, FIRST_SUPPLEMENTARY..=LAST_SUPPLEMENTARY)
    };
    if path.chars().all(holdable) { Ok(()) } else { Err(SelectionFailure::NotRepresentableInXml) }
}

/// Returns `value` with the five XML attribute characters escaped.
///
/// One pass. The bytes an escape produces are never scanned again, so an
/// ampersand in the input becomes `&amp;` and not `&amp;amp;`.
#[must_use]
pub fn escape_xml_attribute(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}
