//! Assertions for the closed profile, selection, and commit-inventory shapes.
//!
//! Three properties carry most of the weight here. An address has one spelling,
//! so two profiles cannot name the same server two ways. Appending an endpoint
//! extends the context path instead of replacing it, which is the difference
//! between a request that reaches a configured mount point and one that reaches
//! the site root. And a password is never a string: it is redacted from the
//! moment the document is read, so no rendering of a parsed profile can show it.
//!
//! Every bound is exercised at its exact value and one byte over, and both
//! values come from the contract manifest, so no limit is copied into this file.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Deserialize;
use slingshot_domain::configuration_snapshot::{ConfigurationReference, ConfigurationSnapshot};
use slingshot_domain::profile::{
    AdobeExperienceManagerDeployment, BasicUserName, EnvironmentAuthentication, EnvironmentName,
    Profile, ProfileName, SelectionDocument, TierBaseAddress,
};
use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract, narrow_limit,
};

/// Directory holding the contract vector fixtures.
const VECTOR_DIRECTORY: &str = "../slingshot-test-support/fixtures/profile-authentication-contract";

/// Directory holding the profile document fixtures.
const PROFILE_DIRECTORY: &str = "../slingshot-test-support/fixtures/profiles";

/// Directory holding the commit-inventory fixtures.
const SNAPSHOT_DIRECTORY: &str = "../slingshot-test-support/fixtures/configuration-snapshots";

/// Source file the structural scans read.
const PROFILE_SOURCE: &str = "src/profile.rs";

/// A byte every bounded value in these assertions is built from.
const FILLER: &str = "a";

/// The address fixture.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AddressVectors {
    /// Format identifier of the fixture.
    format: String,
    /// Addresses that canonicalize.
    accepted: Vec<AcceptedAddress>,
    /// Addresses that are refused.
    refused: Vec<RefusedAddress>,
}

/// One address and the spelling it canonicalizes to.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptedAddress {
    /// Address as it is written in a document.
    input: String,
    /// The one spelling it has afterwards.
    canonical: String,
    /// Whether its host is one the contract accepts as loopback.
    loopback: bool,
    /// Whether its transport protects what it carries.
    protected: bool,
}

/// One address the contract refuses.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RefusedAddress {
    /// Address as it is written in a document.
    input: String,
    /// Why it cannot be accepted.
    reason: String,
}

/// The endpoint fixture.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EndpointVectors {
    /// Format identifier of the fixture.
    format: String,
    /// One entry per append.
    vector: Vec<EndpointVector>,
}

/// One base address, the segments appended to it, and the result.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EndpointVector {
    /// Base address the segments are appended to.
    base_address: String,
    /// Segments to append.
    segments: Vec<String>,
    /// The resulting endpoint.
    endpoint: String,
}

/// The Basic credential fixture.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BasicVectors {
    /// Format identifier of the fixture.
    format: String,
    /// One entry per credential.
    vector: Vec<BasicVector>,
}

/// One Basic credential and its canonical input bytes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BasicVector {
    /// User name as it is written in a document.
    user_name: String,
    /// Password as it is written in a document.
    password: String,
    /// Exact canonical input bytes, as text.
    canonical: String,
    /// The independently written base64 spelling of those bytes.
    base64: String,
}

/// The boundary fixture.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundaryVectors {
    /// Format identifier of the fixture.
    format: String,
    /// One entry per bounded value.
    vector: Vec<BoundaryVector>,
}

/// One bounded value and the manifest limit that bounds it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundaryVector {
    /// Value the limit bounds.
    subject: String,
    /// Manifest limit that bounds it.
    limit: String,
}

/// Returns the directory this crate's manifest lives in.
fn crate_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Reads one fixture from a named directory.
fn read_fixture(directory: &str, name: &str) -> String {
    let path = crate_directory().join(directory).join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|failure| panic!("{} could not be read: {failure}", path.display()))
}

/// Parses one vector fixture.
fn parse_vectors<Shape: serde::de::DeserializeOwned>(name: &str) -> Shape {
    toml::from_str(&read_fixture(VECTOR_DIRECTORY, name))
        .unwrap_or_else(|failure| panic!("{name} could not be read: {failure}"))
}

