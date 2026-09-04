---
id: secret-block-redaction-spans-lines
title: "Secret Block Redaction Spans Lines"
workstream: "0066"
kind: task
depends_on: []
gated: false
touches:
  - crates/slingshot-daemon/src/diagnostics.rs
  - crates/slingshot-daemon/tests/diagnostics.rs
  - crates/slingshot-daemon/tests/fixtures/diagnostics/redaction.jsonl
status: completed
merged_as: ""
---
# Secret Block Redaction Spans Lines

Private keys are multiline blocks. The redactor stops at the whitespace immediately after the opening marker, so its normal input shape leaves the encoded key body and closing marker in the diagnostic record.

**Steps:**

1. Recognize each supported PEM opening through its matching closing marker across line endings and replace the complete bounded block before field truncation or persistence.
2. Treat an unterminated opening conservatively through the end of the record, and keep bearer-token redaction bounded to the credential rather than unrelated following text.
3. Replace the one-line synthetic key fixture with real multiline PKCS#8 and RSA-shaped records, plus CRLF, unterminated, adjacent, prefixed/suffixed, and multiple-block cases carrying unique sentinels.
4. Prove no sentinel, encoded body fragment, opening, or closing marker reaches active or rotated diagnostic files, while nonsecret text on either side remains.

- **Done when:** complete multiline key material and bearer credentials are absent from all rendered and persisted diagnostics for every terminated and unterminated fixture, without erasing unrelated surrounding context.
