---
id: live-adobe-experience-manager-harness
title: "Live Adobe Experience Manager Harness"
workstream: "0039"
kind: task
depends_on:
  - author-network-chaos
  - credential-exposure-threat-suite
  - protocol-compatibility-snapshots
gated: false
touches:
  - crates/slingshot-command-line/src/live_adobe_experience_manager.rs
  - crates/slingshot-command-line/src/invocation.rs
  - crates/slingshot-command-line/src/application.rs
  - crates/slingshot-command-line/src/lib.rs
  - crates/slingshot-command-line/tests/live_adobe_experience_manager.rs
  - "crates/slingshot-command-line/tests/fixtures/live-adobe-experience-manager/**"
status: done
merged_as: "20299d75693fc2c648a9a4375abdd3fc887bd7b1"
---
# Live Adobe Experience Manager Harness

Hermetic fake-author proof remains the release gate, while an operator also needs a safe way to verify the same read path against a selected real author. This task provides an explicitly enabled harness whose command selection is read-only by construction.

**Steps:**

1. Author fake-author fixtures and command-admission tests first for absent enablement, invalid target, all twelve exact access/destructive/idempotency registry rows, exact five-field selected-command identity plus separate canonical-contract artifact/digest/dual-annotation capability acceptance and independent drift of each role, raw-canonical-before-Draft-2020-12-before-typed validation, canonical phrase/asset/token input including exact `AssetByteLength` boundaries, every revised closed failure, inline/externalized result, heartbeat, reconnect, snapshot recovery, result, artifact, non-author dial attempt, and redacted failure.
2. Adopt the dependency-ordered command-line crate, invocation, and exhaustive application roots; declare the live Adobe Experience Manager module exactly once and compose `slingshot verify live-author` through an explicit application branch requiring an enable flag, profile, environment, and bounded absolute Adobe Experience Manager repository-content root represented by the production repository-path type. It never accepts a local filesystem or source-checkout path, cannot fall through to an ordinary operation branch, and normal `cargo test` never infers or enables live access.
3. Resolve configuration and credentials through production providers and connect only to the selected author.
4. Byte-match the complete Plan 0003 twelve-row authority, select exactly the nine `Read`/`NonDestructive` commands, reject all three `Write` commands before daemon dispatch, and never use intrinsic idempotency as an access decision. Require the selected agent capability to match all five identity fields plus separately authenticated canonical-contract artifact/digest/dual annotations; reject any drift with the command name recorded.
5. Exercise capabilities, content load as JavaScript Object Notation, path query, one supported page or asset query with exact bounded `AssetByteLength`, progress, terminal result, heartbeat/reconnect, snapshot recovery, and optional verified artifact. When exact configuration inspection is supported and explicitly selected, submit an adversarial persistent identifier and require the agent's conformance report to attest escaped `listConfigurations`-only lookup, exact persistent-identifier postcheck, exactly one `getProperties()` acquisition and one complete keys-only enumeration, bounded no-partial handling, hostile carriers, metatype/redaction-before-value classification, zero reads for rejected/redacted values, and exactly one read for each visible value; never infer those internal calls from a successful value alone.
6. Emit a structured report containing deployment class, nonsecret target identity, command, duration category, operation identifier, result classification, agent stream generation, and connector-audited author-only assertion. The report distinguishes hermetic conformance from optional live evidence and never generalizes one live run to an untested patch level.

**Tests:**

- Without explicit enablement the harness performs zero configuration, credential, daemon, and network access.
- The invocation and application inventories contain exactly one `verify live-author` branch, and parser-to-dispatch call counts prove it reaches only the live harness while every prior exhaustive branch remains reachable and unchanged.
- All twelve registry rows match exactly; all three `Write` entries are rejected before dispatch and exactly nine `Read`/`NonDestructive` entries are admissible, independent of their idempotency annotation.
- Any identity, canonical-contract, or classification drift rejects before dispatch, while accepted SearchPhrase, ascending UTF-8 asset sets, zero/maximum `AssetByteLength`, and opaque continuation bytes retain their canonical spelling. Optional configuration evidence is reported unsupported unless the selected bundle supplies the exact one-acquisition/one-enumeration conformance trace; no live claim extrapolates beyond it.
- Basic and Cloud fake-author runs use production authentication and contain no sentinel output.
- Reconnect and artifact cases converge and verify exactly as the hermetic conformance suite.
- The production connector audit records only the selected author origin; a configured publisher and every other destination record zero dial attempts.

- **Done when:** `cargo test -p slingshot-command-line --test live_adobe_experience_manager` proves explicit enablement, registry-enforced read-only admission, both authentication modes, reconnect/artifact behavior, redaction, and author-only traffic, and `scripts/quality` succeeds; an operator can then invoke `slingshot verify live-author` explicitly against a configured author.
