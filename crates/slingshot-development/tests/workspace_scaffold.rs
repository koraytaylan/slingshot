//! Structural assertions for the unpublished workspace scaffold.
//!
//! The manifests are read twice from independent sources: a manifest reader
//! written for this test parses the committed Cargo manifests, and resolved
//! Cargo metadata supplies the package, target, and dependency inventory. The
//! workspace resolver is only ever read from the root manifest and is never
//! claimed to be a Cargo metadata schema version 1 field.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use manifest_reader::ManifestDocument;
use metadata_reader::{JsonValue, PackageFacts};

/// Directory holding the fixtures this test compares the workspace against.
const FIXTURE_DIRECTORY: &str = "tests/fixtures/workspace-scaffold";

/// Fixture describing the expected workspace shape.
const EXPECTED_WORKSPACE_FIXTURE: &str = "expected-workspace.toml";

/// Package that owns the product executable.
const PRODUCT_PACKAGE_NAME: &str = "slingshot-command-line";

/// Binary target name of the product executable.
const PRODUCT_BINARY_NAME: &str = "slingshot";

/// Argument that asks the product executable for its version line.
const VERSION_ARGUMENT: &str = "--version";

/// Number of library targets every workspace member declares.
const LIBRARY_TARGETS_PER_PACKAGE: usize = 1;

/// Outermost tooling package, which no other local package may depend on.
const DEVELOPMENT_PACKAGE_NAME: &str = "slingshot-development";

/// Manifest sections that may declare a dependency.
const DEPENDENCY_SECTIONS: &[&str] = &["dependencies", "build-dependencies", "dev-dependencies"];

mod manifest_reader {
    //! A reader for the exact Cargo manifest subset this workspace uses:
    //! table headers, array-of-table headers, quoted strings, booleans, and
    //! single- or multi-line lists of quoted strings, each assignment
    //! flattened onto a dotted key path like `workspace.lints.rust.unsafe_code`.

    use std::collections::BTreeMap;

