# The Model Context Protocol server

What `slingshot protocol serve` offers a model host: which revisions it speaks,
which tools and resources it publishes, what a call answers with, and what
happens when a client stops listening or a stream goes away.

The commands these tools come from are the ones in [COMMANDS.md](COMMANDS.md);
the daemon they eventually reach is the one in [DAEMON.md](DAEMON.md).

## Starting it

```sh
slingshot --profile local --environment author protocol serve
```

The leaf takes the target and nothing else. While the server runs it owns
standard output: every line there is one protocol message, and nothing a person
would read is written beside them. Diagnostics go to standard error, and are
dropped rather than waited for when nothing is reading that stream.

## Revisions

<!-- generated: revisions -->

- `2026-07-28` (preferred)
- `2025-06-18` (offered)

<!-- end generated: revisions -->

This build's preference decides which revision a session speaks, not the
client's order. A stateless client of the current revision says which revision
it speaks on every request and nothing is remembered between them. A client of
the older revision initializes first, and everything it sends afterwards is
served in that era whatever a later request says about revisions - which is
what lets a client that sends nothing about them keep working.

A client of the older era that asks for a revision this build does not serve is
offered the older one rather than refused, because asking is how it finds out
whether there is common ground. A stateless client that asks for one is told
which two exist.

## Tools

One tool per published command, plus the controls. Every tool's safety
annotations are the registry's own classifications rather than a second
judgement, so a tool that says it may be called twice says so because the
command does.

<!-- generated: tools -->

| Tool | Read-only | Destructive | Same call twice | Operation key |
|---|---|---|---|---|
| `add_component` | false | false | false | required |
| `create_page` | false | false | false | required |
| `download_content_package` | true | false | false | required |
| `find_assets_by_metadata` | true | false | true | optional |
| `find_assets_referenced_by_page` | true | false | true | optional |
| `find_pages_by_template` | true | false | true | optional |
| `find_pages_containing_phrase` | true | false | true | optional |
| `find_pages_using_components` | true | false | true | optional |
| `inspect_open_service_gateway_initiative_configuration` | true | false | true | optional |
| `load_content_as_json` | true | false | false | required |
| `query_paths` | true | false | true | optional |
| `replicate_content` | false | true | false | required |
| `operation-list` | true | false | true | none |
| `operation-status` | true | false | true | none |
| `operation-wait` | true | false | true | none |
| `operation-restart` | false | false | true | none |
| `operation-result` | true | false | true | none |
| `operation-artifact` | true | false | true | none |
| `maintenance-preview` | true | false | true | none |
| `maintenance-apply` | false | true | true | none |

<!-- end generated: tools -->

A command the registry classifies as not intrinsically idempotent requires an
operation key. A command that may omit one gets one invented, once: the same
identifier is reused for every reconnect and retry of that request, because a
second identifier would turn a retry into a second operation.

## Resources

<!-- generated: resource-templates -->

- `slingshot://profiles/{profile}/environments/{environment}/targets/{author_target_identity_digest}/operations/{operation_identifier}`
- `slingshot://profiles/{profile}/environments/{environment}/targets/{author_target_identity_digest}/operations/{operation_identifier}/artifacts/{artifact_identifier}`
- `slingshot://profiles/{profile}/environments/{environment}/targets/{author_target_identity_digest}/maintenance/results/{maintenance_result_identifier}`

<!-- end generated: resource-templates -->

A maintenance result belongs to a target rather than to any command, so its
address names a target and an identifier and nothing else. Reading one asks for
its metadata first and checks what the read starts against what the lookup
described; the only difference admitted between the two is a current preview
becoming an application receipt at the next revision, which is what an apply
committing between the calls looks like.

No resource carries a credential, a publisher address, a filesystem path, or a
readiness nonce.

## Answers

A tool call that ran and reported a failure is a tool result, not a protocol
error: the call worked, and the failure is what it found. Only a caller waiting
on an operation that ended badly is told their call failed - an observation
that reports the same operation succeeded, because it answered the question it
was asked.

The text a client receives and the structured content beside it are the same
document, and the same bytes a command line writes for the same outcome.

<!-- generated: errors -->

| Code | When |
|---|---|
| `-32700` | The line could not be read as a message. |
| `-32600` | The line was read and is not a request. |
| `-32601` | This server offers no such method. |
| `-32602` | The arguments cannot be used. |
| `-32603` | This server failed to answer. |
| `-32022` | This build serves neither of the revisions the request names. |

<!-- end generated: errors -->

## Cancelling

Cancelling a request ends that client's interest and nothing else. The
operation keeps running, because a client walking away is not a decision about
work that may already have changed an author, and because the same client
reconnected can ask how it went. Nothing remote is ever asked to stop.

Progress reports never go backwards: a durable sequence repeats after a
reconnect, and a repeat is dropped rather than forwarded.

## What is not here

This document describes the server. It does not describe the official protocol
revisions themselves, the shape of any command's result, or the daemon's own
protocol.
