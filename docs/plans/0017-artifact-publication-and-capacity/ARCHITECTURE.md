# Plan 0017 — Artifact Publication and Capacity

## Architectural boundary

Publication is a state transition under a pinned, verified directory handle. Staging objects are private, uniquely created, and opened without following links; the final transition is an operating-system no-replace operation, not a metadata check followed by a replacing rename. A losing publisher verifies and adopts the already committed digest object only after reading it through a stable handle.

Durability is part of success. Bytes and metadata are flushed before publication, the containing directory is synchronized afterwards, and only explicitly documented unsupported-directory-sync results may be treated differently. Other I/O failures remain failures. Startup reconciliation removes or resumes only authenticated partials owned by this namespace.

Capacity is namespace state, not caller state. A durable transaction reserves bytes before the stream is accepted. Publication converts that reservation to a blob charge and association atomically; identical verified content reuses one charge. Cancellation, oversize input, I/O failure, crash, and restart release or reconcile the reservation without relying on a process-local destructor alone.

## What proves what

Filesystem race tests use barriers at validation/publication, create regular and symbolic destinations in the gap, and run real concurrent publishers. The original destination must remain byte-identical and exactly one publisher may win. Fault injection at write, file-sync, rename, directory-sync, and accounting boundaries yields only an authenticated old or new state after reopen.

Capacity tests construct multiple repository/store instances against one namespace, race reservations at limit-minus-one, retry identical digests, interrupt streams, and restart. The durable total must always equal verified blobs plus live reservations and never exceed the contract.

## What stays outside

Operation-level result atomicity consumes this boundary later; it is not reimplemented here.