    /// Value read from a manifest assignment.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ManifestValue {
        /// A quoted string, or an unquoted token the subset does not model.
        Text(String),
        /// A `true` or `false` literal.
        Boolean(bool),
        /// A list of quoted strings.
        List(Vec<String>),
    }

    impl ManifestValue {
        /// Returns the text when this value is [`ManifestValue::Text`].
        #[must_use]
        pub fn as_text(&self) -> Option<&str> {
            if let Self::Text(value) = self { Some(value.as_str()) } else { None }
        }

        /// Returns the flag when this value is [`ManifestValue::Boolean`].
        #[must_use]
        pub fn as_boolean(&self) -> Option<bool> {
            if let Self::Boolean(value) = self { Some(*value) } else { None }
        }

        /// Returns the entries when this value is [`ManifestValue::List`].
        #[must_use]
        pub fn as_list(&self) -> Option<&[String]> {
            if let Self::List(values) = self { Some(values.as_slice()) } else { None }
        }
    }

    /// A manifest flattened onto dotted key paths.
    #[derive(Debug, Clone, Default)]
    pub struct ManifestDocument {
        entries: BTreeMap<String, ManifestValue>,
    }

    impl ManifestDocument {
        /// Returns the value stored at `path`, if the manifest declared one.
        #[must_use]
        pub fn value(&self, path: &str) -> Option<&ManifestValue> {
            self.entries.get(path)
        }

        /// Returns the text stored at `path`.
        #[must_use]
        pub fn text(&self, path: &str) -> Option<&str> {
            self.value(path).and_then(ManifestValue::as_text)
        }

        /// Returns the flag stored at `path`.
        #[must_use]
        pub fn boolean(&self, path: &str) -> Option<bool> {
            self.value(path).and_then(ManifestValue::as_boolean)
        }

        /// Returns the list stored at `path`.
        #[must_use]
        pub fn list(&self, path: &str) -> Option<&[String]> {
            self.value(path).and_then(ManifestValue::as_list)
        }

        /// Returns every declared key path, in sorted order.
        #[must_use]
        pub fn paths(&self) -> Vec<&str> {
            self.entries.keys().map(String::as_str).collect()
        }

        /// Returns the immediate child names declared under `prefix`.
        #[must_use]
        pub fn names_under(&self, prefix: &str) -> Vec<&str> {
            let qualified = format!("{prefix}.");
            let mut names = Vec::new();
            for path in self.entries.keys() {
                let Some(remainder) = path.strip_prefix(qualified.as_str()) else {
                    continue;
                };
                if !remainder.contains('.') {
                    names.push(remainder);
                }
            }
            names
        }
    }

    /// Removes a trailing comment that begins outside a quoted string.
    fn strip_comment(line: &str) -> &str {
        let mut inside_quotes = false;
        for (offset, character) in line.char_indices() {
            if character == '"' {
                inside_quotes = !inside_quotes;
            } else if character == '#' && !inside_quotes {
                return &line[..offset];
            }
        }
        line
    }

    /// Returns the table path when `line` is a table or array-of-table header.
    fn parse_table_header(
        line: &str,
        array_counts: &mut BTreeMap<String, usize>,
    ) -> Option<String> {
        if let Some(inner) = line.strip_prefix("[[").and_then(|rest| rest.strip_suffix("]]")) {
            let name = inner.trim().to_owned();
            let index = array_counts.entry(name.clone()).or_default();
            let header = format!("{name}.{index}");
            *index += 1;
            return Some(header);
        }
        line.strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .map(|inner| inner.trim().to_owned())
    }

    /// Reports whether an accumulated assignment has balanced list brackets.
    fn is_complete_assignment(text: &str) -> bool {
        let mut depth: isize = 0;
        let mut inside_quotes = false;
        for character in text.chars() {
            match character {
                '"' => inside_quotes = !inside_quotes,
                '[' if !inside_quotes => depth += 1,
                ']' if !inside_quotes => depth -= 1,
                _ => {}
            }
        }
        depth == 0
    }

    /// Splits an assignment at its first unquoted equals sign.
    fn split_assignment(text: &str) -> Result<(String, String), String> {
        let mut inside_quotes = false;
        for (offset, character) in text.char_indices() {
            if character == '"' {
                inside_quotes = !inside_quotes;
            } else if character == '=' && !inside_quotes {
                let name = text[..offset].trim().to_owned();
                let value = text[offset + '='.len_utf8()..].trim().to_owned();
                return Ok((name, value));
            }
        }
        Err(format!("manifest line {text:?} is neither a header nor an assignment"))
    }

    /// Splits a bracketed list body into its quoted entries.
    fn split_list_entries(body: &str) -> Vec<String> {
        let mut entries = Vec::new();
        let mut current = String::new();
        let mut inside_quotes = false;
        for character in body.chars() {
            match character {
                '"' => {
                    inside_quotes = !inside_quotes;
                    if !inside_quotes {
                        entries.push(std::mem::take(&mut current));
                    }
                }
                _ if inside_quotes => current.push(character),
                _ => {}
            }
        }
        entries
    }

    /// Reads one manifest value out of its unparsed text.
    fn parse_value(raw: &str) -> Result<ManifestValue, String> {
        if let Some(body) = raw.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            return Ok(ManifestValue::List(split_list_entries(body)));
        }
        if let Some(body) = raw.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')) {
            return Ok(ManifestValue::Text(body.replace("\\\"", "\"")));
        }
        match raw {
            "true" => Ok(ManifestValue::Boolean(true)),
            "false" => Ok(ManifestValue::Boolean(false)),
            "" => Err("manifest assignment has no value".to_owned()),
            other => Ok(ManifestValue::Text(other.to_owned())),
        }
    }

    /// Joins a table path and a key path into one dotted key path.
    fn qualify(table: &str, key: &str) -> String {
        if table.is_empty() { key.to_owned() } else { format!("{table}.{key}") }
    }

    /// Parses the supported Cargo manifest subset out of `source`.
    ///
    /// # Errors
    ///
    /// Returns a description of the first line that is neither a comment, a
    /// table header, nor a complete assignment in the supported subset.
    pub fn parse(source: &str) -> Result<ManifestDocument, String> {
        let mut document = ManifestDocument::default();
        let mut array_counts = BTreeMap::new();
        let mut table = String::new();
        let mut pending = String::new();
        for raw_line in source.lines() {
            let line = strip_comment(raw_line).trim();
            if pending.is_empty() {
                if line.is_empty() {
                    continue;
                }
                if let Some(header) = parse_table_header(line, &mut array_counts) {
                    table = header;
                    continue;
                }
            }
            pending.push_str(line);
            if !is_complete_assignment(&pending) {
                continue;
            }
            let (key, raw_value) = split_assignment(&pending)?;
            document.entries.insert(qualify(&table, &key), parse_value(&raw_value)?);
            pending.clear();
        }
        if pending.is_empty() {
            Ok(document)
        } else {
            Err(format!("manifest ended inside the unfinished assignment {pending:?}"))
        }
    }
}

