//! Resolution of the configuration root from the operating-system account.
//!
//! `~/.config/slingshot` has one non-shell meaning here. The tilde is the home
//! directory the operating-system account database names for the account this
//! process actually runs as, never a variable a caller can set: `HOME`,
//! `XDG_CONFIG_HOME`, `USERPROFILE`, `HOMEDRIVE`, `HOMEPATH`, and the working
//! directory are all ignored. Anyone who could set one of those could otherwise
//! point Slingshot at a configuration root they control and choose which
//! credentials it reads.
//!
//! The same account identity that answers that question is the identity every
//! later ownership check compares against, and it is sampled exactly once. A
//! resolver that answered as one account and then let a file be verified
//! against another would defeat the check entirely.

use std::path::{Path, PathBuf};

use slingshot_domain::profile_authentication_contract::{
    ConfigurationFailureCode, ProfileAuthenticationContract,
};

/// Reason the configuration root could not be resolved.
///
/// The failure carries the contract's stable code and a structural location
/// from the manifest's vocabulary, never the path, the account, or the
/// operating system's own message.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code} at {structural_location}")]
pub struct ConfigurationRootFailure {
    /// Stable code from the contract registry.
    pub code: ConfigurationFailureCode,
    /// Manifest vocabulary naming where the failure was found.
    pub structural_location: &'static str,
}

impl ConfigurationRootFailure {
    /// Returns one failure at a named structural location.
    #[must_use]
    pub fn at(code: ConfigurationFailureCode, structural_location: &'static str) -> Self {
        Self { code, structural_location }
    }
}

/// Structural location every root failure is reported at.
const ROOT_LOCATION: &str = "configuration_root";

/// The account this process runs as.
///
/// The value is opaque to everything but the platform policy that compares it
/// with a file's owner, and it is what makes "this account owns it" a single
/// decision rather than one per file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountIdentity {
    /// Effective user identifier of a Unix account.
    UnixUser(u32),
    /// Security identifier of a Windows process-token user.
    WindowsUser(String),
}

/// One answer from the operating-system account database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountProfile {
    /// Account the answer was resolved for.
    pub identity: AccountIdentity,
    /// Home directory the account database named for it.
    pub home: PathBuf,
}

/// Answers which account this process runs as and where its home directory is.
///
/// The trait exists so every platform rule can be proved deterministically. A
/// test supplies an answer this machine cannot produce - an ambiguous account,
/// a relative home, a home that is not Unicode - and the resolution above it is
/// exercised without needing that platform or that account.
pub trait AccountResolver {
    /// Returns the account and the home directory the database names for it.
    ///
    /// # Errors
    ///
    /// Returns the contract code for an unavailable account database, a missing
    /// or ambiguous home, or a platform this build does not support.
    fn resolve(&self) -> Result<AccountProfile, ConfigurationRootFailure>;
}

/// The resolved configuration root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationRoot {
    /// Account sampled once, for this root and every later ownership check.
    identity: AccountIdentity,
    /// Absolute path of the root itself.
    path: PathBuf,
    /// Absolute path the traversal to the root starts from.
    home: PathBuf,
}

impl ConfigurationRoot {
    /// Resolves the root from the account `resolver` names.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationFailureCode::ConfigurationHomeUnavailable`] for an
    /// empty home, [`ConfigurationFailureCode::ConfigurationHomeNotAbsolute`]
    /// for a relative one, and
    /// [`ConfigurationFailureCode::ConfigurationHomeNotUnicode`] for one whose
    /// bytes are not text, in addition to whatever the resolver itself reports.
    pub fn resolve(resolver: &dyn AccountResolver) -> Result<Self, ConfigurationRootFailure> {
        let profile = resolver.resolve()?;
        let home = profile.home;
        if home.as_os_str().is_empty() {
            return Err(ConfigurationRootFailure::at(
                ConfigurationFailureCode::ConfigurationHomeUnavailable,
                ROOT_LOCATION,
            ));
        }
        if home.to_str().is_none() {
            return Err(ConfigurationRootFailure::at(
                ConfigurationFailureCode::ConfigurationHomeNotUnicode,
                ROOT_LOCATION,
            ));
        }
        if !home.is_absolute() {
            return Err(ConfigurationRootFailure::at(
                ConfigurationFailureCode::ConfigurationHomeNotAbsolute,
                ROOT_LOCATION,
            ));
        }
        let mut path = home.clone();
        for component in Self::root_components() {
            path.push(component);
        }
        Ok(Self { identity: profile.identity, path, home })
    }

