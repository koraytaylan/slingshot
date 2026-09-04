# Plan 0020 — Product Runtime Composition

## Architectural boundary

One `ProductRuntime` composition root owns the immutable resolved profile snapshot, verified target and two route-specific trust-policy identities, runtime namespace, installed storage, repositories, artifact completion, author transport, executor, scheduler, cancellation tree, and diagnostics. No product entry point reconstructs one of these independently.

Startup order is security- and recovery-significant: resolve and authenticate configuration; acquire namespace/installation locks; establish and migrate through the hardened storage factory; audit foreign unfinished state; install transport/executor and recover durable work; start the transactionally claiming scheduler; bind service; then publish readiness. Failure before readiness unwinds owned resources and leaves durable state recoverable.

The local service has a control plane and a versioned operation plane. Hello advertises only installed operation versions. Dispatch validates envelope/command identity and persists admission before scheduling. Scheduler ticks atomically claim an eligible operation with a lease/fence before execution; overlapping ticks and restart can retry physical work but cannot claim one logical effect concurrently.

The standard-stream server is an adapter over the same command and daemon boundaries. Tool/resource listings are projections of the registries, calls use canonical operation execution, and reads use target-qualified operation/maintenance artifact access. Current stateless and initialized legacy revisions change envelope decoration, not product capability.

## What proves what

Process tests launch the compiled binary with isolated real directories and a protocol-faithful fake author server. They perform hello, submit through the framed local socket, observe progress/result/artifacts, stop/restart, and verify recovered state. Every catalog descriptor is compared exactly with CLI and Model Context Protocol discovery and driven through at least schema refusal plus fake-agent success/failure.

Concurrency tests overlap scheduler ticks and crash after durable claim, remote send, receipt, settlement, and output. Fences and Plan 0019 state transitions permit one consistent logical result. Diagnostics tests inject unique secrets and failures through every product adapter, then scan stderr and rotated files while asserting stdout remains protocol-only.

## What stays outside

The composition root does not contain domain behavior. It owns lifetimes and connects ports to already-tested implementations.