mod metadata_reader {
    //! A reader for the JavaScript Object Notation Cargo emits: objects,
    //! arrays, numbers, booleans, nulls, and strings whose escapes are the
    //! single-character forms. A `\u` escape is refused, not resolved.

    use std::collections::BTreeMap;

    /// One parsed JavaScript Object Notation value.
    #[derive(Debug, Clone, PartialEq)]
    pub enum JsonValue {
        /// The `null` literal.
        Null,
        /// A `true` or `false` literal.
        Boolean(bool),
        /// Any numeric literal.
        Number(f64),
        /// A string literal with its escapes already resolved.
        Text(String),
        /// An array of values.
        List(Vec<JsonValue>),
        /// An object keyed by member name.
        Map(BTreeMap<String, JsonValue>),
    }

    impl JsonValue {
        /// Returns the member stored under `name` when this value is an object.
        #[must_use]
        pub fn member(&self, name: &str) -> Option<&Self> {
            if let Self::Map(members) = self { members.get(name) } else { None }
        }

        /// Returns the text when this value is a string.
        #[must_use]
        pub fn text(&self) -> Option<&str> {
            if let Self::Text(value) = self { Some(value.as_str()) } else { None }
        }

        /// Returns the entries when this value is an array.
        #[must_use]
        pub fn list(&self) -> Option<&[Self]> {
            if let Self::List(values) = self { Some(values.as_slice()) } else { None }
        }

        /// Returns the text stored under `name`, treating `null` as absent.
        #[must_use]
        pub fn optional_text(&self, name: &str) -> Option<String> {
            self.member(name).and_then(Self::text).map(str::to_owned)
        }
    }

