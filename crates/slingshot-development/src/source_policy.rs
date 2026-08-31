//! Repository source policy checker.
//!
//! The rules here are the falsifiable ones: a file that is too long, a declared
//! name that is not spelled in full, a numeric value with meaning that carries
//! no name, a function that branches too many ways, unchecked syntax in any of
//! its forms, an exported item without documentation, a placeholder standing in
//! for behavior, a marker for unfinished work in product prose, and a workflow
//! that reaches beyond least privilege. Whether prose is accurate, complete,
//! historically framed, or narrating adjacent code is a judgement a reader
//! makes; `policy/documentation-rules.toml` records it as a checklist.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;
use slingshot_domain::command::command_identity::CommandContract;
use syn::visit::Visit;

/// Repository path of the source policy values.
pub const SOURCE_POLICY_PATH: &str = "policy/source-policy.toml";

/// Repository path of the shortened forms a declared name may not use.
pub const ABBREVIATED_IDENTIFIERS_PATH: &str = "policy/abbreviated-identifiers.txt";

/// Repository path of the closed external-interface table.
pub const EXTERNAL_INTERFACES_PATH: &str = "policy/external-interface-identifiers.toml";

/// Repository path of the documentation rules and review checklist.
pub const DOCUMENTATION_RULES_PATH: &str = "policy/documentation-rules.toml";

/// Directory every workflow document lives in.
const WORKFLOW_DIRECTORY: &str = ".github/workflows";

/// Directory every executable script lives in.
const SCRIPT_DIRECTORY: &str = "scripts";

/// The values every falsifiable rule reads.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SourcePolicy {
    /// Format identifier of the policy document.
    pub format: String,
    /// Largest physical line count of a repository-owned code file.
    pub maximum_code_file_lines: usize,
    /// Largest cyclomatic complexity of a function or conditional expression.
    pub maximum_cyclomatic_complexity: u32,
    /// Smallest numeric literal the checker attributes meaning to.
    pub smallest_meaningful_literal: u128,
    /// Tokens that mark work the author has not finished.
    pub forbidden_markers: Vec<String>,
    /// Headings that belong to a plan bundle rather than product documentation.
    pub planning_headings: Vec<String>,
    /// Macros whose expansion is a placeholder rather than behavior.
    pub placeholder_macros: Vec<String>,
    /// Markers that switch a rule off where somebody found it inconvenient.
    pub suppression_markers: Vec<String>,
    /// Markers that record an expectation the compiler keeps honest.
    pub expectation_markers: Vec<String>,
    /// What an expectation has to state.
    pub required_expectation_reason: String,
    /// The directory that owns Plan 0003's command contract.
    pub command_contract_directory: String,
    /// Directories the scan does not enter.
    pub excluded_directories: Vec<String>,
    /// The one workflow job that may hold provenance-attestation permissions.
    pub release_attestation_job: String,
    /// Permissions that job alone may add to read-only content access.
    pub release_attestation_permissions: Vec<String>,
    /// Workflow expression prefixes that carry values a caller controls.
    pub untrusted_expression_prefixes: Vec<String>,
}

/// One external interface whose spelling the workspace cannot change.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ExternalInterface {
    /// Leading-colon fully qualified path the implementation must literally name.
    pub path: String,
    /// Locked package or standard-library identity the path belongs to.
    pub package: String,
    /// Exact item spelling the exception covers.
    pub item: String,
    /// Target condition the exception applies under, or the empty string.
    pub target: String,
}

/// The closed external-interface table.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ExternalInterfaces {
    /// Format identifier of the table.
    pub format: String,
    /// One entry per exempt interface.
    pub interface: Vec<ExternalInterface>,
}

/// The documentation rules and the review checklist beside them.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DocumentationRules {
    /// Format identifier of the rules document.
    pub format: String,
    /// Whether every exported item must carry documentation.
    pub exported_items_are_documented: bool,
    /// Section a public function that can fail must carry.
    pub required_failure_section: String,
    /// Section a public function that can end the process must carry.
    pub required_panic_section: String,
    /// Judgements a reader makes, which this module never infers.
    pub review_checklist: Vec<String>,
    /// Where the completed review is recorded.
    pub review_record: String,
    /// The closed subjects the checklist covers, by the entry that covers each.
    pub review_subjects: std::collections::BTreeMap<String, String>,
}

