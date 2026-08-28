# Plan 0002 — Profiles and Authentication

> Turn files below `~/.config/slingshot` into deterministic, typed profile and environment selections whose secrets cross one audited authentication boundary.

## Why this plan

Every command needs an unambiguous author environment before the daemon can open a connection. The configuration also carries two materially different secret forms: plain Basic credentials for Adobe Experience Manager 6.5 installations and environment-scoped service-credentials JSON downloaded from the Adobe Experience Manager Developer Console for Cloud Service. Treating those forms as interchangeable would permit an invalid deployment combination, leak secrets through diagnostics, or send traffic to a publisher that Slingshot is required never to dial.

Adobe documents that Adobe Experience Manager Developer Console service credentials contain the material used to create a JSON Web Token assertion and exchange it with Adobe Identity Management Services for an access token: [Adobe Experience Manager service credentials](https://experienceleague.adobe.com/en/docs/experience-manager-learn/getting-started-with-aem-headless/authentication/service-credentials). Those credentials are distinct from the deprecated Service Account (JSON Web Token) credentials issued by the Adobe Developer Console; Adobe explicitly states that the Adobe Experience Manager Developer Console credentials are not deprecated: [JSON Web Token credentials deprecation in Adobe Developer Console](https://experienceleague.adobe.com/en/docs/experience-manager-cloud-service/content/security/jwt-credentials-deprecation-in-adobe-developer-console).

The supported downloaded document is the Adobe-documented success shape with `ok`, `statusCode`, the closed `integration` object, and its public certificate as well as the fields consumed during exchange. Cloud credentials establish identity, not authorization. Adobe Experience Manager grants the generated technical-account user read access by default; an operator separately grants the least repository and agent-route privileges required by mutating Slingshot commands. Authentication success and static configuration checking never claim those permissions exist.

## In scope

- **0005 — Profile Documents.** Define exact profile/selection/generation TOML, service-credential depth charging, normalized context-path addresses, Basic transport warnings, constructible contract-bound principal/target/revision digests, canonical metascopes, and distinct immutable identity-management and author server-authentication trust identities.
- **0006 — Credential File Boundary.** Resolve literal `~/.config/slingshot` from the operating-system account with no ambient override; stable-read every root-contained source under exact platform ownership/access rules; accept truthful per-file old-or-new bytes only as one before/after-manifest-verified committed generation; snapshot platform trust; extend trust only for the selected author; define direct transport; and keep secrets/private source digests redacted.
- **0007 — Cloud Token Exchange.** Parse the Adobe Experience Manager Developer Console service-credential authority and organization/client/technical-account tuple, construct one exact compact `RS256` assertion, restrict one `POST` form exchange to the fixed HTTPS endpoint and immutable trust snapshot, enforce exact decoded-head/body/deadline charges, and accept only a conservative usable token lease.
- **0008 — Authentication Provider.** Snapshot profile, credential, and certificate material once at daemon startup, put Basic and Cloud authentication behind one author-only provider, cache Cloud access tokens in memory with scheduled and race-safe conditional unauthorized single-flight refresh, and prove that publisher addresses and secret values never reach an outbound Adobe Experience Manager request.

## Out of scope

Operating-system keychains, interactive browser login, local development access-token files, ambient or configured outbound proxies, client certificates, Adobe Developer Console OAuth Server-to-Server projects, and deprecated Adobe Developer Console Service Account credentials are not configuration variants in this plan. The daemon lifecycle, local command transport, Sling Job submission, and server-sent event connection belong to later plans. Publisher addresses are retained as environment metadata for command semantics and reporting, but no publisher connection is created.

## Plan dependencies

Plan 0001 supplies the workspace crates, structural module-family roots, and probe-frozen safe native APIs for descriptor-relative traversal, Linux/macOS access-control-list evidence, and Windows Security-Identifier/security-descriptor/reparse/file-identity evidence. Plan 0002 begins with one exact compiling feature-leaf scaffold under those roots; every behavioral task descends from it and therefore never relies on an out-of-footprint parent-module edit. Workstream 0006 consumes the native baseline and does not introduce an unprobed manifest, lockfile, unsafe wrapper, or second supported-platform policy. Plan 0002 proves every abstract row with deterministic policy fakes and may execute only the one native row matching the current environment, whose report is explicitly untrusted and cannot claim another row. Plan 0009 alone owns provider mapping and authenticated aggregate native release evidence. Later daemon and transport plans consume the immutable selected-environment snapshot and authentication provider defined here.

## Configuration contract

The product spelling `~/.config/slingshot` has one non-shell meaning. On Linux and macOS, `~` is the absolute home directory returned by the operating-system account database for the process's once-sampled effective user identifier. On Windows it is the absolute `FOLDERID_Profile` directory resolved for the once-sampled current process-token user. Slingshot ignores `HOME`, `XDG_CONFIG_HOME`, `USERPROFILE`, `HOMEDRIVE`, `HOMEPATH`, and working-directory state, appends the literal `.config/slingshot` components, and opens the result through absolute directory handles. Missing, ambiguous, relative, empty, or non-Unicode account-profile results fail with a stable configuration-root error before any profile or credential read. Each ordinary file directly below that root's `profiles/` directory defines exactly one profile. Profile and environment names are lowercase kebab identifiers and are independent of filenames. The only accepted deployment and authentication pairs are shown here:

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

An optional `~/.config/slingshot/selection.toml` supplies the complete default pair; omitting it requires both names on every daemon-backed invocation, and specifying only one name is invalid:

```toml
format_version = 1
profile = "cloud-site"
environment = "production"
```

Every startup also requires private `~/.config/slingshot/configuration-snapshot.toml`. Its strictly sorted source inventory contains the exact SHA-256 digest of every profile, optional selection, and transitively referenced credential/certificate file. Writers synchronize and atomically replace source files first and publish this manifest last. The loader stable-reads the manifest before and after all listed sources and accepts only an unchanged, exact, complete inventory. For example:

```toml
format_version = 1

[[sources]]
reference = "credentials/production.json"
sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[[sources]]
reference = "profiles/cloud-site.toml"
sha256 = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
```

The snapshot manifest and every listed source use the same root-contained verified-handle authority; traversal, symbolic links/reparse points, a final-file link count other than one, unsafe ownership/effective permissions, nonregular files, and escape are invalid. Rejecting a second hard-link name prevents an otherwise same-owner alias outside the verified root from mutating the accepted object. One file attempt takes three evidence samples around two complete same-handle reads and accepts only identical evidence/bytes, with at most two named stable-read attempts. A handle opened before atomic replacement can truthfully yield the complete old object; commit-inventory source-digest equality, the 16,777,216-byte aggregate-generation ceiling, exact role-tagged discovered/transitive source-set equality, and unchanged S2 prevent a hybrid generation. The generic coordinator makes at most two complete S1/source/S2 attempts and delegates only profile/selection TOML inspection through a parser-independent interface. Selected credential JSON and certificate PEM are parsed after S2; unselected credential/certificate sources remain opaque. An actively malicious same-account writer is inside the declared account trust boundary. The account identity used to resolve `~` is the same once-sampled identity used for ownership policy. Linux, macOS, and Windows enforce the exact access-control rules in `ARCHITECTURE.md`. The supported-platform adapter admits only roots whose provider records are unconditionally usable for server authentication without external distrust, purpose, application, policy, or name restrictions; conflicting/unevaluable records fail instead of being reduced to DER. The identity-management client is built only from that immutable verified platform set. An optional safe certificate-authority PEM source extends a distinct immutable author set and can never enter the identity-management connector, verifier, or request path. Both route-specific trust identities enter the revision. Unknown keys are errors.

No public value, diagnostic, cache key, durable operation fact, log, or wire value contains a digest derived from password, private-key, client-secret, assertion, or access-token bytes. The one persisted exception is each exact source digest inside the private owner-only `configuration-snapshot.toml` commit inventory; a profile or credential source can contain a low-entropy secret, so that digest is secret-adjacent, receives the same filesystem protection as the source, and is never rendered, copied into an identity, or persisted outside that manifest. Temporary `SensitiveConfigurationDocument` buffers redact formatting, expose only narrowly named digest/inspection/parse lending operations, and zeroize their Slingshot-owned mutable storage on disposal; `SecretValue` is the long-lived typed wrapper for extracted secrets. Neither wrapper claims control over dependency/operating-system/allocator copies. `AuthenticationPrincipalIdentity` is instead a domain-separated digest of bounded nonsecret principal fields; only that opaque digest, never the raw principal tuple, is carried in target identity, local protocol, logs, or durable partition keys. Unknown JSON key bytes and parser-library source excerpts are also untrusted secret-bearing input. Public diagnostics contain only a manifest source class, stage, structural location, stable code, and occurrence count; they never expose or sort by a source reference, name, or private digest, and their 32-item maximum includes the truncation marker. In-memory token single-flight identity is process-random, scoped to one immutable provider snapshot, and cannot be displayed, serialized, or persisted.

Every author and publisher `base_address` is a normalized allowed origin plus an optional normalized context-path prefix. It has a scheme, host, optional port, and either root or an absolute non-root prefix, but no user information, query, or fragment. Prefixes reject empty/repeated segments, literal or encoded dot segments, encoded slash/backslash, control bytes, and ambiguous trailing-slash forms. Endpoint construction appends encoded endpoint segments to that prefix without URL-join path replacement. Cloud addresses require TLS.

Adobe Experience Manager 6.5 permits a cleartext author on exact loopback hosts `localhost`, `127.0.0.1`, and `[::1]` by default. A non-loopback cleartext author is legal only when the environment sets `allow_insecure_author_transport = true`; that field is legal only for the Adobe Experience Manager 6.5 Basic combination and only when it is needed. Selection carries a stable insecure-transport warning/status for configuration-check and connection consumers. Adobe Experience Manager 6.5 publisher metadata may represent a normalized cleartext address because no API can turn it into a connection. A Basic `user_name` is nonempty, bounded, and colon-free; its canonical bytes are formed without normalization. Outbound identity-management and author clients ignore ambient proxies and connect directly. Operations receive author only; publisher is data, never a dial target.

Snapshot construction produces `AuthenticationPrincipalIdentity`, `AuthorTargetIdentity`, `VerifiedIdentityManagementTrustPolicyIdentity`, `VerifiedAuthorTrustPolicyIdentity`, and `SelectedEnvironmentRevision`. Their exact domain tags, ordered field names, presence bytes, length framing, typed canonical values, SHA-256, and lowercase rendering are closed in `ARCHITECTURE.md`; `AuthorTargetIdentityDigest` is the target hash output itself, never a digest of its rendering. Cloud principal fields are authentication method, `organization_identifier = integration.org`, `technical_account_client_identifier = integration.technicalAccount.clientId`, then `technical_account_identifier = integration.id`; `integration.id` also supplies JSON Web Token `sub` and has no competing integration-identifier type. The revision includes exact normalized nonsecret runtime/profile/source metadata, target identity, transport policy, canonical metascopes, the platform-only identity-management trust identity, and the author platform-plus-selected-additional trust identity, never source-content digests, secrets, or raw principals. Genuine same-principal rotation under a newly committed generation preserves identity/revision; metascope or either route's effective-root drift changes revision; principal drift changes target/revision.

Cloud assertion construction samples UTC once, accepts only whole Unix seconds from zero through 253,402,300,799, and adds the exact 28,800-second lifetime to at most 253,402,329,599 before producing the reachable 12,776-byte compact maximum. The fixed Identity Management Services client emits only `POST`, omits `Expect` and upgrade, bounds the reachable form body at 25,868 bytes, rejects the first informational response head, requires exactly one final head, and accepts a token only after a bounded complete stream proves that neither a `Trailer` declaration nor an empty/nonempty trailer section exists.