    /// A cursor over the bytes of one JavaScript Object Notation document.
    struct Reader<'source> {
        bytes: &'source [u8],
        position: usize,
    }

    impl<'source> Reader<'source> {
        /// Creates a cursor positioned at the first byte of `bytes`.
        fn new(bytes: &'source [u8]) -> Self {
            Self { bytes, position: 0 }
        }

        /// Advances past insignificant whitespace.
        fn skip_whitespace(&mut self) {
            while let Some(byte) = self.bytes.get(self.position) {
                if byte.is_ascii_whitespace() {
                    self.position += 1;
                } else {
                    break;
                }
            }
        }

        /// Returns the byte at the cursor without advancing.
        fn peek(&self) -> Result<u8, String> {
            self.bytes
                .get(self.position)
                .copied()
                .ok_or_else(|| "the document ended before the value was complete".to_owned())
        }

        /// Consumes `expected` at the cursor or reports the mismatch.
        fn expect(&mut self, expected: u8) -> Result<(), String> {
            let found = self.peek()?;
            if found == expected {
                self.position += 1;
                return Ok(());
            }
            Err(format!(
                "expected {:?} at byte {} but found {:?}",
                char::from(expected),
                self.position,
                char::from(found)
            ))
        }

        /// Consumes a literal keyword and yields `value`.
        fn literal(&mut self, keyword: &str, value: JsonValue) -> Result<JsonValue, String> {
            let end = self.position + keyword.len();
            if self.bytes.get(self.position..end) == Some(keyword.as_bytes()) {
                self.position = end;
                return Ok(value);
            }
            Err(format!("expected the literal {keyword} at byte {}", self.position))
        }

        /// Resolves one escape sequence that follows a backslash.
        fn escape(&mut self) -> Result<char, String> {
            let marker = self.peek()?;
            self.position += 1;
            match marker {
                b'"' => Ok('"'),
                b'\\' => Ok('\\'),
                b'/' => Ok('/'),
                b'b' => Ok('\u{8}'),
                b'f' => Ok('\u{c}'),
                b'n' => Ok('\n'),
                b'r' => Ok('\r'),
                b't' => Ok('\t'),
                other => Err(format!("unsupported escape {:?}", char::from(other))),
            }
        }

        /// Reads a string literal, resolving its escapes.
        fn string(&mut self) -> Result<String, String> {
            self.expect(b'"')?;
            let mut resolved = String::new();
            let mut plain_start = self.position;
            loop {
                let byte = self.peek()?;
                if byte == b'"' {
                    resolved.push_str(self.text_between(plain_start, self.position)?);
                    self.position += 1;
                    return Ok(resolved);
                }
                if byte == b'\\' {
                    resolved.push_str(self.text_between(plain_start, self.position)?);
                    self.position += 1;
                    resolved.push(self.escape()?);
                    plain_start = self.position;
                    continue;
                }
                self.position += 1;
            }
        }

        /// Returns the unescaped slice between two byte offsets.
        fn text_between(&self, start: usize, end: usize) -> Result<&'source str, String> {
            std::str::from_utf8(&self.bytes[start..end])
                .map_err(|failure| format!("a string is not valid text: {failure}"))
        }

        /// Reads a numeric literal.
        fn number(&mut self) -> Result<JsonValue, String> {
            let start = self.position;
            while let Some(byte) = self.bytes.get(self.position) {
                if byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E') {
                    self.position += 1;
                } else {
                    break;
                }
            }
            let text = self.text_between(start, self.position)?;
            text.parse::<f64>()
                .map(JsonValue::Number)
                .map_err(|failure| format!("{text:?} is not a number: {failure}"))
        }

        /// Reads an array.
        fn array(&mut self) -> Result<JsonValue, String> {
            self.expect(b'[')?;
            let mut entries = Vec::new();
            self.skip_whitespace();
            if self.peek()? == b']' {
                self.position += 1;
                return Ok(JsonValue::List(entries));
            }
            loop {
                entries.push(self.value()?);
                self.skip_whitespace();
                if self.peek()? == b',' {
                    self.position += 1;
                    continue;
                }
                self.expect(b']')?;
                return Ok(JsonValue::List(entries));
            }
        }

        /// Reads an object.
        fn object(&mut self) -> Result<JsonValue, String> {
            self.expect(b'{')?;
            let mut members = BTreeMap::new();
            self.skip_whitespace();
            if self.peek()? == b'}' {
                self.position += 1;
                return Ok(JsonValue::Map(members));
            }
            loop {
                self.skip_whitespace();
                let name = self.string()?;
                self.skip_whitespace();
                self.expect(b':')?;
                members.insert(name, self.value()?);
                self.skip_whitespace();
                if self.peek()? == b',' {
                    self.position += 1;
                    continue;
                }
                self.expect(b'}')?;
                return Ok(JsonValue::Map(members));
            }
        }

        /// Reads any value at the cursor.
        fn value(&mut self) -> Result<JsonValue, String> {
            self.skip_whitespace();
            match self.peek()? {
                b'n' => self.literal("null", JsonValue::Null),
                b't' => self.literal("true", JsonValue::Boolean(true)),
                b'f' => self.literal("false", JsonValue::Boolean(false)),
                b'"' => self.string().map(JsonValue::Text),
                b'[' => self.array(),
                b'{' => self.object(),
                _ => self.number(),
            }
        }
    }

    /// Parses one complete JavaScript Object Notation document.
    ///
    /// # Errors
    ///
    /// Returns a description of the first byte offset that does not match the
    /// supported grammar, including trailing bytes after the root value.
    pub fn parse(bytes: &[u8]) -> Result<JsonValue, String> {
        let mut reader = Reader::new(bytes);
        let root = reader.value()?;
        reader.skip_whitespace();
        if reader.position == reader.bytes.len() {
            Ok(root)
        } else {
            Err(format!("unexpected trailing bytes at offset {}", reader.position))
        }
    }

    /// One workspace member as resolved Cargo metadata describes it.
    #[derive(Debug, Clone)]
    pub struct PackageFacts {
        /// Package name.
        pub name: String,
        /// Package version.
        pub version: String,
        /// Rust edition the package compiles under.
        pub edition: String,
        /// Minimum supported Rust version, when the manifest declares one.
        pub rust_version: Option<String>,
        /// Registries the package may be published to, when restricted.
        pub publish_registries: Option<Vec<String>>,
        /// Declared license expression, when the manifest supplies one.
        pub license: Option<String>,
        /// Declared license file, when the manifest supplies one.
        pub license_file: Option<String>,
        /// Declared repository location, when the manifest supplies one.
        pub repository: Option<String>,
        /// Library target names.
        pub library_targets: Vec<String>,
        /// Binary target names.
        pub binary_targets: Vec<String>,
        /// Direct dependencies, paired with their dependency kind.
        pub dependencies: Vec<DependencyFacts>,
    }

    /// One direct dependency edge as resolved Cargo metadata describes it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DependencyFacts {
        /// Dependency package name.
        pub name: String,
        /// Dependency kind: `normal`, `build`, or `development`.
        pub kind: String,
    }

    /// Metadata member name for the normal dependency kind.
    const NORMAL_DEPENDENCY_KIND: &str = "normal";

    /// Metadata member name for the development dependency kind.
    const DEVELOPMENT_DEPENDENCY_KIND: &str = "dev";

    /// Collects the target names of one kind out of a package entry.
    fn target_names(package: &JsonValue, wanted: &str) -> Vec<String> {
        let mut names = Vec::new();
        let Some(targets) = package.member("targets").and_then(JsonValue::list) else {
            return names;
        };
        for target in targets {
            let kinds = target.member("kind").and_then(JsonValue::list).unwrap_or_default();
            if kinds.iter().any(|kind| kind.text() == Some(wanted))
                && let Some(name) = target.member("name").and_then(JsonValue::text)
            {
                names.push(name.to_owned());
            }
        }
        names.sort_unstable();
        names
    }

    /// Collects the direct dependency edges of one package entry.
    fn dependency_facts(package: &JsonValue) -> Vec<DependencyFacts> {
        let mut edges = Vec::new();
        let Some(dependencies) = package.member("dependencies").and_then(JsonValue::list) else {
            return edges;
        };
        for dependency in dependencies {
            let name = dependency.member("name").and_then(JsonValue::text).unwrap_or_default();
            let kind = dependency
                .member("kind")
                .and_then(|value| value.text().map(str::to_owned))
                .unwrap_or_else(|| NORMAL_DEPENDENCY_KIND.to_owned());
            let kind =
                if kind == DEVELOPMENT_DEPENDENCY_KIND { "development".to_owned() } else { kind };
            edges.push(DependencyFacts { name: name.to_owned(), kind });
        }
        edges.sort_by(|left, right| (&left.name, &left.kind).cmp(&(&right.name, &right.kind)));
        edges
    }

    /// Reads the publish restriction of one package entry.
    fn publish_registries(package: &JsonValue) -> Option<Vec<String>> {
        let value = package.member("publish")?;
        if matches!(value, JsonValue::Null) {
            return None;
        }
        Some(
            value
                .list()
                .unwrap_or_default()
                .iter()
                .filter_map(|entry| entry.text().map(str::to_owned))
                .collect(),
        )
    }

    /// Reads a member that every metadata package entry must declare.
    fn required_text(entry: &JsonValue, name: &str) -> Result<String, String> {
        entry.optional_text(name).ok_or_else(|| format!("a metadata package has no {name}"))
    }

    /// Extracts every workspace member out of a metadata document.
    ///
    /// # Errors
    ///
    /// Returns a description when the document has no `packages` array or a
    /// package entry has no name, version, or edition.
    pub fn packages(document: &JsonValue) -> Result<Vec<PackageFacts>, String> {
        let entries = document
            .member("packages")
            .and_then(JsonValue::list)
            .ok_or_else(|| "the metadata document has no packages array".to_owned())?;
        let mut facts = Vec::new();
        for entry in entries {
            facts.push(PackageFacts {
                name: required_text(entry, "name")?,
                version: required_text(entry, "version")?,
                edition: required_text(entry, "edition")?,
                rust_version: entry.optional_text("rust_version"),
                publish_registries: publish_registries(entry),
                license: entry.optional_text("license"),
                license_file: entry.optional_text("license_file"),
                repository: entry.optional_text("repository"),
                library_targets: target_names(entry, "lib"),
                binary_targets: target_names(entry, "bin"),
                dependencies: dependency_facts(entry),
            });
        }
        facts.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(facts)
    }
}