/// One rule a repository file breaks.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Violation {
    /// Repository-relative path of the file.
    pub path: String,
    /// Line the violation was found on.
    pub line: usize,
    /// Rule the file broke.
    pub rule: String,
    /// Symbol or statement the violation is about.
    pub symbol: String,
}

impl Violation {
    /// Records one rule a file broke.
    #[must_use]
    pub(crate) fn at(path: &str, line: usize, rule: &str, symbol: impl Into<String>) -> Self {
        Self { path: path.to_owned(), line, rule: rule.to_owned(), symbol: symbol.into() }
    }
}

impl ::core::fmt::Display for Violation {
    fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        write!(formatter, "{}:{}: {}: {}", self.path, self.line, self.rule, self.symbol)
    }
}

/// Reason the source policy could not be applied.
///
/// The policy can only refuse what it can read, so a file it cannot read is a
/// failure of the run rather than a violation of a rule.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{path} could not be read: {reason}")]
pub struct PolicyFailure {
    /// Repository-relative path of the file.
    pub path: String,
    /// Reason the file could not be read.
    pub reason: String,
}

/// Every policy document, loaded once.
#[derive(Debug, Clone)]
pub struct LoadedPolicy {
    /// Values every falsifiable rule reads.
    pub source: SourcePolicy,
    /// Shortened forms a declared name may not use.
    pub abbreviations: BTreeSet<String>,
    /// Closed external-interface table.
    pub interfaces: ExternalInterfaces,
    /// Documentation rules and the review checklist.
    pub documentation: DocumentationRules,
}

/// Reads one policy document out of the repository.
fn read_policy(root: &Path, relative: &str) -> Result<String, PolicyFailure> {
    std::fs::read_to_string(root.join(relative))
        .map_err(|failure| PolicyFailure { path: relative.to_owned(), reason: failure.to_string() })
}

/// Parses one policy document.
fn parse_policy<Shape: serde::de::DeserializeOwned>(
    relative: &str,
    text: &str,
) -> Result<Shape, PolicyFailure> {
    toml::from_str(text)
        .map_err(|failure| PolicyFailure { path: relative.to_owned(), reason: failure.to_string() })
}

impl LoadedPolicy {
    /// Loads every policy document from one repository root.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyFailure`] when a document is missing
    /// or is not a valid policy document.
    pub fn load(root: &Path) -> Result<Self, PolicyFailure> {
        let source = parse_policy(SOURCE_POLICY_PATH, &read_policy(root, SOURCE_POLICY_PATH)?)?;
        let interfaces =
            parse_policy(EXTERNAL_INTERFACES_PATH, &read_policy(root, EXTERNAL_INTERFACES_PATH)?)?;
        let documentation =
            parse_policy(DOCUMENTATION_RULES_PATH, &read_policy(root, DOCUMENTATION_RULES_PATH)?)?;
        let abbreviations = read_policy(root, ABBREVIATED_IDENTIFIERS_PATH)?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_owned)
            .collect();
        Ok(Self { source, abbreviations, interfaces, documentation })
    }

    /// Reports whether a declared name is spelled in full.
    #[must_use]
    pub fn name_is_spelled_in_full(&self, name: &str) -> bool {
        let trimmed = name.trim_start_matches('_').trim_start_matches('\'');
        if trimmed.is_empty() {
            return true;
        }
        if trimmed.chars().count() == 1 {
            return false;
        }
        trimmed
            .split('_')
            .filter(|word| !word.is_empty())
            .all(|word| !self.abbreviations.contains(&word.to_lowercase()))
    }
}

/// Collects every rule one Rust file breaks.
struct RustScan<'policy> {
    policy: &'policy LoadedPolicy,
    path: String,
    violations: Vec<Violation>,
    named_value_depth: u32,
}

/// Returns the line one syntax element begins on.
fn line_of(spanned: &impl syn::spanned::Spanned) -> usize {
    spanned.span().start().line
}

