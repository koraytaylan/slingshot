//! Who is in a group, and how they got there.
//!
//! Every permission question turns into a membership question, and answering it
//! needs the difference between a direct member and one that arrives through
//! another group. A listing that flattened the two would answer "why does this
//! person have access" with a list that does not contain the reason.
//!
//! So each row says whether the membership is direct, and a request that asked
//! for direct members only is answered with direct members only.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use crate::command::authorizable_identity::{AuthorizableIdentifier, AuthorizableKind};
use crate::command::operational_listing::{ListingResultFailure, require_strictly_ascending_text};
use crate::command::repository_path::RepositoryPath;
use crate::command::result_window::{ContinuationToken, ResultWindow};

/// One request to list a group's members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListGroupMembersCommand {
    /// Group whose members are listed.
    pub group_identifier: AuthorizableIdentifier,
    /// Whether members that arrive through another group are reported.
    pub include_indirect: bool,
    /// Page the caller is asking for, when the caller said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_window: Option<ResultWindow>,
}

impl ListGroupMembersCommand {
    /// Returns the page this request asks for, stated or resolved.
    #[must_use]
    pub fn resolved_window(&self) -> ResultWindow {
        self.result_window.clone().unwrap_or_default()
    }

    /// Returns whether a membership so arrived is one this request asked about.
    #[must_use]
    pub fn admits(&self, direct: bool) -> bool {
        direct || self.include_indirect
    }
}

/// One member of a group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupMemberMatch {
    /// Identifier the member answers to.
    pub authorizable_identifier: AuthorizableIdentifier,
    /// Whether the membership is held on this group itself.
    pub direct: bool,
    /// What kind of authorizable it is.
    pub kind: AuthorizableKind,
    /// Where the author keeps it.
    pub repository_path: RepositoryPath,
}

/// One page of group members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListGroupMembersResult {
    /// Matches, strictly ascending by member identifier bytes.
    pub matches: Vec<GroupMemberMatch>,
    /// Where the next page resumes, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<ContinuationToken>,
}

impl ListGroupMembersResult {
    /// Returns the page these matches describe.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotStrictlyAscending`] when an identifier
    /// repeats or sorts before its predecessor.
    pub fn new(
        matches: Vec<GroupMemberMatch>,
        next_continuation_token: Option<ContinuationToken>,
    ) -> Result<Self, ListingResultFailure> {
        require_strictly_ascending_text(
            matches.iter().map(|found| found.authorizable_identifier.as_text()),
        )?;
        Ok(Self { matches, next_continuation_token })
    }

    /// Requires this page to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when an indirect member
    /// appears in an answer to a request that asked for direct members only.
    pub fn require_answers(
        &self,
        command: &ListGroupMembersCommand,
    ) -> Result<(), ListingResultFailure> {
        let admitted = self.matches.iter().all(|found| command.admits(found.direct));
        if admitted { Ok(()) } else { Err(ListingResultFailure::NotThisRequest) }
    }
}

/// Why a group's members could not be listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListGroupMembersFailure {
    /// Nothing answers to that identifier.
    GroupNotFound,
    /// Something answers to it and it is a user.
    AuthorizableKindMismatch,
    /// This caller may not read its members.
    AuthorizableAccessDenied,
}

/// One refused member listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListGroupMembersRefusal {
    /// Why it was refused.
    pub failure: ListGroupMembersFailure,
    /// Group this request named.
    pub group_identifier: AuthorizableIdentifier,
}

impl ListGroupMembersRefusal {
    /// Requires this refusal to answer `command`.
    ///
    /// # Errors
    ///
    /// Returns [`ListingResultFailure::NotThisRequest`] when it names another
    /// request's group.
    pub fn require_answers(
        &self,
        command: &ListGroupMembersCommand,
    ) -> Result<(), ListingResultFailure> {
        if self.group_identifier == command.group_identifier {
            Ok(())
        } else {
            Err(ListingResultFailure::NotThisRequest)
        }
    }
}

/// One page exactly as it is written on the wire.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultDocument {
    /// Matches this page carries.
    matches: Vec<GroupMemberMatch>,
    /// Where the next page resumes.
    #[serde(default)]
    next_continuation_token: Option<ContinuationToken>,
}

impl<'de> Deserialize<'de> for ListGroupMembersResult {
    fn deserialize<Source: serde::Deserializer<'de>>(
        deserializer: Source,
    ) -> Result<Self, Source::Error> {
        let document = ResultDocument::deserialize(deserializer)?;
        Self::new(document.matches, document.next_continuation_token).map_err(Source::Error::custom)
    }
}