/// Returns the workspace root directory that holds this test's manifests.
fn workspace_root() -> PathBuf {
    slingshot_development::locate_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the development crate lives inside the workspace")
}

/// Reads one repository file relative to the workspace root.
fn read_repository_file(relative: &str) -> String {
    let path = workspace_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Parses one manifest relative to the workspace root.
fn repository_document(relative: &str) -> ManifestDocument {
    manifest_reader::parse(&read_repository_file(relative))
        .unwrap_or_else(|failure| panic!("{relative} is not a supported manifest: {failure}"))
}

/// Parses one fixture manifest owned by this test.
fn fixture_document(name: &str) -> ManifestDocument {
    let relative = format!("crates/slingshot-development/{FIXTURE_DIRECTORY}/{name}");
    repository_document(&relative)
}

/// Returns the resolved Cargo metadata for the workspace.
fn workspace_packages() -> Vec<PackageFacts> {
    let mut rendered = Vec::new();
    slingshot_development::emit_workspace_metadata(&workspace_root(), &mut rendered)
        .expect("cargo metadata describes the workspace");
    let document: JsonValue =
        metadata_reader::parse(&rendered).expect("cargo metadata is well-formed");
    metadata_reader::packages(&document).expect("cargo metadata lists workspace members")
}

/// Returns a fixture list as owned strings.
fn fixture_list(document: &ManifestDocument, path: &str) -> Vec<String> {
    document
        .list(path)
        .unwrap_or_else(|| panic!("the expectation fixture declares {path}"))
        .to_vec()
}

/// Returns a fixture string.
fn fixture_text(document: &ManifestDocument, path: &str) -> String {
    document
        .text(path)
        .unwrap_or_else(|| panic!("the expectation fixture declares {path}"))
        .to_owned()
}

/// Compares an expected and an observed name set and reports each difference.
fn compare_name_sets(expected: &[String], observed: &[String]) -> Vec<String> {
    let mut differences = Vec::new();
    for name in expected {
        if !observed.contains(name) {
            differences.push(format!("missing {name}"));
        }
    }
    for name in observed {
        if !expected.contains(name) {
            differences.push(format!("additional {name}"));
        }
    }
    differences.sort();
    differences
}

/// Reports every way a member manifest fails to inherit the lint table.
fn evaluate_lint_inheritance(document: &ManifestDocument) -> Vec<String> {
    let mut violations = Vec::new();
    if document.boolean("lints.workspace") != Some(true) {
        violations.push("the manifest does not set lints.workspace to true".to_owned());
    }
    for path in document.paths() {
        if path.starts_with("lints.") && path != "lints.workspace" {
            violations.push(format!("the manifest overrides the workspace lint table at {path}"));
        }
    }
    violations
}

/// Reports every owner-supplied release field a package still lacks.
fn evaluate_release_prerequisites(
    license: Option<&str>,
    license_file: Option<&str>,
    repository: Option<&str>,
) -> Vec<String> {
    let mut missing = Vec::new();
    if license.is_none() && license_file.is_none() {
        missing.push("license or license-file".to_owned());
    }
    if repository.is_none() {
        missing.push("repository".to_owned());
    }
    missing
}

/// Reports every member dependency that is not inherited from the workspace.
fn evaluate_dependency_centralization(document: &ManifestDocument) -> Vec<String> {
    let mut violations = Vec::new();
    for path in document.paths() {
        let segments: Vec<&str> = path.split('.').collect();
        let found = segments.iter().position(|part| DEPENDENCY_SECTIONS.contains(part));
        let Some(section) = found else { continue };
        let remainder = &segments[section + 1..];
        if remainder.last() != Some(&"workspace") || document.boolean(path) != Some(true) {
            violations.push(format!("{path} does not inherit its dependency from the workspace"));
        }
    }
    violations
}

/// Reports every forbidden product edge that reaches a support crate.
fn evaluate_support_edges(package: &PackageFacts, support: &[String]) -> Vec<String> {
    package
        .dependencies
        .iter()
        .filter(|edge| support.contains(&edge.name) && edge.kind != "development")
        .map(|edge| format!("{} reaches {} as a {} dependency", package.name, edge.name, edge.kind))
        .collect()
}

#[test]
fn workspace_declares_exactly_the_expected_package_set() {
    let expectation = fixture_document(EXPECTED_WORKSPACE_FIXTURE);
    let expected = fixture_list(&expectation, "packages");
    let observed: Vec<String> =
        workspace_packages().into_iter().map(|package| package.name).collect();
    assert_eq!(compare_name_sets(&expected, &observed), Vec::<String>::new());
    let split = fixture_list(&expectation, "product-packages")
        .into_iter()
        .chain(fixture_list(&expectation, "support-packages"))
        .collect::<Vec<String>>();
    assert_eq!(compare_name_sets(&expected, &split), Vec::<String>::new());
    let mut without_storage = expected.clone();
    without_storage.retain(|name| name != "slingshot-storage");
    assert_eq!(compare_name_sets(&expected, &without_storage), vec!["missing slingshot-storage"]);
    let mut with_extra = expected.clone();
    with_extra.push("slingshot-surprise".to_owned());
    assert_eq!(compare_name_sets(&expected, &with_extra), vec!["additional slingshot-surprise"]);
}

#[test]
fn root_manifest_declares_the_resolver_edition_toolchain_and_lint_policy() {
    let expectation = fixture_document(EXPECTED_WORKSPACE_FIXTURE);
    let root = repository_document("Cargo.toml");
    assert_eq!(
        root.text("workspace.resolver"),
        Some(fixture_text(&expectation, "resolver").as_str())
    );
    assert_eq!(
        root.text("workspace.package.edition"),
        Some(fixture_text(&expectation, "edition").as_str())
    );
    assert_eq!(
        root.text("workspace.package.rust-version"),
        Some(fixture_text(&expectation, "rust-version").as_str())
    );
    assert_eq!(
        root.text("workspace.package.version"),
        Some(fixture_text(&expectation, "version").as_str())
    );
    assert_eq!(root.boolean("workspace.package.publish"), expectation.boolean("publish"));
    assert_eq!(
        root.text("workspace.lints.rust.unsafe_code"),
        Some(fixture_text(&expectation, "unsafe-code-lint-level").as_str())
    );
    let directory = fixture_text(&expectation, "member-directory");
    let expected_members: Vec<String> = fixture_list(&expectation, "packages")
        .iter()
        .map(|name| format!("{directory}/{name}"))
        .collect();
    let declared = root.list("workspace.members").expect("the root manifest lists members");
    assert_eq!(compare_name_sets(&expected_members, declared), Vec::<String>::new());
}

#[test]
fn toolchain_file_pins_the_workspace_compiler_and_components() {
    let expectation = fixture_document(EXPECTED_WORKSPACE_FIXTURE);
    let toolchain = repository_document("rust-toolchain.toml");
    assert_eq!(
        toolchain.text("toolchain.channel"),
        Some(fixture_text(&expectation, "rust-version").as_str())
    );
    let components = toolchain.list("toolchain.components").expect("components are pinned");
    assert!(components.iter().any(|entry| entry == "rustfmt"), "{components:?}");
    assert!(components.iter().any(|entry| entry == "clippy"), "{components:?}");
    assert!(workspace_root().join("rustfmt.toml").is_file(), "formatter settings are committed");
}

#[test]
fn cargo_metadata_declares_the_expected_library_and_binary_targets() {
    let expectation = fixture_document(EXPECTED_WORKSPACE_FIXTURE);
    let edition = fixture_text(&expectation, "edition");
    let rust_version = fixture_text(&expectation, "rust-version");
    let version = fixture_text(&expectation, "version");
    let mut expected_binaries = BTreeMap::new();
    for binary in expectation.names_under("binaries") {
        expected_binaries
            .insert(binary.to_owned(), fixture_text(&expectation, &format!("binaries.{binary}")));
    }
    let mut observed_binaries = BTreeMap::new();
    for package in workspace_packages() {
        assert_eq!(package.edition, edition, "{} edition", package.name);
        assert_eq!(package.version, version, "{} version", package.name);
        assert_eq!(
            package.rust_version.as_deref(),
            Some(rust_version.as_str()),
            "{}",
            package.name
        );
        assert_eq!(package.library_targets.len(), LIBRARY_TARGETS_PER_PACKAGE, "{}", package.name);
        for binary in &package.binary_targets {
            observed_binaries.insert(binary.clone(), package.name.clone());
        }
    }
    assert_eq!(observed_binaries, expected_binaries);
    let mut renamed = observed_binaries.clone();
    renamed.insert("slingshot-surprise".to_owned(), PRODUCT_PACKAGE_NAME.to_owned());
    assert_ne!(renamed, expected_binaries, "an additional binary target must be rejected");
    let mut without_product = observed_binaries;
    without_product.remove(PRODUCT_BINARY_NAME);
    assert_ne!(without_product, expected_binaries, "a missing binary target must be rejected");
}

#[test]
fn every_member_manifest_inherits_the_workspace_lint_table() {
    let expectation = fixture_document(EXPECTED_WORKSPACE_FIXTURE);
    let directory = fixture_text(&expectation, "member-directory");
    for name in fixture_list(&expectation, "packages") {
        let member = repository_document(&format!("{directory}/{name}/Cargo.toml"));
        assert_eq!(evaluate_lint_inheritance(&member), Vec::<String>::new(), "{name}");
        assert_eq!(member.boolean("package.publish.workspace"), Some(true), "{name}");
        assert_eq!(member.boolean("package.edition.workspace"), Some(true), "{name}");
        assert_eq!(member.boolean("package.rust-version.workspace"), Some(true), "{name}");
        for legal in ["package.license", "package.license-file", "package.repository"] {
            assert_eq!(member.value(legal), None, "{name} declares {legal}");
        }
    }
}

#[test]
fn member_lint_override_fixtures_are_rejected() {
    assert_eq!(
        evaluate_lint_inheritance(&fixture_document("accepted-member-manifest.toml")),
        Vec::<String>::new()
    );
    for rejected in [
        "rejected-member-lint-allowance.toml",
        "rejected-member-lint-expectation.toml",
        "rejected-member-without-lint-inheritance.toml",
    ] {
        assert!(
            !evaluate_lint_inheritance(&fixture_document(rejected)).is_empty(),
            "{rejected} must be rejected"
        );
    }
}

#[test]
fn every_member_is_unpublished_without_inferred_legal_metadata() {
    for package in workspace_packages() {
        assert_eq!(package.publish_registries, Some(Vec::new()), "{}", package.name);
        assert_eq!(package.license, None, "{} license", package.name);
        assert_eq!(package.license_file, None, "{} license file", package.name);
        assert_eq!(package.repository, None, "{} repository", package.name);
    }
}

#[test]
fn release_packaging_stays_refused_until_the_owner_supplies_metadata() {
    let expectation = fixture_document(EXPECTED_WORKSPACE_FIXTURE);
    let required = fixture_list(&expectation, "required-release-fields");
    for package in workspace_packages() {
        let missing = evaluate_release_prerequisites(
            package.license.as_deref(),
            package.license_file.as_deref(),
            package.repository.as_deref(),
        );
        assert_eq!(missing, required, "{} must stay unpackageable", package.name);
    }
    let satisfied = fixture_document("release-prerequisites-satisfied.toml");
    assert_eq!(satisfied.boolean("package.publish"), Some(true), "the fixture is publishable");
    assert_eq!(
        evaluate_release_prerequisites(
            satisfied.text("package.license"),
            satisfied.text("package.license-file"),
            satisfied.text("package.repository"),
        ),
        Vec::<String>::new()
    );
}

#[test]
fn no_member_reaches_a_support_crate_or_the_outermost_tooling_crate() {
    let expectation = fixture_document(EXPECTED_WORKSPACE_FIXTURE);
    let support = fixture_list(&expectation, "support-packages");
    let products = fixture_list(&expectation, "product-packages");
    let directory = fixture_text(&expectation, "member-directory");
    for package in workspace_packages() {
        if products.contains(&package.name) {
            assert_eq!(evaluate_support_edges(&package, &support), Vec::<String>::new());
        }
        for edge in &package.dependencies {
            assert_ne!(edge.name, DEVELOPMENT_PACKAGE_NAME, "{} reaches tooling", package.name);
        }
        let member = repository_document(&format!("{directory}/{}/Cargo.toml", package.name));
        let centralized = evaluate_dependency_centralization(&member);
        assert_eq!(centralized, Vec::<String>::new(), "{}", package.name);
    }
}

#[test]
fn product_executable_prints_one_version_line_without_creating_runtime_files() {
    let expectation = fixture_document(EXPECTED_WORKSPACE_FIXTURE);
    let sandbox = std::env::temp_dir().join(format!("slingshot-scaffold-{}", std::process::id()));
    std::fs::remove_dir_all(&sandbox).ok();
    std::fs::create_dir_all(&sandbox).expect("the sandbox root is creatable");
    let produced = Command::new(slingshot_development::cargo_executable())
        .current_dir(workspace_root())
        .args([
            "run",
            "--locked",
            "--quiet",
            "--package",
            PRODUCT_PACKAGE_NAME,
            "--",
            VERSION_ARGUMENT,
        ])
        .env("XDG_RUNTIME_DIR", &sandbox)
        .env("XDG_CONFIG_HOME", &sandbox)
        .env("XDG_STATE_HOME", &sandbox)
        .env("XDG_DATA_HOME", &sandbox)
        .output()
        .expect("cargo run starts");
    let rendered = String::from_utf8(produced.stdout).expect("the version line is text");
    assert!(produced.status.success(), "{}", String::from_utf8_lossy(&produced.stderr));
    let lines: Vec<&str> = rendered.lines().collect();
    let version = fixture_text(&expectation, "version");
    assert_eq!(lines, vec![format!("{PRODUCT_BINARY_NAME} {version}")]);
    let remaining: Vec<PathBuf> = std::fs::read_dir(&sandbox)
        .expect("the sandbox root is readable")
        .filter_map(|entry| entry.ok().map(|found| found.path()))
        .collect();
    assert_eq!(remaining, Vec::<PathBuf>::new(), "the version proof created runtime state");
    std::fs::remove_dir_all(&sandbox).expect("the sandbox root is removable");
}
