# Plan 0012 — Cryptographic Backend Consolidation

> Compile one cryptographic implementation into this product instead of two, and make the number of them a thing an assertion decides rather than a thing a dependency's default chooses.

## Why this plan

This workspace selected `ring` for every transport it opens. It also compiles a second, unrelated cryptographic implementation, and nobody selected it: the assertion library's default backend named one, and the graph resolved it. Two implementations of the same arithmetic are now linked into every binary this project ships.

The cost is not theoretical. It is two bodies of C and assembly rather than one, two sets of advisories to track against one product, two build toolchains a release has to reproduce on three platforms, and one of them arrived without a decision anybody recorded. It has already had a practical consequence: the second implementation assembles its own primitives with a tool no runner image is required to carry, and the Windows row could not build until the build was pointed at assembly the crate ships prebuilt.

Consolidating is not a matter of changing a feature. The assertion library offers exactly two backends, and the other one depends on a pure-Rust modular-exponentiation implementation carrying an unfixed key-recovery timing sidechannel - an advisory this repository's own gate refuses, correctly, because the key it would leak is the one that signs every credential assertion this product makes. So the library goes, and the one signature it produced is produced directly against the transport's own backend.

The narrowness is what makes this safe. One algorithm, one key format, one assertion shape, and a suite that already pins the exact bytes of the assertions this product signs. If the signature is what it was, nothing above it can tell the difference; if it is not, the fixtures say so before anything reaches a network.

## In scope

- **0055 — One backend, and an assertion that says so.** Produce the credential assertion's signature against the backend the transports already use, over the same key material, and delete the assertion library and the implementation it brought with it. The existing assertion fixtures decide whether the bytes are unchanged; the exchange fixtures decide whether what is signed is still accepted. Then close the question structurally: the resolved graph carries exactly one cryptographic implementation, and a second one entering by any path - a default feature, a new dependency, a backend swap - is refused with the name of what pulled it.

## Out of scope

Which backend is chosen is not reopened: the transports already chose, this plan follows them, and a change to that choice is a different plan. Key custody, key rotation, the assertion's shape, the exchange it is presented to, and the algorithms named in the command contract are unchanged - this plan changes what performs an operation, never what the operation is. Removing the C and assembly that the pinned embedded database compiles is outside this plan; that dependency is deliberate, pinned to its own source, and reproduces offline today.
