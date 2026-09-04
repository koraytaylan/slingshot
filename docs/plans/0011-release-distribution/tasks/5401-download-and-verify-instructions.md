---
id: download-and-verify-instructions
title: "Download And Verify Instructions"
workstream: "0054"
kind: task
depends_on: ["an-attestation-travels-with-its-archive"]
gated: false
touches:
  - README.md
  - crates/slingshot-development/tests/download_instructions.rs
status: planned
merged_as: ""
---
# Download And Verify Instructions

An attestation nobody is told how to check is a claim rather than evidence. A reader who has downloaded an archive should be able to establish what it is without trusting the page that offered it.

**Steps:**

1. Publish, in product documentation, what a release carries: the archive for each supported row, the attestation bundle beside it, and what each is for.
2. Publish the exact command that authenticates one archive offline against the committed trust root, and say plainly that it reaches nothing.
3. Hold the documented command to the verifier's real interface: every option it passes is one the verifier accepts, and every option the verifier requires appears in it.

- **Done when:** the documented command's options and the verifier's accepted options are the same set, so the instructions cannot describe an interface that does not exist.
