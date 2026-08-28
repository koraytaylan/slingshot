---
id: build-signed-token-assertions
title: "Build Signed Token Assertions"
workstream: "0007"
kind: task
depends_on:
  - parse-cloud-service-credentials
gated: false
touches:
  - crates/slingshot-agent-connection/src/authentication/token_assertion.rs
  - crates/slingshot-agent-connection/tests/token_assertions.rs
  - crates/slingshot-test-support/fixtures/token-assertions/**
status: planned
merged_as: ""
---
# Build Signed Token Assertions

Construct byte-pinned Adobe exchange JSON Web Token assertions from validated service credentials, one injected UTC wall-clock observation, the named assertion lifetime, and `RS256`.

**Steps:**

1. Commit independent language-neutral vectors for exact protected-header/payload UTF-8, claim ordering, literal Unicode, quote/reverse-solidus escaping, forbidden alternate `\u`/solidus/surrogate spellings, minimal integers, unpadded base64url segments, period framing, complete signing input, modulus-width signature, and compact assertion; cover exact `iss`, `integration.id`-backed `sub`, `exp`, `aud`, metascope claims, lifetime, 12,776-byte reachable maximum, sampled UTC below/equal/above zero and 253,402,300,799, maximum `exp = 253,402,329,599`, certificate clock boundaries, and malformed/mismatched keys.
2. Validate a bounded comma-separated unique metascope collection of lowercase ASCII letters, digits, and underscores plus the separate bounded client-identifier path segment. Build `aud` as `https://{validated identity-management authority}/c/{client identifier}` and one true claim per metascope at `https://{validated identity-management authority}/s/{metascope}`; no input can add a path, query, header field, or arbitrary claim.
3. Sample the injected UTC clock once; fail in exact order when unavailable, outside nonnegative whole Unix seconds through `MAXIMUM_SERVICE_CREDENTIAL_UTC_UNIX_SECONDS`, before certificate not-before, or after certificate not-after. Accept equality at either certificate boundary, add the named lifetime with checked arithmetic to at most 253,402,329,599, emit no other claim, encode strings/objects exactly as architecture specifies, base64url without padding, sign the exact two-segment ASCII input through RSASSA-PKCS1-v1_5 SHA-256, and emit the exact three-segment compact form. A signing failure follows all clock/certificate checks.
4. Verify every fixture signature independently with the credential public certificate and scan all failure paths for private-key, assertion, identifier, certificate-subject, or unknown-field disclosure.

**Tests:**

- `token_assertions` compares header, payload, encoded segments, signing input, signature octets, and compact bytes exactly; rejects alternate equivalent-looking encodings/surplus data; and verifies every signature independently.
- Unavailable/out-of-range whole-second, checked-lifetime, certificate-validity equality, signing, and every precedence-tie case is pinned without reading a system or monotonic cache clock.

- **Done when:** `cargo test -p slingshot-agent-connection --test token_assertions` independently verifies one exact Adobe compact `RS256` byte contract from canonical JSON through signing input/signature, path-safe audience/metascopes, `integration.id` technical-account subject, checked expiry/certificate boundaries, and every alternate/error case.