/// Returns the whole documentation text of one attribute list.
fn documentation_text(attributes: &[syn::Attribute]) -> String {
    let mut collected = String::new();
    for attribute in attributes.iter().filter(|found| found.path().is_ident("doc")) {
        if let syn::Meta::NameValue(named) = &attribute.meta
            && let syn::Expr::Lit(literal) = &named.value
            && let syn::Lit::Str(text) = &literal.lit
        {
            collected.push_str(&text.value());
            collected.push('\n');
        }
    }
    collected
}

/// Reports whether a signature returns a fallible result.
fn returns_result(signature: &syn::Signature) -> bool {
    let syn::ReturnType::Type(_, returned) = &signature.output else {
        return false;
    };
    let syn::Type::Path(path) = returned.as_ref() else {
        return false;
    };
    path.path.segments.last().is_some_and(|segment| segment.ident == "Result")
}

/// Reports whether an item is exported from the crate it is declared in.
fn is_exported(visibility: &syn::Visibility) -> bool {
    matches!(visibility, syn::Visibility::Public(_))
}

/// Counts the decisions one block reaches, as cyclomatic complexity.
#[derive(Default)]
struct ComplexityScan {
    decisions: u32,
}

impl<'ast> ::syn::visit::Visit<'ast> for ComplexityScan {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        self.decisions += 1;
        syn::visit::visit_expr_if(self, node);
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.decisions += 1;
        syn::visit::visit_expr_while(self, node);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.decisions += 1;
        syn::visit::visit_expr_for_loop(self, node);
    }

    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        self.decisions += u32::try_from(node.arms.len()).unwrap_or_default().saturating_sub(1);
        syn::visit::visit_expr_match(self, node);
    }

    fn visit_expr_binary(&mut self, node: &'ast syn::ExprBinary) {
        if matches!(node.op, syn::BinOp::And(_) | syn::BinOp::Or(_)) {
            self.decisions += 1;
        }
        syn::visit::visit_expr_binary(self, node);
    }

    fn visit_item_fn(&mut self, _node: &'ast syn::ItemFn) {}
}

/// Returns the cyclomatic complexity of one block.
fn complexity_of(block: &syn::Block) -> u32 {
    let mut scan = ComplexityScan::default();
    scan.visit_block(block);
    scan.decisions + 1
}

impl<'policy> RustScan<'policy> {
    /// Begins a scan of one Rust file.
    fn new(policy: &'policy LoadedPolicy, path: &str) -> Self {
        Self { policy, path: path.to_owned(), violations: Vec::new(), named_value_depth: 0 }
    }

    /// Records one violation.
    fn report(&mut self, line: usize, rule: &str, symbol: impl Into<String>) {
        self.violations.push(Violation::at(&self.path, line, rule, symbol));
    }

    /// Refuses a declared name that is not spelled in full.
    fn check_name(&mut self, identifier: &syn::Ident) {
        let name = identifier.to_string();
        if !self.policy.name_is_spelled_in_full(&name) {
            self.report(line_of(identifier), "declared-name-is-not-spelled-in-full", name);
        }
    }

    /// Refuses an exported item that carries no documentation.
    fn check_documentation(
        &mut self,
        visibility: &syn::Visibility,
        attributes: &[syn::Attribute],
        signature: Option<&syn::Signature>,
        line: usize,
        symbol: &str,
    ) {
        if !is_exported(visibility) || !self.policy.documentation.exported_items_are_documented {
            return;
        }
        if documentation_text(attributes).trim().is_empty() {
            self.report(line, "exported-item-is-not-documented", symbol);
            return;
        }
        let required = &self.policy.documentation.required_failure_section;
        if signature.is_some_and(returns_result)
            && !documentation_text(attributes).contains(required.as_str())
        {
            self.report(line, "fallible-interface-omits-its-failure-section", symbol);
        }
    }

    /// Records one declared item's name and documentation.
    fn declare(&mut self, name: &syn::Ident, seen: &syn::Visibility, notes: &[syn::Attribute]) {
        self.check_name(name);
        self.check_documentation(seen, notes, None, line_of(name), &name.to_string());
    }