/// Returns the limit one fixture names.
fn named_limit(name: &str) -> usize {
    let limits = &ProfileAuthenticationContract::embedded().limits;
    let rendered = serde_json::to_value(limits).expect("the limits render");
    narrow_limit(
        rendered
            .get(name)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_else(|| panic!("the contract declares no limit named {name}")),
    )
}

/// Returns a Basic profile document carrying `password`.
fn basic_document(password: &str) -> String {
    format!(
        "format_version = 1\nname = \"site\"\n\n[environments.development]\n\
         deployment = \"adobe_experience_manager_6_5\"\n\n\
         [environments.development.author]\nbase_address = \"http://localhost:4502\"\n\n\
         [environments.development.publisher]\nbase_address = \"http://localhost:4503\"\n\n\
         [environments.development.authentication]\nmethod = \"basic\"\n\
         user_name = \"admin\"\npassword = \"{password}\"\n"
    )
}

/// Returns a Basic profile document defining `count` environments.
fn many_environment_document(count: usize) -> String {
    let mut document = String::from("format_version = 1\nname = \"site\"\n");
    for index in 0..count {
        document.push_str(&format!(
            "\n[environments.environment-{index}]\n\
             deployment = \"adobe_experience_manager_6_5\"\n\n\
             [environments.environment-{index}.author]\nbase_address = \"http://localhost:4502\"\n\n\
             [environments.environment-{index}.publisher]\nbase_address = \"http://localhost:4503\"\n\n\
             [environments.environment-{index}.authentication]\nmethod = \"basic\"\n\
             user_name = \"admin\"\npassword = \"admin\"\n"
        ));
    }
    document
}

/// Reports whether one subject of the given size is accepted.
fn accepts(subject: &str, size: usize) -> bool {
    let filler = FILLER.repeat(size);
    let repeated = || (0..size).map(|_| FILLER).collect::<Vec<&str>>().join("/");
    match subject {
        "profile_name" => ProfileName::parse(&filler).is_ok(),
        "environment_name" => EnvironmentName::parse(&filler).is_ok(),
        "basic_user_name" => BasicUserName::parse(&filler).is_ok(),
        "basic_password" => Profile::parse(&basic_document(&filler)).is_ok(),
        "environments_per_profile" => Profile::parse(&many_environment_document(size)).is_ok(),
        other => accepts_reference_or_address(other, &filler, &repeated()),
    }
}

/// Reports whether one reference or address subject of that size is accepted.
fn accepts_reference_or_address(subject: &str, filler: &str, repeated: &str) -> bool {
    match subject {
        "reference_component" => ConfigurationReference::parse(filler).is_ok(),
        "reference_components" => ConfigurationReference::parse(repeated).is_ok(),
        "tier_host" => TierBaseAddress::parse(&format!("https://{filler}")).is_ok(),
        "context_path_segment" => TierBaseAddress::parse(&format!("https://host/{filler}")).is_ok(),
        "context_path_segments" => {
            TierBaseAddress::parse(&format!("https://host/{repeated}")).is_ok()
        }
        other => panic!("the fixture names the unknown subject {other}"),
    }
}

/// Returns the code one profile document fails with, if it fails at all.
fn parse_code(document: &str) -> Result<(), ConfigurationFailureCode> {
    Profile::parse(document).map(|_| ()).map_err(|failure| failure.code)
}

/// Returns the named environment of one parsed profile.
fn environment_of<'profile>(
    profile: &'profile Profile,
    name: &str,
) -> &'profile slingshot_domain::profile::Environment {
    let name = EnvironmentName::parse(name).expect("the name is valid");
    profile.environments().get(&name).expect("the profile defines it")
}

#[test]
fn the_documented_profiles_parse_into_the_shapes_the_scope_describes() {
    let basic = Profile::parse(&read_fixture(PROFILE_DIRECTORY, "basic-profile.toml"))
        .expect("the documented Basic profile parses");
    assert_eq!(basic.name().as_text(), "local-site");
    let environment = environment_of(&basic, "development");
    assert_eq!(
        environment.deployment(),
        AdobeExperienceManagerDeployment::AdobeExperienceManager65
    );
    assert_eq!(environment.author_connection_target().as_text(), "http://localhost:4502");
    assert_eq!(environment.publisher_metadata().as_text(), "http://localhost:4503");
    assert!(environment.insecure_author_transport_warning().is_none(), "loopback warns");
    assert!(matches!(
        environment.authentication(),
        EnvironmentAuthentication::BasicCredentials { .. }
    ));

    let cloud = Profile::parse(&read_fixture(PROFILE_DIRECTORY, "cloud-profile.toml"))
        .expect("the documented Cloud profile parses");
    let environment = environment_of(&cloud, "production");
    assert_eq!(
        environment.deployment(),
        AdobeExperienceManagerDeployment::AdobeExperienceManagerCloudService
    );
    assert_eq!(
        environment.additional_certificate_authority_file().map(ConfigurationReference::as_text),
        Some("certificates/corporate-ca.pem")
    );
    assert!(matches!(
        environment.authentication(),
        EnvironmentAuthentication::DeveloperConsoleServiceCredentialsFile { .. }
    ));
}

