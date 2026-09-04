# Plan 0020 — Product Runtime Composition

> Turn the existing contracts, repositories, executor, catalogs, and transports into the runtime the shipped `slingshot` process actually serves.

## Why this plan

Most of the codebase implements detailed product behavior, but the production composition root does not connect it. The daemon entry acquires ownership, binds a socket, constructs a ping/stop service, publishes readiness, and serves. It advertises no operation protocol version and does not establish storage, audit target identity, install an executor, recover work, or schedule it. The CLI nevertheless performs hello and sends operation envelopes. The standard-stream protocol server similarly returns empty tool/resource lists without touching the existing catalogs or daemon client.

This is an architecture gap rather than a missing leaf function. Unit and development tests compose repositories and executors directly and can pass while the compiled product cannot reach them. Plans 0016–0019 deliberately harden the boundaries this composition will expose; this final plan wires only those hardened paths and proves behavior across real process boundaries.

## In scope

- **0075 — Production adapters and daemon.** Implement the real author-only transport behind `AuthorPorts`; resolve one immutable profile/target/trust snapshot; establish installation and database state; compose repositories, artifact sink, executor, recovery, maintenance, and a transactionally claiming scheduler; serve hello plus versioned operation envelopes; publish readiness only after all durable audits and adapters are installed.
- **0075 — Model Context Protocol application.** Compose the exact command registry, schemas, operation execution, progress/cancellation, and target-qualified resources into both supported protocol revisions instead of returning empty payloads.
- **0075 — Product diagnostics.** Install the bounded redacting diagnostic sink on daemon, child, scheduler, transport, and protocol failures while preserving protocol-only stdout and useful stderr behavior.
- **0076 — Process proof and truthful documentation.** Drive the compiled binaries through sockets and pipes against a fake author, including restart and concurrency; make README, architecture, daemon, CLI, and Model Context Protocol documentation describe only the product path the tests execute.

## Out of scope

Adding commands, protocol revisions, supported platforms, or live cloud credentials is out of scope. The fake author proves composition and wire contracts, not Adobe service availability. Hosted live-author adaptation remains owner-gated. This plan does not bypass a hardened boundary for convenience; if a prerequisite cannot support product composition, that earlier plan is incomplete.

## Review evidence at `2808354`

- `daemon_entry.rs::run_daemon_entry` performs only directory creation, namespace/ownership acquisition, endpoint bind, `DaemonService::new`, readiness publication, and `local_server::serve`. `DaemonService` stores only contract and ownership, dispatches only ping/stop, and returns an empty supported-operation-version list. `startup::establish` appears only in tests and the development chaos subject.
- Product CLI operation paths in `command_line.rs` require hello and send `OperationEnvelope`, so the real service's method-not-found/empty-version behavior prevents every catalog operation. `operation_submission_process.rs` calls repository, submission, apply, and settle modules directly despite describing the subject as product; it never starts `slingshot` or crosses the local socket.
- `author_agent_operation_executor.rs::AuthorPorts` has test implementations but no product implementation. `slingshot-agent-connection/src/lib.rs` calls itself documentation-only and contains no HTTP connection path despite transport dependencies. `operation_scheduler.rs` selects from a snapshot but does not durably claim rows, so naively attaching concurrent ticks could select one operation twice.
- `model_context_protocol/application.rs::payload_for` returns empty tools, content, resources, and templates. The shipped `protocol serve` path constructs this application with no catalogs, target, daemon, or executor; golden fixtures bless the empty listing even though Plan 0007 and product docs claim the command surface.
- `DiagnosticSink` has no product caller, while detached child stderr is discarded. `docs/DAEMON.md` both says the product installs an author-backed executor and later says none is installed; its described startup stages are not invoked. The README's no-AEM disclaimer conflicts with the exposed CLI/catalog claims. These documents cannot all describe the current binary.