    /// Records one function's name, documentation, and branching.
    fn declare_function(
        &mut self,
        signature: &syn::Signature,
        visibility: &syn::Visibility,
        attributes: &[syn::Attribute],
        block: &syn::Block,
    ) {
        self.check_name(&signature.ident);
        let line = line_of(&signature.ident);
        let symbol = signature.ident.to_string();
        self.check_documentation(visibility, attributes, Some(signature), line, &symbol);
        let reached = complexity_of(block);
        if reached > self.policy.source.maximum_cyclomatic_complexity {
            let detail = format!("{symbol} reaches {reached}");
            self.report(line, "function-branches-too-many-ways", detail);
        }
    }

    /// Reports whether one signature is exempt through the closed table.
    fn signature_is_exempt(&self, header: &str, signature: &syn::Signature) -> bool {
        let qualified = format!("{header}::{}", signature.ident);
        self.policy.interfaces.interface.iter().any(|interface| interface.path == qualified)
    }
}

impl<'ast> ::syn::visit::Visit<'ast> for RustScan<'_> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.declare_function(&node.sig, &node.vis, &node.attrs, &node.block);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.declare_function(&node.sig, &node.vis, &node.attrs, &node.block);
        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if node.unsafety.is_some() {
            self.report(line_of(node), "unchecked-implementation", "impl");
        }
        let header = node.trait_.as_ref().map(|(path, _)| quote_path(path)).unwrap_or_default();
        for item in &node.items {
            if let syn::ImplItem::Fn(function) = item
                && self.signature_is_exempt(&header, &function.sig)
            {
                // The signature spelling belongs to the external interface;
                // everything the body declares is still this workspace's own.
                let reached = complexity_of(&function.block);
                if reached > self.policy.source.maximum_cyclomatic_complexity {
                    let detail = format!("{} reaches {reached}", function.sig.ident);
                    let line = line_of(&function.sig.ident);
                    self.report(line, "function-branches-too-many-ways", detail);
                }
                self.visit_block(&function.block);
                continue;
            }
            self.visit_impl_item(item);
        }
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.declare(&node.ident, &node.vis, &node.attrs);
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.declare(&node.ident, &node.vis, &node.attrs);
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.check_name(&node.ident);
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        if node.unsafety.is_some() {
            self.report(line_of(&node.ident), "unchecked-contract", node.ident.to_string());
        }
        self.declare(&node.ident, &node.vis, &node.attrs);
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.declare(&node.ident, &node.vis, &node.attrs);
        self.named_value_depth += 1;
        syn::visit::visit_item_const(self, node);
        self.named_value_depth -= 1;
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.check_name(&node.ident);
        self.named_value_depth += 1;
        syn::visit::visit_item_static(self, node);
        self.named_value_depth -= 1;
    }

    fn visit_impl_item_const(&mut self, node: &'ast syn::ImplItemConst) {
        self.check_name(&node.ident);
        self.named_value_depth += 1;
        syn::visit::visit_impl_item_const(self, node);
        self.named_value_depth -= 1;
    }

    fn visit_variant(&mut self, node: &'ast syn::Variant) {
        self.check_name(&node.ident);
        self.named_value_depth += 1;
        syn::visit::visit_variant(self, node);
        self.named_value_depth -= 1;
    }

    fn visit_field(&mut self, node: &'ast syn::Field) {
        if let Some(identifier) = &node.ident {
            self.check_name(identifier);
        }
        syn::visit::visit_field(self, node);
    }

    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        self.check_name(&node.ident);
        syn::visit::visit_pat_ident(self, node);
    }

    fn visit_type_param(&mut self, node: &'ast syn::TypeParam) {
        self.check_name(&node.ident);
        syn::visit::visit_type_param(self, node);
    }

    fn visit_lifetime(&mut self, node: &'ast syn::Lifetime) {
        self.check_name(&node.ident);
    }

    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        self.report(line_of(node), "unchecked-block", "unsafe");
        syn::visit::visit_expr_unsafe(self, node);
    }

    fn visit_signature(&mut self, node: &'ast syn::Signature) {
        if matches!(node.safety, syn::Safety::Unsafe(_)) {
            self.report(line_of(&node.ident), "unchecked-function", node.ident.to_string());
        }
        syn::visit::visit_signature(self, node);
    }

    fn visit_item_foreign_mod(&mut self, node: &'ast syn::ItemForeignMod) {
        self.report(line_of(node), "foreign-declaration-block", "extern");
        syn::visit::visit_item_foreign_mod(self, node);
    }

    fn visit_expr_index(&mut self, node: &'ast syn::ExprIndex) {
        self.visit_expr(&node.expr);
        self.named_value_depth += 1;
        self.visit_expr(&node.index);
        self.named_value_depth -= 1;
    }

    fn visit_expr_array(&mut self, node: &'ast syn::ExprArray) {
        self.named_value_depth += 1;
        syn::visit::visit_expr_array(self, node);
        self.named_value_depth -= 1;
    }

    fn visit_lit_int(&mut self, node: &'ast syn::LitInt) {
        if self.named_value_depth > 0 {
            return;
        }
        let Ok(value) = node.base10_parse::<u128>() else {
            return;
        };
        if value >= self.policy.source.smallest_meaningful_literal {
            self.report(line_of(node), "numeric-value-carries-no-name", node.to_string());
        }
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let name =
            node.path.segments.last().map(|segment| segment.ident.to_string()).unwrap_or_default();
        if self.policy.source.placeholder_macros.contains(&name) {
            self.report(line_of(&node.path), "placeholder-stands-in-for-behavior", name);
        }
    }
}

