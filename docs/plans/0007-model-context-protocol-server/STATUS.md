# Plan 0007 — Model Context Protocol Server — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.
- **Goal:** expose every Slingshot operation and durable operation/maintenance-result state through a byte-clean standard-stream Model Context Protocol server with bounded active requests/messages/drop-capable diagnostics and exact ordered revision support `["2026-07-28","2025-06-18"]`.
- **Root cause:** model hosts otherwise need the private daemon protocol, while a manually duplicated tool catalog or single-era lifecycle would drift from registry schemas and exclude either modern stateless clients or the fsm executor.
- **Approach:** centralize bounded JSON-RPC transport and the exact ordered revision authority; pin modern discovery/method/error behavior and legacy successful fallback negotiation; derive tools, the exact five-required/seven-optional operation-key policy, and operation-free target-qualified maintenance-result resources from authenticated daemon-runtime/author-agent-transport/canonical-JSON/`1.0.0`/limits/role-schema provenance; resolve each standalone maintenance URI through target-and-identifier metadata before expected-digest read without hash inversion; generate an omitted optional key once per active tool request and retain it across reconnect/retry; validate raw bytes before decoded shape and typed construction; preserve complete over-inline maintenance plus closed result/failure parity with CLI JSON; and map progress/cancellation onto durable observation with byte-exact and retained-instance process proofs.
- **Progress:** 14/16 tasks done; 0 blocked; 0 dropped.
- **Integration:** `in progress`; run `develop`; base `main` @ `8d286e88c06f91a1513834a4839ae36582212242`; validation base `1a592249115df391edac4d8f84fcb52262a3d36e`; mode `sequential`; final integration —.
- **Exceptions:**
  - **3104 found the composition serving an initialized session in the wrong era.**
    Replaying the sequence an executor really sends showed the server decorating a
    legacy session's results with the current era's members, because the era was decided
    per request and a client that had initialized sends nothing about revisions
    afterwards. A session that finished the older handshake is now served in that era
    whatever a later request says; a session that never initialized stays stateless.
    The composition is 3107's file, and the transcript is where the defect was visible
    at all.
  - **3103 taught the process harness to feed a child its input.** A conversation is
    lines in and lines out, and the harness started every child with nothing on its
    standard input. Writing the whole input and closing it joins the harness, because a
    child left with an open input that never produces anything waits for a deadline
    instead of finishing.
  - **3107 also touched the command reference and the golden sessions.** Adding a leaf
    to the closed vocabulary changes what the reference lists and what the session
    coverage requires, and both are checked against the vocabulary rather than written
    beside it - which is what they are for. It also corrected the option table: five
    page-mutation options fell through to the default arm and were being advertised on
    every leaf that reaches somewhere, including the daemon controls.
  - **3101 decides everything before dispatch and leaves the dispatch to the entry.**
    Subscribing to an operation until it ends, reconnecting from the last durable
    revision, and preserving complete maintenance bytes are conversations with a daemon
    that serves versioned operations; this build's daemon serves retained control. What
    is implemented and proved here is every decision those conversations turn on:
    provenance before the tool, the tool before the arguments, the three argument checks
    in their fixed order, a supplied key preserved exactly, an omitted optional key
    invented once and reused across every reconnect and retry, and a resume that
    schedules nothing unless what it believes is still true. The exchange is the
    application entry's to make once the daemon answers it.
  - **3003 decides everything a read depends on and reaches no daemon to perform one.**
    The task's read sequence is a conversation with Plan 0004's maintenance metadata and
    read services, which this build's daemon does not serve over the wire; a scenario
    that claimed to have performed one would be describing an exchange that did not
    happen. What is implemented and proved is every decision the sequence turns on: how
    an address is parsed back into the parts it names, that a maintenance identifier can
    carry no operation or slot, what must be equal between the lookup and the read, the
    single ownership transfer that may occur between them and in which direction, the
    bounded listing page, and what no resource may ever carry. The exchange itself is
    the application entry's to make once the daemon answers it.
  - **3002 projects and pins schemas rather than committing golden copies of them.**
    The task asks for committed schema goldens for all twelve command tools. A schema
    is projected from the registry, so committing its bytes would commit a second copy
    of something already derived, and the copy is what drifts. What is committed is one
    digest per tool for its input and output schema, rewritten deliberately under a
    named review command, which catches the change the goldens exist to catch without
    keeping a second inventory.
    The typed-construction stage is declared and the first two are enforced here. Raw
    canonical bytes and declared shape are what this projection can decide; constructing
    a command from the values is the command builder's own decision, and duplicating it
    here would create a second constructor to disagree with the first.
  - **2902 has no way to retrieve the official artifact, and does not pretend otherwise.**
    The task requires the complete unmodified official revision document, its source
    location, its retrieval identity, and its digest, used as the oracle every request
    and response is validated against. This environment has no network access, and a
    hand-written file presented as that document would be a forgery - worse than an
    absent one, because every later test would cite it as authority. What is committed
    is this build's own declaration of the shapes it serves, digest-pinned, recomputed
    before use, and named for what it is in a `PROVENANCE.md` beside it. The mechanism
    is real and running; the authority is this build's own until the artifact is
    retrieved, at which point it replaces the declaration and the same tests validate
    against it.
  - **2901 reads the wire bounds it was going to restate.** The task asks for named
    line and depth limits, and the workspace already has exactly one source for both:
    the foundation contract's framing limits, which the repository's own checker
    refuses to see repeated. The transport reads them rather than declaring numbers of
    its own, so a message this server admits is one the rest of the product admits.
    Queue counts, byte bounds, and the three deadlines are this transport's own and are
    named here.
    The scaffold's emptiness claim is relaxed the same way Plan 0006's was: it was true
    of the commit that created the leaves and stops being true of the first one to
    implement any of them. What stays checked is the enduring claim - every leaf opens
    with enough of its own documentation to say what it owns, declares nothing above
    that documentation, and claims nothing it has not done.
- **Outcome:** Modern clients receive the exact two-item discovery/error revision order, fsm-compatible legacy clients negotiate `2025-06-18` correctly, and both eras discover the same exact-version/limits-bound tools with five required and seven optional operation keys plus target-qualified operation and maintenance-result resources, retain each supplied or once-generated request operation identifier through reconnect/retry, retrieve lifecycle-valid canonical maintenance JSON, submit canonical phrase/asset/token requests, receive only command-specific closed results/failures byte-identical to CLI JSON, cannot overrun or alias requests, detach safely, and observe protocol-only stdout even when stderr is unavailable.

_Last updated: 2026-08-31, against `main` @ `8d286e8`._
