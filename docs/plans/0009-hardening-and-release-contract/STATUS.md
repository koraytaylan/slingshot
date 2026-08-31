# Plan 0009 — Hardening and Release Contract — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.

- **Goal:** establish a reproducible release contract whose parsers, exact command provenance/canonical forms, retained control plane, durable recovery state, credentials, compatibility surfaces, supported-platform adapters, documentation, and packaged binaries withstand the named generative, failure, and threat suites.
- **Root cause:** focused feature tests do not by themselves prove daemon and agent restart recovery, safe cursor reset, hostile-input bounds, absence of credential or configuration-secret exposure, minimum-compiler and platform compatibility, retained data/control compatibility, or agreement between product documentation and release artifacts.
- **Approach:** add pinned fuzzing, chaos/threat/compatibility gates that authenticate Plan 0002's exact two-per-file/two-whole-generation attempts, source/aggregate bounds, parser-independent role inventory and S2 order, source-reference-free inclusive diagnostics, temporary-sensitive versus long-lived-secret lifecycles, exact HTTP/1.1-or-HTTP/2-only assertion/exchange head/compression/body/token/framing refusal with no migration/`Alt-Svc`/automatic decompression, route-typed immutable `VerifiedIdentityManagementTrustPolicyIdentity` platform roots and `VerifiedAuthorTrustPolicyIdentity` platform-plus-selected-additional roots from one committed snapshot, and hostile additional/private author-CA interception refusal alongside both canonical daemon-runtime and author-agent-transport manifests/digests; independently prove Plan 0005's all-route HTTP/1.1-or-HTTP/2-only, raw-and-encoded-head/informational/final/trailer/framing/exact-end/post-byte-certainty policy; reject drift in the five-field command identity and separate canonical-contract/role-schema provenance; and exhaust universal 431-byte continuation-authority readiness, request-start retention, distinct DNS/TCP and TLS phases, logical-outbox/fenced-effect, exact SQLite no-spill/closed-SQL/restrictive-VFS arithmetic and checkpoint/backpressure, same-handle operation-artifact resume, operation-free `maintenance_result_access` identifier/association/metadata/read/retention lifecycle with authenticated expected-digest lookup and no operation/workflow identity, canonical request, exact SemVer/OSGi, closed failure, and CLI/Model Context Protocol/fake-agent cases. Then use owner automation to authenticate every mapped native row's real safe provider API and actual enumerated effective fields separately from the same compiled adapter's deterministic permit/distrust/purpose/external-constraint/unevaluable/equal-duplicate/conflicting-same-DER matrix, reject certificate-only reduction or trust-store mutation, prove Windows remote-pipe rejection, require a protected same-release RustSec owner-review record, prepare exact offline row/coordinator caches plus the bounded Plan 0008 seed, authenticate native archives/security and the single FSM row, and aggregate them in network-none acceptance.
- **Progress:** 11/24 tasks done; 0 blocked; 0 dropped.
- **Integration:** `in progress`; run `develop`; base `main` @ `8d286e88c06f91a1513834a4839ae36582212242`; validation base `9008acc24db81466e88145367a6f3cbcd03c4faa`; mode `sequential`; final integration —.
- **Exceptions:**
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