/// Renders one trait path as the header text an exemption is matched against.
fn quote_path(path: &syn::Path) -> String {
    let leading = if path.leading_colon.is_some() { "::" } else { "" };
    let segments: Vec<String> =
        path.segments.iter().map(|segment| segment.ident.to_string()).collect();
    format!("{leading}{}", segments.join("::"))
}

/// What kind of repository file one path holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// Repository-owned Rust.
    Rust,
    /// A package or workspace manifest.
    Manifest,
    /// A workflow document.
    Workflow,
    /// An executable script.
    Script,
    /// A Structured Query Language migration.
    Migration,
    /// Product prose.
    Prose,
}

/// Classifies one repository-relative path, if the policy examines it.
#[must_use]
pub fn classify(relative: &str) -> Option<SourceKind> {
    let workflow = relative.starts_with(WORKFLOW_DIRECTORY)
        && (relative.ends_with(".yml") || relative.ends_with(".yaml"));
    match relative {
        _ if workflow => Some(SourceKind::Workflow),
        _ if relative.starts_with(SCRIPT_DIRECTORY) && !relative.contains('.') => {
            Some(SourceKind::Script)
        }
        _ if relative.ends_with(".rs") => Some(SourceKind::Rust),
        _ if relative.ends_with(".sql") => Some(SourceKind::Migration),
        _ if relative.ends_with("Cargo.toml") => Some(SourceKind::Manifest),
        _ if relative.ends_with(".md") => Some(SourceKind::Prose),
        _ => None,
    }
}

/// Refuses a file that is longer than the policy allows.
pub(crate) fn check_line_count(policy: &LoadedPolicy, path: &str, text: &str) -> Vec<Violation> {
    let lines = text.lines().count();
    if lines <= policy.source.maximum_code_file_lines {
        return Vec::new();
    }
    vec![Violation::at(path, lines, "file-is-longer-than-the-ceiling", format!("{lines} lines"))]
}

/// Refuses a marker that switches a rule off where somebody found it awkward.
///
/// A rule with an escape hatch holds only where nobody minded it, which is not
/// what a rule is for. This looks at raw lines rather than at parsed syntax,
/// because the point is to catch the marker wherever it is written - inside a
/// comment, above an item, or at the top of a file.
///
/// An expectation is a different thing and is admitted when it says why it is
/// there. The compiler reports an expectation whose lint has stopped firing, so
/// it cannot quietly outlive the situation it was written for; a suppression
/// can, and does.
pub(crate) fn check_suppressions(policy: &LoadedPolicy, path: &str, text: &str) -> Vec<Violation> {
    let mut violations = Vec::new();
    let rule = "suppression-marker-silences-a-rule";
    let reason = policy.source.required_expectation_reason.as_str();
    for (offset, line) in text.lines().enumerate() {
        let line_number = offset + FIRST_LINE;
        for marker in &policy.source.suppression_markers {
            if line.contains(marker.as_str()) {
                violations.push(Violation::at(path, line_number, rule, marker.clone()));
            }
        }
        let unexplained = policy
            .source
            .expectation_markers
            .iter()
            .filter(|marker| line.contains(marker.as_str()) && !line.contains(reason));
        for marker in unexplained {
            violations.push(Violation::at(path, line_number, rule, marker.clone()));
        }
    }
    violations
}

