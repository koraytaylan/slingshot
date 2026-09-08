# Releases

The release workflow stops at evidence. A release is publishable only after an
operator has downloaded and checked that evidence; the workflow does not create
a GitHub release or push anything.

## What a release carries

There is one archive for every row in
[`support/platforms.toml`](../support/platforms.toml):

| row | archive | beside the archive |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | the native `slingshot-*.tar.gz` archive | `attestation.jsonl` and `evidence.toml` |
| `aarch64-apple-darwin` | the native `slingshot-*.tar.gz` archive | `attestation.jsonl` and `evidence.toml` |
| `x86_64-pc-windows-msvc` | the native `slingshot-*.zip` archive | `attestation.jsonl` and `evidence.toml` |

The archive contains the executable, `LICENSE`, and `SHA256SUMS`. The
`attestation.jsonl` bundle is the provider's signed statement about the exact
archive bytes; it is kept beside those bytes so it can be checked without
asking the provider for anything later. The row's `evidence.toml` records the
cache digest and other inputs the verifier binds to the archive. The downloaded
evidence also includes the acceptance decision and release notes.

## Verify one archive offline

Run this from a clean checkout of the same source commit named by the release
evidence. Substitute the paths and values from the downloaded row; the cache
digest is the `cache-sha256` value in that row's `evidence.toml`.

```sh
scripts/verify_release_artifacts \
  --archive path/to/slingshot-<version>-<triple>.tar.gz \
  --attestation-bundle path/to/attestation.jsonl \
  --evidence path/to/evidence.toml \
  --source-commit <40-character-source-commit> \
  --cache-sha256 <64-character-cache-sha256>
```

This command reaches nothing: it uses the reviewed trust root committed at
`support/release-attestation-policy.toml`, authenticates the attestation
bundle first, and then checks the archive and evidence. It does not contact a
Git remote, the provider, an operating-system trust store, or an ambient
cache. If any required file or value is absent, or any check refuses, the
archive is not verified.
