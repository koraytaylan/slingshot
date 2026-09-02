---
id: resolve-credential-paths
title: "Resolve Credential Paths"
workstream: "0006"
kind: task
depends_on:
  - define-profile-documents
gated: false
touches:
  - crates/slingshot-configuration/src/configuration_root.rs
  - crates/slingshot-configuration/src/credential_path.rs
  - crates/slingshot-configuration/tests/configuration_root.rs
  - crates/slingshot-configuration/tests/credential_paths.rs
  - "crates/slingshot-configuration/tests/fixtures/configuration-root/**"
status: done
merged_as: "da98cc997a2bc36b271ccf3cb9fc728d8b9b026e"
---
# Resolve Credential Paths

Resolve the canonical product root from the sampled operating-system account, then resolve Cloud credential and optional additional certificate-authority files with one portable grammar that cannot escape it.

**Steps:**

1. Add deterministic account-root policy fixtures for every exact Plan 0001 supported row covering Linux/macOS effective versus real users and account-database homes; Windows current-token `FOLDERID_Profile`; populated trap values for `HOME`, `XDG_CONFIG_HOME`, `USERPROFILE`, `HOMEDRIVE`, and `HOMEPATH`; and absolute, relative, empty, non-Unicode, unavailable, ambiguous, wrong-user, and operating-system-failure results. Add a separately labelled current-environment native observation when the environment matches one supported row.
2. Implement `ConfigurationRootResolver` by sampling the same current account identity used by filesystem ownership policy exactly once. On Linux/macOS query that effective user's operating-system account-database home; on Windows query `FOLDERID_Profile` for the sampled process-token user. Require one nonempty absolute Unicode native path, ignore ambient environment and working-directory state, append literal `.config` and `slingshot`, and obtain the root by absolute no-follow directory-handle traversal. Expose only an injected fake account resolver and explicit test-root source to tests; production accepts no override.
3. Add reference path fixtures for normal descendants, absolute paths, parent traversal, empty components, platform prefixes, mixed separators, and misleading extensions. Implement lexical parsing and root-handle-relative resolution for credential and certificate references without consulting the process working directory or profile directory.
4. Add property tests showing every accepted reference remains a descendant after normalization and every rejected root/reference form maps to the exact content-free `ConfigurationDiagnostic` source class/stage/manifest structural location/code/occurrence shape.

**Tests:**

- `configuration_root` proves exact Linux/macOS account-database and Windows current-token known-folder selection, effective-user behavior, literal `.config/slingshot` append, environment-variable ignorance, and stable failure categories through deterministic fakes for every supported row. At most the one row matching the current environment executes a native lookup and labels its report untrusted; absence of another host is not a Plan 0002 failure and no current run claims another row.
- `credential_paths` exercises the complete fixture table for credential and certificate-authority references on supported platforms.
- A generated component test proves accepted paths cannot normalize outside the configured root.
- Relative/empty/non-Unicode/unavailable/ambiguous account-profile results and a no-follow root-component failure open no profile, credential, or certificate document.

- **Done when:** `cargo test -p slingshot-configuration --test configuration_root` and `cargo test -p slingshot-configuration --test credential_paths` prove every supported-row policy deterministically, derive `~/.config/slingshot` only from the sampled operating-system account with no ambient override, prevent every accepted credential path from escaping its verified root handle, and keep any current-row native observation explicitly untrusted pending Plan 0009's authenticated aggregate release evidence.