/// Refuses a source that writes Plan 0003's command contract down again.
///
/// The contract is one document and every consumer asks it for a limit by
/// name. A constant named after a limit, or the contract's own format
/// identifier spelled out, is a second declaration - and a second declaration
/// is a thing that can disagree with the first, quietly, for as long as nobody
/// compares them.
pub(crate) fn check_contract_redeclaration(
    policy: &LoadedPolicy,
    path: &str,
    text: &str,
) -> Vec<Violation> {
    if path.starts_with(policy.source.command_contract_directory.as_str()) {
        return Vec::new();
    }
    let contract = CommandContract::embedded();
    let mut violations = Vec::new();
    let rule = "contract-value-is-declared-again";
    for (offset, line) in text.lines().enumerate() {
        let line_number = offset + FIRST_LINE;
        for identifier in [contract.format.as_str(), contract.canonicalization.as_str()] {
            if line.contains(identifier) {
                violations.push(Violation::at(path, line_number, rule, identifier));
            }
        }
        let Some(declared) = declared_constant_name(line) else {
            continue;
        };
        if contract.limits.contains_key(&declared.to_lowercase()) {
            violations.push(Violation::at(path, line_number, rule, declared));
        }
    }
    violations
}

/// Returns the name one line declares a constant or static under.
fn declared_constant_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("pub const ")
        .or_else(|| trimmed.strip_prefix("const "))
        .or_else(|| trimmed.strip_prefix("pub static "))
        .or_else(|| trimmed.strip_prefix("static "))
        .or_else(|| trimmed.strip_prefix("readonly "))?;
    let named: String =
        rest.chars().take_while(|held| held.is_ascii_alphanumeric() || *held == '_').collect();
    if named.is_empty() { None } else { Some(named) }
}

/// Refuses an unfinished-work marker or a planning heading in product prose.
fn check_prose(policy: &LoadedPolicy, path: &str, text: &str) -> Vec<Violation> {
    let mut violations = Vec::new();
    let mut report = |line: usize, rule: &str, symbol: &String| {
        let found = Violation {
            path: path.to_owned(),
            line,
            rule: rule.to_owned(),
            symbol: symbol.clone(),
        };
        violations.push(found);
    };
    for (offset, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let markers =
            policy.source.forbidden_markers.iter().filter(|found| line.contains(found.as_str()));
        for marker in markers {
            report(offset + 1, "unfinished-work-marker", marker);
        }
        let headings = policy
            .source
            .planning_headings
            .iter()
            .filter(|found| trimmed.starts_with(found.as_str()));
        for heading in headings {
            report(offset + 1, "planning-heading-in-product-prose", heading);
        }
    }
    violations
}

/// Refuses every rule one Rust file breaks.
fn check_rust(policy: &LoadedPolicy, path: &str, text: &str) -> Vec<Violation> {
    let mut violations = check_line_count(policy, path, text);
    let parsed = match syn::parse_file(text) {
        Ok(parsed) => parsed,
        Err(failure) => {
            let line = failure.span().start().line;
            violations.push(Violation::at(
                path,
                line,
                "source-is-not-parseable",
                failure.to_string(),
            ));
            return violations;
        }
    };
    let mut scan = RustScan::new(policy, path);
    scan.visit_file(&parsed);
    violations.extend(scan.violations);
    violations.extend(check_prose(policy, path, &documentation_lines(text)));
    violations.extend(check_suppressions(policy, path, text));
    violations.extend(check_contract_redeclaration(policy, path, text));
    violations.sort();
    violations
}

