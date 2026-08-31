//! Turning option text into the values the domain declares.
//!
//! Ten families read the same handful of shapes off a command line: an address,
//! a flag, a reference policy, a removal list, a document. Each family parsing
//! them itself would give ten chances to accept a spelling the domain refuses, or
//! to refuse one it accepts. They are read once here and handed over as the
//! domain's own values.
//!
//! Nothing here decides what a value means. Every one of these functions either
//! returns what a validated constructor built or names the option that carried
//! something the constructor would not take.

use slingshot_domain::command::find_pages_containing_phrase::PageTitle;
use slingshot_domain::command::repository_path::{PropertyName, RepositoryPath};
use slingshot_domain::command::resource_mutation::{ReferencePolicy, RemovedPropertyNames};

use crate::commands::content::{RequestRefusal, required};
use crate::commands::package::LIST_SEPARATOR;
use crate::invocation::{
    Invocation, REFERENCE_POLICY_OPTION, REMOVED_PROPERTIES_OPTION, TITLE_OPTION,
};

/// The spelling that refuses a deletion while anything points at its subject.
pub const REFUSE_WHEN_REFERENCED: &str = "refuse-when-referenced";

/// The spelling that removes a subject whatever points at it.
pub const IGNORE_REFERENCES: &str = "ignore-references";

/// Returns the refusal that names `option` as unusable.
#[must_use]
pub fn unusable(option: &str) -> RequestRefusal {
    RequestRefusal::ValueUnusable { named: option.to_owned() }
}

/// Returns the address `option` carries.
///
/// # Errors
///
/// Returns [`RequestRefusal::OptionMissing`] when the option is absent and
/// [`RequestRefusal::ValueUnusable`] when its value is not an address.
pub fn path(invocation: &Invocation, option: &str) -> Result<RepositoryPath, RequestRefusal> {
    RepositoryPath::parse(required(invocation, option)?).map_err(|_| unusable(option))
}

/// Returns the address `option` carries, when it carries one.
///
/// # Errors
///
/// Returns [`RequestRefusal::ValueUnusable`] when the value is not an address.
pub fn optional_path(
    invocation: &Invocation,
    option: &str,
) -> Result<Option<RepositoryPath>, RequestRefusal> {
    invocation
        .arguments
        .get(option)
        .map(|stated| RepositoryPath::parse(stated).map_err(|_| unusable(option)))
        .transpose()
}

/// Returns the text `option` carries, when it carries any.
#[must_use]
pub fn optional_text(invocation: &Invocation, option: &str) -> Option<String> {
    invocation.arguments.get(option).cloned()
}

/// Returns whether `option` was given at all.
#[must_use]
pub fn flag(invocation: &Invocation, option: &str) -> bool {
    invocation.arguments.contains_key(option)
}

/// Returns the reference policy this invocation states.
///
/// There is no default. A caller that has not said what happens to the
/// references has not made the decision, and making it for them is how content
/// disappears from somewhere nobody was looking.
///
/// # Errors
///
/// Returns [`RequestRefusal::OptionMissing`] when the option is absent and
/// [`RequestRefusal::ValueUnusable`] when it carries another spelling.
pub fn reference_policy(invocation: &Invocation) -> Result<ReferencePolicy, RequestRefusal> {
    match required(invocation, REFERENCE_POLICY_OPTION)? {
        REFUSE_WHEN_REFERENCED => Ok(ReferencePolicy::RefuseWhenReferenced),
        IGNORE_REFERENCES => Ok(ReferencePolicy::IgnoreReferences),
        _ => Err(unusable(REFERENCE_POLICY_OPTION)),
    }
}

