# Plan 0003 — Command Contracts and Catalog

> Express every supported Slingshot operation as a bounded, typed, transport-independent command and publish one catalog from those definitions.

## Why this plan

The command line, local daemon protocol, author-agent wire protocol, Model Context Protocol server, and FSM workflows all need the same answer to three questions: what a command is called, which arguments it accepts, and what result it returns. Defining those answers separately at each boundary creates drift that tests cannot reliably detect after the fact.

This plan places the command contracts in slingshot-domain before any remote transport exists. Each requested operation lands as an independently tested type with canonical JSON fixtures and independently authored language-neutral agent-conformance scenarios. Rust tests separately validate raw canonical bytes and typed collection order, standard-schema-expressible decoded shape, typed semantics, and complete scenario inventories; repository execution remains the responsibility of the external Java slingshot-agent implementation that consumes those scenarios.

The command contracts are structured data. Natural-language examples describe user intent, but Slingshot does not parse free-form prose. Command-line flags and Model Context Protocol arguments construct the same Rust values.

## In scope

- **0009 — Command Foundations.** Create the compiling command module layout plus one versioned normative limits manifest, exact ABNF and charging for bounded canonical Semantic Versioning including legal numeric build leading zeros, exact `1.0.0` command versions, validated repository paths, continuation-aware bounded result windows with provider/topology/storage/profile-neutral durable continuation-key-authority ownership assigned to Plan 0005, a JCR-only mutation property model, exact operator-specific search predicates, shared maximum-length artifact descriptors/slots, and full-word Rust identifiers.
- **0010 — Inspection Commands.** Define independently tested commands for finite-traversal repository-content loading as JSON with one exact canonical-document Inline/Artifact boundary and side-effect-free exact-persistent-identifier Open Service Gateway Initiative configuration inspection with a single property-Dictionary acquisition, complete keys-only snapshot, closed ordinary/factory PID, provider, bundle-location, default-locale, designate-inventory, and attribute-definition applicability algorithm that redacts every absent/ambiguous/failed metatype observation before any value access.
- **0011 — Discovery Commands.** Define independently tested structured commands for path queries, page phrase/template/component discovery, asset metadata discovery with one exact nonnegative signed-64-bit request/result byte-length type, and assets referenced by a page, with exact no-effect anchor failures, canonical request-set/phrase spellings, deterministic page-at-a-time ordering, and named per-job computation budgets.
- **0012 — Action Commands.** Define independently tested bounded replication, valid FileVault content-package download, page creation, and component addition contracts; package selection uses a restricted absolute segment-expression grammar and deterministic bounded dynamic-programming matcher, while a separate finite-set FileVault generator quotes Java regular expressions/XML without widening selection and negotiates one exact supported import profile. Component addition requires an orderable parent before mutation.
- **0013 — Registry and Schemas.** Publish one closed descriptor registry whose wire names are the sole capability names, carrying exact `1.0.0` versions, the normative limits-manifest digest, the exact twelve-row read/write/destructive/intrinsic-idempotency authority, separate argument/result schema digests bound to one machine-readable canonical-byte contract, request-context validation of every checkable echoed/derived invariant, ordinary Draft 2020-12 decoded-shape schemas, a separate language-neutral raw-byte/typed-order validator and conformance set, and completeness proofs binding every command to its fixtures and external-agent scenarios. Plan 0005 adds the authenticated canonical submitted-command digest and owns continuation-token cryptography/key lifecycle needed to bind and resume remote execution.

## Out of scope

- Java servlet, Sling Job, repository query, replication, package-building, page-authoring, and component-authoring implementations.
- Natural-language command parsing.
- Network submission, server-sent events, authentication, daemon durability, command-line rendering, and Model Context Protocol handling.
- Repository traversal and package construction inside the separately built Java agent; this plan implements the pure bounded matcher in Rust and requires the agent to pass the same language-neutral vectors without invoking a regular-expression engine.
- Raw JCR-SQL2 passthrough. The plan exposes structured predicates whose translation belongs to the Java agent.

## Plan dependencies

Plan 0001 provides the workspace and crate skeletons. Plan 0002 provides profile and credential configuration but is not needed by the pure command model. Plans 0004 through 0008 consume the registry and schemas produced here.