/// Returns every documentation line one source file carries.
fn documentation_lines(text: &str) -> String {
    let documented = text.lines().filter_map(|line| {
        let trimmed = line.trim_start();
        trimmed.strip_prefix("///").or_else(|| trimmed.strip_prefix("//!"))
    });
    documented.collect::<Vec<&str>>().join("\n")
}

/// Refuses every rule one migration breaks.
fn check_migration(policy: &LoadedPolicy, path: &str, text: &str) -> Vec<Violation> {
    use sqlparser::ast::Statement;

    let mut violations = check_line_count(policy, path, text);
    let dialect = sqlparser::dialect::SQLiteDialect {};
    let statements = match sqlparser::parser::Parser::parse_sql(&dialect, text) {
        Ok(statements) => statements,
        Err(failure) => {
            let rule = "source-is-not-parseable";
            violations.push(Violation::at(path, FIRST_LINE, rule, failure.to_string()));
            return violations;
        }
    };
    let mut declared = Vec::new();
    for statement in &statements {
        match statement {
            Statement::CreateTable(created) => {
                declared.push(created.name.to_string());
                declared.extend(created.columns.iter().map(|column| column.name.to_string()));
            }
            Statement::CreateIndex(created) => {
                declared.extend(created.name.as_ref().map(ToString::to_string));
            }
            _ => {}
        }
    }
    let refused = declared
        .iter()
        .map(|name| name.trim_matches('"'))
        .filter(|name| !policy.name_is_spelled_in_full(name));
    for name in refused {
        let rule = "declared-name-is-not-spelled-in-full";
        violations.push(Violation::at(path, FIRST_LINE, rule, name));
    }
    violations.sort();
    violations
}

/// Line a violation is attributed to when its source has no line of its own.
pub(crate) const FIRST_LINE: usize = 1;

/// Refuses every rule one repository file breaks.
///
/// # Errors
///
/// Returns [`PolicyFailure`] when the file cannot be read.
pub fn check_file(
    policy: &LoadedPolicy,
    root: &Path,
    relative: &str,
) -> Result<Vec<Violation>, PolicyFailure> {
    let Some(kind) = classify(relative) else {
        return Ok(Vec::new());
    };
    let text = std::fs::read_to_string(root.join(relative)).map_err(|failure| PolicyFailure {
        path: relative.to_owned(),
        reason: failure.to_string(),
    })?;
    Ok(match kind {
        SourceKind::Rust => check_rust(policy, relative, &text),
        SourceKind::Workflow => crate::workflow_policy::check(policy, relative, &text),
        SourceKind::Script => crate::script_policy::check(policy, relative, &text),
        SourceKind::Migration => check_migration(policy, relative, &text),
        SourceKind::Manifest => check_line_count(policy, relative, &text),
        SourceKind::Prose => {
            let mut found = check_line_count(policy, relative, &text);
            found.extend(check_prose(policy, relative, &text));
            found.extend(check_suppressions(policy, relative, &text));
            found
        }
    })
}

/// Returns every repository-relative path the policy examines.
///
/// # Errors
///
/// Returns [`PolicyFailure`] when a directory cannot be read.
pub fn examined_paths(policy: &LoadedPolicy, root: &Path) -> Result<Vec<String>, PolicyFailure> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory).map_err(|failure| PolicyFailure {
            path: directory.display().to_string(),
            reason: failure.to_string(),
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(relative) = path.strip_prefix(root) else { continue };
            let relative = relative.to_string_lossy().into_owned();
            let excluded = &policy.source.excluded_directories;
            if excluded.iter().any(|name| relative.starts_with(name.as_str())) {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else if classify(&relative).is_some() {
                found.push(relative);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// Refuses every rule the repository breaks.
///
/// # Errors
///
/// Returns [`PolicyFailure`] when a policy document or a repository file cannot
/// be read.
pub fn check_repository(root: &Path) -> Result<Vec<Violation>, PolicyFailure> {
    let policy = LoadedPolicy::load(root)?;
    let mut violations = Vec::new();
    for relative in examined_paths(&policy, root)? {
        violations.extend(check_file(&policy, root, &relative)?);
    }
    violations.sort();
    Ok(violations)
}