#[test]
fn a_crossed_deployment_and_authentication_pair_is_refused() {
    let crossed = read_fixture(PROFILE_DIRECTORY, "invalid-authentication-pair.toml");
    assert_eq!(parse_code(&crossed), Err(ConfigurationFailureCode::ConfigurationValueInvalid));
    let cloud_with_basic_members = basic_document("admin").replace(
        "method = \"basic\"",
        "method = \"adobe_experience_manager_developer_console_service_credentials\"",
    );
    assert!(parse_code(&cloud_with_basic_members).is_err());
    let basic_with_credentials_file = basic_document("admin")
        .replace("password = \"admin\"", "credentials_file = \"credentials/production.json\"");
    assert!(parse_code(&basic_with_credentials_file).is_err());
}

#[test]
fn every_address_vector_canonicalizes_or_is_refused() {
    let vectors: AddressVectors = parse_vectors("address-vectors.toml");
    assert_eq!(vectors.format, "slingshot.address-vectors/1");
    for accepted in vectors.accepted {
        let address = TierBaseAddress::parse(&accepted.input)
            .unwrap_or_else(|failure| panic!("{} was refused: {failure}", accepted.input));
        assert_eq!(address.as_text(), accepted.canonical, "{}", accepted.input);
        assert_eq!(address.is_loopback(), accepted.loopback, "{}", accepted.input);
        assert_eq!(address.is_protected(), accepted.protected, "{}", accepted.input);
        let again = TierBaseAddress::parse(&accepted.canonical)
            .expect("a canonical address parses to itself");
        assert_eq!(again.as_text(), accepted.canonical, "canonicalization is not stable");
    }
    for refused in vectors.refused {
        assert!(
            TierBaseAddress::parse(&refused.input).is_err(),
            "{} was accepted despite {}",
            refused.input,
            refused.reason
        );
    }
}

#[test]
fn appending_an_endpoint_cannot_drop_replace_or_escape_the_prefix() {
    let vectors: EndpointVectors = parse_vectors("endpoint-vectors.toml");
    assert_eq!(vectors.format, "slingshot.endpoint-vectors/1");
    for vector in vectors.vector {
        let address =
            TierBaseAddress::parse(&vector.base_address).expect("the base address parses");
        let borrowed: Vec<&str> = vector.segments.iter().map(String::as_str).collect();
        let endpoint = address.endpoint(&borrowed);
        assert_eq!(endpoint, vector.endpoint, "{}", vector.base_address);
        assert!(endpoint.starts_with(address.as_text()), "the prefix did not survive");
    }
}

#[test]
fn canonical_basic_input_is_the_exact_bytes_with_one_separator() {
    let vectors: BasicVectors = parse_vectors("basic-credential-vectors.toml");
    assert_eq!(vectors.format, "slingshot.basic-credential-vectors/1");
    for vector in vectors.vector {
        let document = basic_document(&vector.password)
            .replace("user_name = \"admin\"", &format!("user_name = \"{}\"", vector.user_name));
        let profile = Profile::parse(&document).expect("the credential parses");
        let environment = environment_of(&profile, "development");
        let canonical = environment
            .authentication()
            .lend_canonical_basic_input(<[u8]>::to_vec)
            .expect("Basic authentication has canonical input");
        assert_eq!(canonical, vector.canonical.as_bytes(), "{}", vector.canonical);
        assert_eq!(encode_base64(&canonical), vector.base64, "{}", vector.canonical);
    }
}

#[test]
fn service_credential_authentication_has_no_canonical_basic_input() {
    let cloud = Profile::parse(&read_fixture(PROFILE_DIRECTORY, "cloud-profile.toml"))
        .expect("the documented Cloud profile parses");
    let environment = environment_of(&cloud, "production");
    assert!(environment.authentication().lend_canonical_basic_input(<[u8]>::to_vec).is_none());
}

