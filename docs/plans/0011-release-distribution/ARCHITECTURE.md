# Plan 0011 — Release Distribution

## Architectural boundary

Nothing this plan adds runs inside the product. The distribution path is scripts the operator and the provider run, and `slingshot-development` is where their contracts are asserted, which is where every other repository-policy contract is already asserted. No product crate gains a dependency, and the dependency diagram does not change.

## What proves what

A script is proved the way the workflow policy is proved: by driving the real executable and requiring each refusal for its own reason. A refusal is observed as a nonzero status with nothing written, never as the text of a message, because a message is prose and prose drifts. The publisher is driven against a recording stand-in for the provider command so that "nothing was uploaded" is a fact the suite can read rather than an outcome nobody looked for.

Two values in this path are declared once and read everywhere else. The pinned tool versions live in the tool manifest, and the installer reads them; the repository identity lives in the automation authority, and the publisher reads the repository from the workspace manifest. A fixture that needs one of them expands it from the document rather than writing it down again, so no assertion is pinned to a value that is free to change.

## What stays outside

The provider's own behaviour is not simulated. Whether an attestation authenticates is decided by the pinned provider client against the committed trust root, and this plan asserts only that the publisher refuses when that decision is negative. The gate's own checks are unchanged: this plan gives the gate its tools and asserts that it was given them, and changes nothing the gate concludes.
