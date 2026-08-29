# Configuration

Slingshot reads one configuration root, `~/.config/slingshot`, and turns it into
one selected environment. This describes what that root contains, what each
value means, and which rules the loader will not bend.

## Where the root is

The tilde is not a shell expansion. It is the home directory the operating-system
account database names for the account this process actually runs as: the
effective user's account entry on Linux and macOS, the profile known folder of
the process token on Windows. `HOME`, `XDG_CONFIG_HOME`, `USERPROFILE`,
`HOMEDRIVE`, `HOMEPATH`, and the working directory are all ignored.

That is a security rule rather than a preference. If the root came from an
environment variable, anyone who could set one could point Slingshot at a
configuration they wrote and choose which credentials it reads and which server
it sends them to.

The same account identity answers a second question: every file below the root
must be owned by that account, readable by nobody else, and reachable by no
second name. A file with a second hard link is refused even when both names have
the same owner, because the second name is a way to rewrite an accepted file
from outside the verified root.

## What the root contains

```
~/.config/slingshot/
  configuration-snapshot.toml
  selection.toml            (optional)
  profiles/
    <name>.toml
  credentials/              (referenced by profiles)
  certificates/             (referenced by profiles)
```

Every file below `profiles/` defines exactly one profile. A profile's name comes
from the document, not from the file name, so renaming a file does not rename a
profile and two files may not declare one name.

## Profiles

```toml
format_version = 1
name = "local-site"

[environments.development]
deployment = "adobe_experience_manager_6_5"

[environments.development.author]
base_address = "http://localhost:4502"

[environments.development.publisher]
base_address = "http://localhost:4503"

[environments.development.authentication]
method = "basic"
user_name = "admin"
password = "admin"
```

```toml
format_version = 1
name = "cloud-site"

[environments.production]
deployment = "adobe_experience_manager_cloud_service"
additional_ca_certificate_file = "certificates/corporate-ca.pem"

[environments.production.author]
base_address = "https://author-p123-e456.adobeaemcloud.com"

[environments.production.publisher]
base_address = "https://publish-p123-e456.adobeaemcloud.com"

[environments.production.authentication]
method = "adobe_experience_manager_developer_console_service_credentials"
credentials_file = "credentials/production.json"
```

Those two are the only legal combinations. Basic credentials go with Adobe
Experience Manager 6.5; Developer Console service credentials go with Cloud
Service. Every other pairing is refused when the document is read.

Documents are TOML 1.0.0 with one-line basic strings. A literal string, a
multiline string, and a dotted key are all refused, because each lets one value
be written several ways.

## Addresses

An author or publisher address is an origin plus either the root or one absolute
context path. It has exactly one spelling: the scheme and host fold to
lowercase, a port equal to the scheme's default is refused rather than dropped,
percent escapes are uppercase and never spell a byte that did not need escaping,
and the prefix has one leading separator and no trailing one.

Anything ambiguous is an error rather than something to normalize: user
information, a query, a fragment, an empty or dot segment, an encoded separator,
a backslash, a bare non-ASCII byte. Normalizing any of those away is how one
server ends up written two ways and holding two identities.

Appending an endpoint extends the context path. Each segment is encoded on its
own, so `https://author.example.com/context` plus `bin` and `querybuilder.json`
is `https://author.example.com/context/bin/querybuilder.json`, and a segment can
never start a new absolute path or climb out.

## Transport

Cloud Service requires a protected transport on both addresses.

Adobe Experience Manager 6.5 allows a cleartext author on exactly `localhost`,
`127.0.0.1`, and `[::1]`. Anywhere else, a cleartext author requires the
environment to say so:

```toml
allow_insecure_author_transport = true
```

That field is legal only for the 6.5 Basic combination and only where it is
needed. An explicit `false` is refused because it says nothing the absence does
not, and the field beside a protected or loopback address is refused because it
claims a risk that is not being taken. Where it is used, the selection carries a
stable warning that configuration checking and connection setup both report.

A 6.5 publisher address may be cleartext with no opt-in, because nothing dials a
publisher. The publisher is metadata for reporting and command semantics; there
is no publisher client to build.

## Selection

An optional `selection.toml` supplies the default pair:

```toml
format_version = 1
profile = "cloud-site"
environment = "production"
```

Selection is never first-found. Either a command names both a profile and an
environment, or the root names both. Naming only one is refused even when the
root could complete it, because completing it from another source is how a
command ends up aimed at a server nobody chose.

## One committed generation

`configuration-snapshot.toml` is required. It lists every profile, the optional
selection, and every credential and certificate those profiles reach, each with
the exact digest of its bytes:

```toml
format_version = 1

[[sources]]
reference = "credentials/production.json"
sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[[sources]]
reference = "profiles/cloud-site.toml"
sha256 = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
```

A writer synchronizes and atomically replaces every source first, then publishes
this file last. Startup reads it, reads every source it lists, checks each
digest, requires the set found on disk and referenced from the profiles to equal
the set listed exactly, and reads it again to require it unchanged. Two whole
attempts are made, so a writer that finishes between them is seen complete.

Reading one file twice proves that file did not change while it was read. It
does not prove the set is one generation - a writer can replace a source between
two perfectly stable reads - which is what this manifest is for.

## Credentials

A Cloud environment names the JSON downloaded from the Adobe Experience Manager
Developer Console. Two of its values are easy to confuse and expensive to
confuse: `technicalAccount.clientId` is the client identity, and `integration.id`
is the technical account. They have different bounds, and swapping them would
authenticate as somebody else.

The deprecated Adobe Developer Console Service Account credential is recognized
and named as a different product rather than reported as a malformed file.

A password, a private key, a client secret, an assertion, and an access token
are all redacted from the moment they are read. No diagnostic carries one, and
none carries a digest of one either: a digest of a low-entropy password is as
good as the password.

## Trust

Startup takes one snapshot of the platform's server-authentication roots and
keeps it. A root is retained only when the platform says unconditionally that it
may authenticate a server; a denied, restricted, or uninterpretable record fails
the snapshot rather than being reduced to bytes.

`additional_ca_certificate_file` extends author trust only. It cannot extend the
trust used to reach Adobe Identity Management Services, which is where the
credentials go, and the two are separate types rather than one list with a flag.

Both routes connect directly. `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`,
`NO_PROXY`, and their lowercase spellings are ignored.

## What a restart changes

The snapshot is taken once. Editing a profile, rotating a credential, or
changing platform trust affects a running daemon not at all; it takes effect on
an explicit restart.

Rotating a password, a private key, a client secret, or a certificate without
changing who the credential belongs to leaves the target identity and the
revision exactly as they were, so work partitioned by target survives the
rotation. Changing the principal or the author address moves the target.
Changing the authorization scope or either route's effective trust changes the
revision and leaves the target where it is.