#[test]
fn every_bounded_value_is_accepted_at_its_bound_and_refused_one_byte_over() {
    let vectors: BoundaryVectors = parse_vectors("boundary-vectors.toml");
    assert_eq!(vectors.format, "slingshot.contract-boundary-vectors/1");
    for vector in vectors.vector {
        let bound = named_limit(&vector.limit);
        assert!(accepts(&vector.subject, bound), "{} was refused at its bound", vector.subject);
        assert!(!accepts(&vector.subject, bound + 1), "{} was accepted over", vector.subject);
    }
}

#[test]
fn the_cleartext_author_permission_is_required_only_where_it_changes_something() {
    let loopback = basic_document("admin");
    assert!(Profile::parse(&loopback).is_ok(), "loopback cleartext needs no opt-in");
    assert!(
        Profile::parse(&format!("{loopback}allow_insecure_author_transport = true\n")).is_err(),
        "a loopback opt-in claims a risk that is not taken"
    );
    assert!(
        Profile::parse(&format!("{loopback}allow_insecure_author_transport = false\n")).is_err(),
        "an explicit refusal says nothing the absence does not"
    );

    let remote = loopback.replace("http://localhost:4502", "http://author.example.com");
    assert_eq!(
        parse_code(&remote),
        Err(ConfigurationFailureCode::InsecureAuthorTransportNotAllowed)
    );
    let opted_in = remote.replace(
        "[environments.development.author]",
        "allow_insecure_author_transport = true\n\n[environments.development.author]",
    );
    let profile = Profile::parse(&opted_in).expect("the opt-in permits the address");
    let environment = environment_of(&profile, "development");
    assert!(environment.insecure_author_transport().is_some());
    assert!(environment.insecure_author_transport_warning().is_some());

    let cloud = read_fixture(PROFILE_DIRECTORY, "cloud-profile.toml");
    let cleartext_cloud = cloud.replace("https://author-p123", "http://author-p123");
    assert!(Profile::parse(&cleartext_cloud).is_err(), "Cloud accepted a cleartext author");
    let opted_in_cloud = cloud.replace(
        "[environments.production.author]",
        "allow_insecure_author_transport = true\n\n[environments.production.author]",
    );
    assert!(Profile::parse(&opted_in_cloud).is_err(), "Cloud accepted a meaningless opt-in");
}

#[test]
fn an_installation_publisher_may_be_cleartext_and_never_becomes_a_target() {
    let profile = Profile::parse(&read_fixture(PROFILE_DIRECTORY, "basic-profile.toml"))
        .expect("the documented Basic profile parses");
    let environment = environment_of(&profile, "development");
    assert!(!environment.publisher_metadata().is_protected(), "the fixture is not cleartext");

    let source = read_fixture(".", PROFILE_SOURCE);
    let targets: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("pub fn ") && line.contains("connection_target"))
        .collect();
    assert_eq!(
        targets,
        vec!["pub fn author_connection_target(&self) -> &TierBaseAddress {"],
        "another method yields a connection target"
    );
}

#[test]
fn no_parsed_profile_can_render_or_declare_a_plain_password() {
    let sentinel = "not-a-real-password";
    let profile = Profile::parse(&basic_document(sentinel)).expect("the profile parses");
    let rendered = format!("{profile:?}\n{profile:#?}");
    assert!(!rendered.contains(sentinel), "{rendered}");

    let source = read_fixture(".", PROFILE_SOURCE);
    let exposed: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|line| line.contains("password"))
        .filter(|line| line.contains("String") || line.contains("Vec<u8>") || line.contains("&str"))
        .filter(|line| !line.starts_with("///") && !line.starts_with("//!"))
        .collect();
    assert_eq!(
        exposed,
        vec!["password: Option<String>,"],
        "the password is reachable as plain bytes outside the transient document shape"
    );
}

