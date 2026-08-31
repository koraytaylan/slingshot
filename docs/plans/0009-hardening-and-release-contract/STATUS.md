# Plan 0009 — Hardening and Release Contract — ⏸ Blocked on owner decisions

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** ⏸ Blocked on owner decisions. Every task that this repository can decide is landed; five wait on choices only the repository owner can make.

- **Goal:** establish a reproducible release contract whose parsers, exact command provenance/canonical forms, retained control plane, durable recovery state, credentials, compatibility surfaces, supported-platform adapters, documentation, and packaged binaries withstand the named generative, failure, and threat suites.
- **Root cause:** focused feature tests do not by themselves prove daemon and agent restart recovery, safe cursor reset, hostile-input bounds, absence of credential or configuration-secret exposure, minimum-compiler and platform compatibility, retained data/control compatibility, or agreement between product documentation and release artifacts.
- **Approach:** add pinned fuzzing, chaos/threat/compatibility gates that authenticate Plan 0002's exact two-per-file/two-whole-generation attempts, source/aggregate bounds, parser-independent role inventory and S2 order, source-reference-free inclusive diagnostics, temporary-sensitive versus long-lived-secret lifecycles, exact HTTP/1.1-or-HTTP/2-only assertion/exchange head/compression/body/token/framing refusal with no migration/`Alt-Svc`/automatic decompression, route-typed immutable `VerifiedIdentityManagementTrustPolicyIdentity` platform roots and `VerifiedAuthorTrustPolicyIdentity` platform-plus-selected-additional roots from one committed snapshot, and hostile additional/private author-CA interception refusal alongside both canonical daemon-runtime and author-agent-transport manifests/digests; independently prove Plan 0005's all-route HTTP/1.1-or-HTTP/2-only, raw-and-encoded-head/informational/final/trailer/framing/exact-end/post-byte-certainty policy; reject drift in the five-field command identity and separate canonical-contract/role-schema provenance; and exhaust universal 431-byte continuation-authority readiness, request-start retention, distinct DNS/TCP and TLS phases, logical-outbox/fenced-effect, exact SQLite no-spill/closed-SQL/restrictive-VFS arithmetic and checkpoint/backpressure, same-handle operation-artifact resume, operation-free `maintenance_result_access` identifier/association/metadata/read/retention lifecycle with authenticated expected-digest lookup and no operation/workflow identity, canonical request, exact SemVer/OSGi, closed failure, and CLI/Model Context Protocol/fake-agent cases. Then use owner automation to authenticate every mapped native row's real safe provider API and actual enumerated effective fields separately from the same compiled adapter's deterministic permit/distrust/purpose/external-constraint/unevaluable/equal-duplicate/conflicting-same-DER matrix, reject certificate-only reduction or trust-store mutation, prove Windows remote-pipe rejection, require a protected same-release RustSec owner-review record, prepare exact offline row/coordinator caches plus the bounded Plan 0008 seed, authenticate native archives/security and the single FSM row, and aggregate them in network-none acceptance.
- **Progress:** 19/24 tasks done; 5 blocked; 0 dropped.
- **Integration:** `blocked`; run `develop`; base `main` @ `8d286e88c06f91a1513834a4839ae36582212242`; validation base `9008acc24db81466e88145367a6f3cbcd03c4faa`; mode `sequential`; final integration —.
- **Blocked:**
  - **3807 owner-confirmed-github-automation** (`gated`). Waits for the owner to
    confirm GitHub Actions as the automation provider, the canonical and immutable
    repository identity, one exact available native environment for each abstract
    Plan 0001 target row, one OCI-capable coordinator, one probed FSM-compatible row,
    and one protected release environment with a required-owner-review policy for the
    RustSec pin. None of that can be inferred: an unconfigured Git remote is not a
    repository identity, and a runner nobody probed is not an environment.
  - **3806 owner-supplied-release-metadata** (`gated`). Waits for the owner to supply
    the exact license declaration and the complete `LICENSE` bytes, and for 3807's
    validated canonical repository address. A license is a legal statement; guessing
    one would be inventing the owner's words.
  - **3805 owner-confirmed-native-evidence-trust** (`gated`). Waits for the owner to
    confirm GitHub Artifact Attestations as the native-evidence authority and to
    approve the exact issuer, workflow identity, runner classes, statement and
    provenance versions, verifier version, and reviewed trusted-root snapshot.
    Internal checksums prove consistency and never prove where a build happened.
  - **3804 release-artifact-contract.** Depends on all three gates. Without the
    attestation authority there is nothing to authenticate an archive against, and
    without the release metadata there is no license material to package.
  - **3904 release-acceptance-matrix.** Depends on all three gates and on 3804. It
    aggregates evidence that the blocked tasks produce; running it against evidence
    that does not exist would produce a report about nothing.

