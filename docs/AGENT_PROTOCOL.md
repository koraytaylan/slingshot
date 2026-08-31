# The author agent protocol

This daemon does no work inside Adobe Experience Manager itself. It submits a
command to an agent that runs there, watches an event stream while that agent
works, and fetches whatever the agent produced. Everything about that
conversation - which protocol versions it may use, how large each part of it may
be, when it may be believed, and what it means when it goes wrong - is written
down in one contract and enforced at the wire.

The contract is
[`policy/author-agent-transport-contract-1.json`](../policy/author-agent-transport-contract-1.json)
with its digest beside it in
[`policy/author-agent-transport-contract-1.sha256`](../policy/author-agent-transport-contract-1.sha256).
Every bound below is read from it by name. None of them is written down a second
time anywhere, which is why this document quotes the names rather than the
numbers wherever a number would otherwise have to be kept in step by hand.

## Two protocol versions, and no way out of them

An exchange with an author happens over HTTP/1.1 or HTTP/2 and over nothing
else. There is no upgrade, no alternative-service migration, and no automatic
decompression, because each of those is a way for a server to move a
conversation somewhere the policy was not applied. A response that offers one is
refused rather than followed.

Redirects are disabled everywhere, same-origin ones included. Selecting one
author origin is the whole point; a redirect is a server asking for something
else to be asked instead.

A body arrives as `identity` or not at all. A coding the client did not ask for
is a body whose decoded length nobody knows, and a bound on an unknown length is
not a bound.

## The head is bounded as it arrives

Three bounds hold one response head, and all three are enforced incrementally,
field by field, as input arrives:

| Bound | Contract name |
| --- | --- |
| Bytes one decoded field name and value occupy together | `maximum_author_response_header_bytes` |
| Fields one head may carry | `maximum_author_response_header_count` |
| Bytes the whole head may occupy | `maximum_author_response_head_bytes` |

A limit applied to a head that has already been collected is a limit on nothing:
the memory was spent before the check ran. So a response is rejected the moment
the next field would cross a bound, not once the head is in hand.

The first informational response is a rejection rather than a step: a head that
precedes the real one is a way to spend the bound twice. Exactly one final head
is read. A declared trailer is refused, because a trailer arrives after a body
this daemon has already acted on. Ambiguous framing is refused, and no partial
view of a token is ever exposed to the rest of the process.

## Deadlines are per phase, not per request

Connecting, negotiating transport security, sending a body, and reading a
response head are four different things that fail for four different reasons, so
each has its own deadline in the contract:
`author_connect_timeout_milliseconds`, `author_tls_timeout_milliseconds`,
`author_request_body_timeout_milliseconds`, and
`author_response_header_timeout_milliseconds`. A finite response has an idle and
a total deadline of its own, and an artifact transfer has another pair, because
a large download that is still moving is not a stalled one.

## What a submission leaves known

The interesting case is not success or refusal. It is the case where a request
left this machine and the answer did not come back, because that is the case
where the agent may or may not be running the command.

Any mismatch after the first byte of a `POST` has been written lands in
`SubmissionOutcome::SubmissionUnknown` with the cause that produced it. Not
knowing prompts a lookup; believing something incorrect does not. So the client
looks the operation up rather than resubmitting, and reconciles from what the
agent says rather than from what the client assumed.

An agent that cannot issue continuations is a special case of the same idea: no
recovery lookup can settle, so nothing is claimed.

## The event stream

Progress arrives as a filtered event stream on one route, asked for by
subscription identifier and by the generation of the agent's event store. A
reconnection carries `Last-Event-ID` and resumes from there; a reconnection with
no cursor exposes nothing that was already exposed and retracts nothing that
was. Reconnection delays back off by `DELAY_MULTIPLIER` from an initial delay to
a maximum, both from the contract, and stop after
`maximum_automatic_retry_attempts`.

A stream that stops sending heartbeats within `heartbeat_timeout_milliseconds`
is a stream that has stopped, which is different from a stream that has nothing
to say. The distinction is the reason the heartbeat exists.

## Continuation authority

Paged queries carry an opaque continuation token. The key state behind it is
bounded at `maximum_agent_continuation_key_state_bytes`, and that bound is
universal: there is no single-node, private, or node-local exception that would
let one deployment carry a different amount of state than another.

An agent whose continuation-key authority is not ready cannot issue tokens that
will still validate later, and discovery refuses it before a paged query starts
rather than after.

## Which contract a submission means

A command is not identified by its name. Two builds can both call something
`query_paths` and disagree about what its arguments are, what its result looks
like, or how large either may be. So an identity is five fields - the wire name,
the semantic version, the digest of the limits, and the digests of both role
schemas - and all five have to match.

The canonical-byte contract is authenticated separately, and in a fixed order. A
role schema carries an annotation naming the contract it was written under, and
that annotation is checked against the digest of the committed contract bytes
*before* the role digest is believed at all. Contract drift and annotation drift
are two different failures with two different causes, and neither can hide
inside the other. There is no sixth identity member and no provenance folded
into the five.

## What is not here

No Java agent. This repository holds the client side of the conversation and the
contract it is held to; the agent that runs inside Adobe Experience Manager is
not in this workspace, and nothing here claims to have run one.

No claim about a specific author product version. Hermetic conformance against
the fake author proves that the contract can be spoken; it says nothing about
any particular installation. Evidence about an installation comes from
`verify live-author`, which is separate, explicit, and reported as its own kind
of evidence.