#[test]
fn a_document_that_can_be_spelled_two_ways_is_refused() {
    let dotted = "format_version = 1\nname.first = \"site\"\n";
    assert!(Profile::parse(dotted).is_err(), "a dotted key was accepted");
    let literal = basic_document("admin").replace("password = \"admin\"", "password = 'admin'");
    assert!(Profile::parse(&literal).is_err(), "a literal string was accepted");
    let multiline =
        basic_document("admin").replace("password = \"admin\"", "password = \"\"\"admin\"\"\"");
    assert!(Profile::parse(&multiline).is_err(), "a multiline string was accepted");
    let unknown = format!("{}extra = 1\n", basic_document("admin"));
    assert!(Profile::parse(&unknown).is_err(), "an unknown member was accepted");
}

#[test]
fn the_selection_document_is_complete_or_refused() {
    let selection = SelectionDocument::parse(
        "format_version = 1\nprofile = \"cloud-site\"\nenvironment = \"production\"\n",
    )
    .expect("the documented selection parses");
    assert_eq!(selection.profile().as_text(), "cloud-site");
    assert_eq!(selection.environment().as_text(), "production");
    for partial in [
        "format_version = 1\nprofile = \"cloud-site\"\n",
        "format_version = 1\nenvironment = \"production\"\n",
        "profile = \"cloud-site\"\nenvironment = \"production\"\n",
    ] {
        assert!(SelectionDocument::parse(partial).is_err(), "{partial} was accepted");
    }
    assert!(
        SelectionDocument::parse(
            "format_version = 2\nprofile = \"cloud-site\"\nenvironment = \"production\"\n"
        )
        .is_err(),
        "another format version was accepted"
    );
}

#[test]
fn the_commit_inventory_accepts_only_a_sorted_distinct_listing() {
    let ordered = ConfigurationSnapshot::parse(&read_fixture(SNAPSHOT_DIRECTORY, "ordered.toml"))
        .expect("the ordered inventory parses");
    let references: Vec<&str> =
        ordered.sources().iter().map(|source| source.reference.as_text()).collect();
    assert_eq!(references, vec!["credentials/production.json", "profiles/cloud-site.toml"]);
    let distinct: BTreeSet<&str> = references.iter().copied().collect();
    assert_eq!(distinct.len(), references.len());
    let listed = ConfigurationReference::parse("profiles/cloud-site.toml").expect("valid");
    assert!(ordered.source(&listed).is_some());

    for name in [
        "unsorted.toml",
        "duplicate-reference.toml",
        "unknown-member.toml",
        "unsupported-format.toml",
        "empty-sources.toml",
        "escaping-reference.toml",
        "absolute-reference.toml",
        "backslash-reference.toml",
        "short-digest.toml",
        "uppercase-digest.toml",
    ] {
        let text = read_fixture(SNAPSHOT_DIRECTORY, name);
        assert!(ConfigurationSnapshot::parse(&text).is_err(), "{name} was accepted");
    }
}

#[test]
fn a_source_digest_compares_but_never_renders() {
    let ordered = ConfigurationSnapshot::parse(&read_fixture(SNAPSHOT_DIRECTORY, "ordered.toml"))
        .expect("the ordered inventory parses");
    let sources = ordered.sources();
    assert!(sources[0].digest.matches(&sources[0].digest));
    assert!(!sources[0].digest.matches(&sources[1].digest));
    let rendered = format!("{:?}", sources[0].digest);
    assert!(!rendered.contains("0123456789"), "{rendered}");
}

/// Encodes bytes with the standard base64 alphabet, independently of the
/// workspace's encoder, so the fixture's column is checked rather than trusted.
fn encode_base64(bytes: &[u8]) -> String {
    /// The standard alphabet, in code order.
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    /// Input bytes one quantum consumes.
    const GROUP: usize = 3;
    /// Output characters one quantum produces.
    const QUANTUM: usize = 4;
    /// Bits one output character carries.
    const BITS: u32 = 6;
    /// Mask selecting those bits.
    const MASK: u32 = 0x3F;
    let mut encoded = String::new();
    for group in bytes.chunks(GROUP) {
        let mut accumulator = 0_u32;
        for index in 0..GROUP {
            accumulator <<= u8::BITS;
            accumulator |= u32::from(group.get(index).copied().unwrap_or(0));
        }
        let produced = group.len() + 1;
        for index in 0..QUANTUM {
            if index >= produced {
                encoded.push('=');
                continue;
            }
            let shift = u32::try_from(QUANTUM - 1 - index).expect("the index fits") * BITS;
            let code = usize::try_from((accumulator >> shift) & MASK).expect("the code fits");
            encoded.push(char::from(ALPHABET[code]));
        }
    }
    encoded
}