    /// Returns a root at an explicit path, for a test that supplies its tree.
    ///
    /// Production resolves a root only through [`ConfigurationRoot::resolve`];
    /// a scan in this crate's tests proves no module here calls this.
    #[must_use]
    pub fn at_explicit_path(identity: AccountIdentity, path: PathBuf) -> Self {
        Self { identity, home: path.clone(), path }
    }

    /// Returns the components appended to the home directory.
    #[must_use]
    pub fn root_components() -> &'static [String] {
        &ProfileAuthenticationContract::embedded().literals.configuration_root_components
    }

    /// Returns the account sampled for this root.
    #[must_use]
    pub fn identity(&self) -> &AccountIdentity {
        &self.identity
    }

    /// Returns the absolute path of the root.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the absolute path the traversal to the root starts from.
    ///
    /// The root is reached by opening each component below this directory in
    /// turn, so a link planted at any of them can be refused rather than
    /// followed.
    #[must_use]
    pub fn traversal_origin(&self) -> &Path {
        &self.home
    }

    /// Returns the directory that holds profile documents.
    #[must_use]
    pub fn profile_directory(&self) -> PathBuf {
        self.path.join(&ProfileAuthenticationContract::embedded().literals.profile_directory_name)
    }

    /// Returns the optional document that supplies the default selection.
    #[must_use]
    pub fn selection_file(&self) -> PathBuf {
        self.path.join(&ProfileAuthenticationContract::embedded().literals.selection_file_name)
    }

    /// Returns the document that publishes the commit inventory.
    #[must_use]
    pub fn commit_inventory_file(&self) -> PathBuf {
        self.path.join(
            &ProfileAuthenticationContract::embedded().literals.configuration_snapshot_file_name,
        )
    }
}

/// The account resolver this build uses in production.
///
/// It is a unit type rather than a value so nothing can configure it. Every
/// input it has comes from the operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatingSystemAccountResolver;

#[cfg(unix)]
impl AccountResolver for OperatingSystemAccountResolver {
    fn resolve(&self) -> Result<AccountProfile, ConfigurationRootFailure> {
        use uzers::os::unix::UserExt;

        let identifier = uzers::get_effective_uid();
        let account = uzers::get_user_by_uid(identifier).ok_or_else(|| {
            ConfigurationRootFailure::at(
                ConfigurationFailureCode::ConfigurationAccountUnavailable,
                ROOT_LOCATION,
            )
        })?;
        Ok(AccountProfile {
            identity: AccountIdentity::UnixUser(identifier),
            home: account.home_dir().to_path_buf(),
        })
    }
}

#[cfg(windows)]
impl AccountResolver for OperatingSystemAccountResolver {
    fn resolve(&self) -> Result<AccountProfile, ConfigurationRootFailure> {
        use winsafe::prelude::*;
        use winsafe::{HPROCESS, TokenInfo, co};

        let unavailable = || {
            ConfigurationRootFailure::at(
                ConfigurationFailureCode::ConfigurationAccountUnavailable,
                ROOT_LOCATION,
            )
        };
        let token = HPROCESS::GetCurrentProcess()
            .OpenProcessToken(co::TOKEN::QUERY)
            .map_err(|_| unavailable())?;
        let information = token
            .GetTokenInformation(co::TOKEN_INFORMATION_CLASS::User)
            .map_err(|_| unavailable())?;
        let TokenInfo::User(user) = information else {
            return Err(unavailable());
        };
        let identifier = user.User.Sid().map_err(|_| unavailable())?.to_string();
        let home = winsafe::SHGetKnownFolderPath(
            &co::KNOWNFOLDERID::Profile,
            co::KF::DEFAULT,
            Some(&token),
        )
        .map_err(|_| {
            ConfigurationRootFailure::at(
                ConfigurationFailureCode::ConfigurationHomeUnavailable,
                ROOT_LOCATION,
            )
        })?;
        Ok(AccountProfile {
            identity: AccountIdentity::WindowsUser(identifier),
            home: PathBuf::from(home),
        })
    }
}

#[cfg(not(any(unix, windows)))]
impl AccountResolver for OperatingSystemAccountResolver {
    fn resolve(&self) -> Result<AccountProfile, ConfigurationRootFailure> {
        Err(ConfigurationRootFailure::at(
            ConfigurationFailureCode::UnsupportedPlatform,
            ROOT_LOCATION,
        ))
    }
}