/// Returns the removal list this invocation carries, when it carries one.
///
/// # Errors
///
/// Returns [`RequestRefusal::ValueUnusable`] when a name is not a property name
/// or the list is not the ascending distinct set the domain requires.
pub fn removed_property_names(
    invocation: &Invocation,
) -> Result<Option<RemovedPropertyNames>, RequestRefusal> {
    let Some(stated) = invocation.arguments.get(REMOVED_PROPERTIES_OPTION) else {
        return Ok(None);
    };
    let names = stated
        .split(LIST_SEPARATOR)
        .map(|name| PropertyName::parse(name).map_err(|_| unusable(REMOVED_PROPERTIES_OPTION)))
        .collect::<Result<Vec<PropertyName>, RequestRefusal>>()?;
    RemovedPropertyNames::new(names).map(Some).map_err(|_| unusable(REMOVED_PROPERTIES_OPTION))
}

/// Returns the document `option` carries, read as the value `Target` declares.
///
/// The command line carries a document as one JSON value rather than as a
/// grammar of its own, because the domain already declares what that document
/// is and a second grammar here would be a second thing to disagree with it.
///
/// # Errors
///
/// Returns [`RequestRefusal::OptionMissing`] when the option is absent and
/// [`RequestRefusal::ValueUnusable`] when the value is not that document.
pub fn document<Target: serde::de::DeserializeOwned>(
    invocation: &Invocation,
    option: &str,
) -> Result<Target, RequestRefusal> {
    serde_json::from_str(required(invocation, option)?).map_err(|_| unusable(option))
}

/// Returns the document `option` carries, when it carries one.
///
/// # Errors
///
/// Returns [`RequestRefusal::ValueUnusable`] when the value is not that
/// document.
pub fn optional_document<Target: serde::de::DeserializeOwned>(
    invocation: &Invocation,
    option: &str,
) -> Result<Option<Target>, RequestRefusal> {
    invocation
        .arguments
        .get(option)
        .map(|stated| serde_json::from_str(stated).map_err(|_| unusable(option)))
        .transpose()
}

/// Returns the list `option` carries, read as the values `Target` declares.
///
/// # Errors
///
/// Returns [`RequestRefusal::OptionMissing`] when the option is absent and
/// [`RequestRefusal::ValueUnusable`] when a member is not one of those values.
pub fn list<Target: serde::de::DeserializeOwned>(
    invocation: &Invocation,
    option: &str,
) -> Result<Vec<Target>, RequestRefusal> {
    required(invocation, option)?
        .split(LIST_SEPARATOR)
        .map(|member| serde_json::from_str(&format!("\"{member}\"")).map_err(|_| unusable(option)))
        .collect()
}

/// Returns the whole number `option` carries, when it carries one.
///
/// # Errors
///
/// Returns [`RequestRefusal::ValueUnusable`] when the value is not one.
pub fn optional_count(
    invocation: &Invocation,
    option: &str,
) -> Result<Option<u64>, RequestRefusal> {
    invocation
        .arguments
        .get(option)
        .map(|stated| stated.parse::<u64>().map_err(|_| unusable(option)))
        .transpose()
}

/// Returns the decision `option` carries as one of two spellings.
///
/// # Errors
///
/// Returns [`RequestRefusal::OptionMissing`] when the option is absent and
/// [`RequestRefusal::ValueUnusable`] when it carries a third spelling.
pub fn decision(
    invocation: &Invocation,
    option: &str,
    affirmative: &str,
    negative: &str,
) -> Result<bool, RequestRefusal> {
    let stated = required(invocation, option)?;
    if stated == affirmative {
        Ok(true)
    } else if stated == negative {
        Ok(false)
    } else {
        Err(unusable(option))
    }
}

/// Returns the title one invocation records, when it records one.
///
/// Four families record a title and every one of them reads it the same way, so
/// it is read here rather than four times.
///
/// # Errors
///
/// Returns [`RequestRefusal::ValueUnusable`] when the value is longer than a
/// title may be.
pub fn title(invocation: &Invocation) -> Result<Option<PageTitle>, RequestRefusal> {
    optional_text(invocation, TITLE_OPTION)
        .map(|stated| PageTitle::new(stated).map_err(|_| unusable(TITLE_OPTION)))
        .transpose()
}
