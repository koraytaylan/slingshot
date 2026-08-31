# Plan 0006 — Command-Line Experience — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.
- **Goal:** expose the complete Slingshot operation registry through a deterministic command surface with namespaced daemon startup, durable detachment, stable output, and process-level proof.
- **Root cause:** operators and automation otherwise need to construct local protocol requests themselves, and long-running jobs become unsafe when command parsing, daemon lifecycle, stream routing, and interruption behavior are implicit.
- **Approach:** build a side-effect-free command tree, authenticate daemon-runtime, author-agent-transport, and canonical-JSON/`1.0.0`/limits/schema provenance, validate raw bytes before decoded shape and typed conversion, externalize complete over-inline maintenance bytes under an operation-free target-qualified association/URI without truncation, authenticate caller digest and read-start facts through target-and-identifier metadata lookup, make operation-artifact or maintenance-result publication the success commit, centralize daemon observation and lossless structured-failure rendering, and pin compiled-process ownership with exhaustive golden sessions and secret scans.
- **Progress:** 20/20 tasks done; 0 blocked; 0 dropped.
- **Integration:** `in progress`; run `develop`; base `main` @ `8d286e88c06f91a1513834a4839ae36582212242`; validation base `95f1e298dbac69e2a37e48d338409fd7c1cf74d5`; mode `sequential`; final integration —.
- **Exceptions:**
  - **2502 needed the configuration crate the contract already allowed it.** Resolving a
    target is Plan 0002's selector and its closed diagnostic vocabulary; the dependency
    table already permitted the edge and the manifest simply did not have it yet, so the
    manifest joins this task's footprint.
  - **2501 needed the registry it is specified to consult.** The task requires an
    operation key for every command the catalog classifies as not intrinsically
    idempotent, and the command-line crate had no edge to the domain that publishes that
    classification. Restating the classification here would have created a second list to
    keep in step with the first, so the crate takes the inward dependency and its manifest
    joins the footprint.
    It also relaxes the scaffold suite's emptiness check, which was true of the commit
    that created the leaves and stops being true of the first one to implement any of
    them; what stays checked is the enduring claim, that every leaf opens with
    documentation of what it owns and claims nothing it has not done.
  - **2505 had to stop the formatter reordering its declarations.** The task fixes the
    command family's declaration order, and `rustfmt` sorts module declarations
    alphabetically by default, so the order could not survive the formatter the gate runs.
    `reorder_modules = false` joins `rustfmt.toml` and the task's footprint. It reformats
    nothing that exists: every other declaration list in the workspace is already
    alphabetical, and stays so because that is how it was written rather than because a
    tool insists.
  - **2706 landed its routing without its application, and 2801 found it.** The task
    requires the parsed leaves to reach their services through the production binary
    entry, and the commit that landed it delivered the routing table alone: the
    executable still offered the walking skeleton's three clap subcommands and its own
    exit taxonomy. Building the golden sessions on that would have pinned a surface the
    plan had already replaced, so the composition was finished in its own review loop
    before 2802 began. `CommandLineApplication` now holds the configuration, daemon,
    filesystem, process, clock, signal, and network boundaries and the two typed
    contracts; `command_line.rs` builds the real ones; and the entry maps every run onto
    the documented exit taxonomy.
    Four things joined the footprint. `daemon_answer.rs` is a new leaf, because reading a
    daemon's answer as an outcome is its own concern and keeping it in the assembly would
    have pushed that file past the length ceiling. `command_line.rs` is rewritten.
    `invocation.rs` gains `--operation`, `--artifact`, and `--runtime-root`, without which
    an observation leaf cannot name what it acts on and no scenario can isolate its
    runtime root. `daemon_connection.rs` gains the versioned exchange, which had no home.
    Three proofs from Plan 0001 move with the surface they describe. `daemon start` and
    `daemon ping` now write the closed outcome envelope rather than an ad-hoc object, so
    the walking skeleton, the explicit-start suite, and `README.md` say so, and the
    skeleton reads the readiness nonce from the record the daemon publishes rather than
    from standard output. That is a fix as much as a change: a nonce authorizes a
    cooperative stop, and 2803 scans exactly that stream for exactly that kind of value.
    The module-map checker also counted a process entry as a claimed module, which no
    task had tripped before because none both owned a module row and touched the entry.
  - **2804 documents what this build has.** The task lists reference sections for
    provenance digests, canonical-JSON annotations, maintenance receipt lifecycles, and
    continuation bounds. Those describe conversations with a daemon that serves versioned
    operations, and a reference is worse than useless when it describes a conversation
    the executable cannot have. What is published is every leaf, option, registry
    command, failure category, answer tag, exit, and interruption template this build
    actually has, each rendered from the metadata the executable itself reads, plus the
    prose a reader needs and generation cannot produce: that an interrupt cancels
    nothing, that publication is the success, and that a pre-receipt interruption
    promises nothing about durability. Registering the document as a product area and
    linking it from the README joins the footprint.
  - **2803 scans the channels a run has and proves its scanner before trusting it.** The
    task lists tracing and a daemon transcript among the channels. This executable emits
    no tracing of its own and the daemon it drives writes none for a client to capture,
    so what is searched is what a run actually produces: its arguments, both streams, and
    every file it left under its runtime root, in seven encodings each. A scanner that
    found nothing because it was looking wrongly would pass every scenario, so each
    encoding is proved against a helper that deliberately emits it, and each boundary is
    checked to have carried a sentinel into the process and produced output at all -
    without that, a scan of an empty stream would look like a clean one.
    The inclusive thirty-two-item bound is asserted where the bound lives, against the
    configuration's own summarizer, because the account's configuration root is chosen by
    the operating system and no scenario can make it produce thirty-three diagnostics.
  - **2802 pins the surface the compiled process actually has.** The task asks for
    sessions across admitted operations, receipts, publication races, and lost-receipt
    replay. Reaching one needs a daemon that answers a versioned operation request, and
    this one serves the retained control surface, so a session claiming an admitted
    operation would be a fiction rather than a proof. What is pinned instead is every
    byte the compiled process does produce: every leaf this build offers in both output
    forms, every refusal the parser and the services can make, the daemon lifecycle
    against absence and against a real owner, what a versioned leaf does when the daemon
    says it serves no such method, a stale nonce beside its replacement, an unresponsive
    owned child ended through its retained handle, and a real interrupt delivered to a
    run waiting on a silent endpoint - exit `130`, nothing on standard output in human
    form, one envelope on it in machine form.
    A configuration check is compared by shape rather than by bytes. Its answer depends
    on the account's own configuration root, which the operating system chooses rather
    than the environment, precisely so that nothing ambient can redirect where a
    credential is read from; committing its bytes would pin one machine. The exit, the
    single line of answer, and the five-field grammar of every diagnostic are asserted
    instead, along with the absence of a path or a home directory in any of them.
    Two defects were fixed on the way. The pre-receipt interruption template ended
    mid-sentence, so a person interrupted before a receipt was told "quoting X will say
    whether anything was" and nothing more. And a refused configuration check answered
    that the selection does not resolve while saying nothing about why, because the
    closed outcome envelope has no member for diagnostics; the diagnostics now go to the
    diagnostic stream where they belong, in Plan 0002's own five fields.
    `daemon_request.rs` joins the footprint for the same reason `daemon_answer.rs` did:
    turning an invocation into a request is its own concern, and keeping it in the
    assembly pushed that file past the length ceiling.
  - **2801 needed a handle a signal can be delivered through.** The task requires
    signal injection through a retained instance-bound primitive and forbids discovering
    or signalling a numeric process identifier. The standard library ends a child only by
    killing it outright, so the harness takes a process file descriptor at spawn and sends
    every signal through that; the same crate opens the pseudo-terminal the terminal-mode
    scenarios need. `rustix` was already the workspace's platform primitive and already
    centralized, so what joins the footprint is the test-support manifest, the capability
    policy row that now also names the `pty` feature and this owner, and the consumer
    fixture. Only Linux publishes such a descriptor; on the other supported target the
    harness retains no child at all rather than fall back to signalling a number.
- **Outcome:** Every exact-version/limits-bound catalog operation is available from the command line with canonical phrase/asset/token inputs, reaches one profile/environment daemon, survives detachment or interrupt, exposes complete maintenance results through exact target-qualified operation-free identities, and renders every registered closed result or failure byte-stably without aliases, lost fields, or secret exposure.

_Last updated: 2026-08-31, against `main` @ `8d286e8`._