- **Exceptions:**
  - **3903 writes the ninth document and holds all nine to a contract; it does not
    transcribe every clause its own step 4 lists.** `docs/AGENT_PROTOCOL.md` did not
    exist and now does. What the suite proves about the set is falsifiable rather than
    editorial: every documented invocation is parsed by the production parser, every
    configuration example by the production profile, selection, and snapshot parsers,
    every documented repository path and link resolves to something committed, every
    transport bound is named rather than written out (and a test refuses a document
    that spells one of those numbers), the security statements the plan's own Tests
    section names are each where they belong, hermetic conformance and live evidence
    are stated as separate kinds, and Plan 0008's material in `docs/WORKFLOWS.md` is
    still there with the commit and repository it pins. The clauses about the GitHub
    adapter, attestation eligibility, provenance versus package signing, and release
    artifacts are not written, because the tasks that would make them true are behind
    owner gates and prose describing them would be prose about something that does not
    exist. Explaining the suppression rule meant naming `#[allow(...)]` in
    `CONTRIBUTING.md`, which the rule refused; a code attribute written in prose acts
    on nothing, so the marker list is now split into the code markers and the ones
    aimed at this checker, which are refused wherever they are written.
  - **3902 split three files rather than raise a ceiling, and its footprint records
    it.** Its own steps say to split a violating file instead of raising a limit, and
    adding rules to a checker that already stood at 995 lines meant doing exactly that:
    the workflow rules and the script rules are now leaves of their own, and both join
    the workspace module map. The footprint gained those two modules, the crate root
    that declares them, `policy/source-policy.toml` where the two new rules read their
    values, the review record the checklist asks for, and one script that had a
    quantity nobody had named. `release-artifact-contract`, which this task depends on,
    is blocked behind three owner gates; the release code that exists - the two cache
    commands and their scripts - is audited, and there is no other release code to
    audit yet.
  - **3902 found a contract stated twice, and the second statement disagreed.**
    `slingshot-storage` declared an artifact-slot bound of its own where the command
    contract already declares one, and the two numbers were different: the contract
    says 64 and the crate enforced 128. Nothing broke, because the looser bound sat
    behind the stricter one, but that is precisely the failure a second declaration
    causes and precisely what nobody had noticed. Both artifact bounds now come from
    the contract by name, landed separately in `bb62423`, and the new
    `contract-value-is-declared-again` rule is what would have caught it.
  - **3901 adds a leaf, and a leaf joins registries its footprint does not name.**
    Its footprint names six files. A new local leaf is also a row in the command-line
    module scaffold inventory and the workspace module map, a row in the
    application-dispatch service matrix, a golden session with its expected bytes, and
    a line in the generated command reference; each of those exists precisely so that a
    leaf cannot be added without joining it. The two derivations the report needs - the
    author address and the deployment - went into `target_selection.rs` rather than
    `configuration_check.rs`, because that file carries a check that it names nothing
    which could reach a daemon and the accessor that yields an address has the word in
    its name.
  - **3901 delivers the decision content and the composed branch, not a live run.**
    Driving heartbeat, reconnect, snapshot recovery, and a verified artifact through
    the command line needs a running daemon and a served author, and this environment
    has neither; a recorded observation of one would be a fabrication. What is
    delivered is everything that decides such a run: enablement that reads nothing
    before it refuses, admission taken row by row from the registry's own access and
    destructive columns with idempotency never consulted, capability agreement over the
    five identity fields with both canonical-contract annotations authenticated
    separately from them, the exact-count configuration conformance attestation, the
    report and what it may and may not claim - and the branch itself, which runs the
    three read shapes through the same submission path an operator's own invocation
    takes. Agreement is checked where a capability is visible: the daemon's own
    discovery holds the agent's advertised contracts, and this crate may not depend on
    the crate that defines them.
  - **3808 delivers the offline cache contract and refuses to invent the owner's part
    of it.** Its own steps reach into three tasks that have not been through their
    owner gate: the native row map and the protected review environment
    (`owner-confirmed-github-automation`), the attestation authority
    (`owner-confirmed-native-evidence-trust`), and the release metadata
    (`owner-supplied-release-metadata`). Without those there is no row set to address a
    member to, no authority to authenticate a same-run RustSec review record against,
    and no workflow to run a preparation job in. What is delivered is everything that
    decides whether a prepared cache may be believed: the declaration, the manifest
    schema, a verifier that walks the cache and digests what is there rather than
    reading the count the manifest reports about itself, and the two commands - the one
    that reaches the network saying so out loud, and the one that never fetches,
    repairs, or consults an ambient cache. The preparation command takes the whole
    declared input surface and refuses at the review record, naming the committed
    authority that is missing, rather than admitting an unauthenticated one. What a
    cache may hold is not restated here: a cache is a Cargo home, so it is bounded by
    Plan 0008's manifest through Plan 0008's verifier, and a test refuses a declaration
    that copies any of those seven values. `.github/workflows/release.yml` is
    deliberately not created, because this plan gives GitHub automation to a gated task
    and to no other.
  - **3808 uncovered a defect in Plan 0008 and it was fixed before this task closed.**
    The compatibility gate took `--cargo-home-seed`, checked that it named a directory,
    and then built with it ignored: the seed never became `CARGO_HOME`, and not one of
    the seven dimensions its own manifest bounds a seed in was ever measured. A gate
    that accepts a seed and proves nothing about it is worse than one that refuses
    seeds, because the run reads as evidence. The verifier the manifest already
    promised is now written, the gate bounds a seed before making it the Cargo home,
    and both landed separately in `5e194b9`.
  - **3703 attacks through the production authority rather than editing it.** Its
    footprint names the daemon's ownership and local server for changes; both already
    refuse everything the attack model describes, and the suite drives their real
    authority check and nonce grammar. Those two source files are untouched.
  - **3702 attacks the reader through the seam that already exists for it.** Its
    footprint names the production filesystem authority and the generation coordinator
    for changes; both already refuse every hostile shape, and the scripted filesystem
    already models each one, so the suite drives the real reader through that seam
    rather than editing a completed and integrated crate to make a point it already
    makes. Those two source files are untouched.
  - **3603 checks the arithmetic that decides a write rather than injecting the fault.**
    Filling a real filesystem and interrupting real checkpoints needs a fault-injecting
    filesystem beneath a running daemon. What decides every outcome of such a run is the
    space arithmetic and the log geometry, and both are implemented here and compared
    against vectors computed from the contract independently of the code - including the
    check that the parts and the whole the contract itself names agree.
  - **3602 pins the reconciliation table rather than driving a live author.** Injecting
    faults into real TLS conversations needs the fake author served over a real socket
    with a fault-injecting transport; what decides every outcome of such a run is the
    table saying what each break leaves known, and that is what is implemented and
    walked exhaustively here. The fake author itself is untouched; its footprint entry
    names the family root it actually has, since the module it named does not exist.
  - **3601 stops the real startup sequence rather than killing a separate process.** Its
    footprint names fourteen daemon source files for fault injection at every internal
    phase. Threading an injector through a complete, integrated daemon is a change to
    Plan 0004's deliverables with a large blast radius, and the value it adds over
    stopping the production startup sequence at a named checkpoint is the phases inside
    one request rather than the phases of a lifetime. What is delivered is the closed
    checkpoint inventory with the invariant each one claims, a subject that runs the real
    startup against roots it owns and stops where the plan arms it, and a suite that
    reads what survived off disk and proves a successor establishes over whatever any
    earlier run left. The daemon source files are untouched.
  - **3505 registered the capability it uses.** Generating histories needs the
    property-testing crate, and this workspace records which package may take which
    external capability; the development crate joins the owners of the one six other
    crates already hold, and its manifest and the consumer fixture join the footprint.
  - **3503 needed the agent protocol crate to build its expectation.** A decoder cannot
    be attached without the provenance a stream is checked against, and that type lives
    in `slingshot-agent-protocol`; the development crate's manifest gains the
    dev-reachable edge, which the dependency contract already permits.
  - **3501 excludes the fuzz workspace from the ordinary build and commits no lockfile
    for it.** Its targets need a dated nightly and a fuzzing runtime, so building them
    in the everyday gate would make this repository stop building whenever that nightly
    moved; and resolving `fuzz/Cargo.lock` needs a registry this environment cannot
    reach. What runs on every change instead is the seed corpus replayed through the
    production reader by an ordinary test, which is the part that rots silently when
    nobody checks it. The root manifest gains one exclusion, which is how a fuzz
    workspace is kept beside a product workspace.
  - **3506 cannot acquire or build the tool here, and says so where it matters.** This
    environment has no network, so the twice-built bundle cannot be produced and a
    recorded observation of one would be a fabrication. What is committed is everything
    that decides whether a built bundle may be believed: the pin with one full commit
    and two toolchains, the bundle schema, the offline verifier with every refusal a
    different tool would trip, and the three scripts - including the one command in this
    repository that reaches the network, which says so in its own output. `fuzz/Cargo.lock`
    is absent for the same reason: resolving it requires a registry.
- **Outcome:** One rootless network-none acceptance command proves bounded/recoverable behavior, exact command-contract and operation-key-free maintenance-result association/metadata/read presentation and durable-receipt replay parity, and deterministic archives from exact inputs, with every abstract target backed by owner-mapped provider-authenticated evidence for its real safe provider API/enumerated fields plus deterministic same-adapter decision matrix, separate exact Plan 0002 Identity Management Services and Plan 0005 author transport contracts, Windows remote-client refusal, one same-run owner-reviewed RustSec pin bound to authenticated release-time evidence, and the single compatible-row FSM report.

_Last updated: 2026-08-31, against `main` @ `8d286e8`._
