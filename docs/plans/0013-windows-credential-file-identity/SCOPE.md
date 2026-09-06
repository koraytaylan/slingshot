# Plan 0013 — Windows Credential File Identity

> Read a credential's identity on Windows through an interface that exists, from the one handle the rest of the check already holds.

## Why this plan

The Windows row has never compiled. `support/platforms.toml` names it supported, the automation authority maps it to a runner, and Plan 0009 requires per-row native evidence for every supported row - and no build has ever produced any, because the credential filesystem reads a file's identity through four standard-library interfaces that are not stable. A workspace pinned to a released compiler cannot call them, so the row fails before it reaches a test.

The workspace already declares the capability that answers three of the four. Its row says, in its own words, to read reparse evidence, the link count, the volume serial number, and the file identifier from a handle, and its probe does exactly that and passes. The product does not use it. What the probe never claimed, and what no version of that library offers, is a change time; the product invented one from an unstable interface instead.

That leaves a real architectural question rather than a substitution. The identity has to come from the same handle the content is read through and the same handle the owner and permissions are checked on - that sameness is the whole point of the check, because two handles opened a moment apart are two chances for the object to have been swapped. Today the content and the security check share a standard-library handle, and the capability that can report identity opens its own. One of those has to give.

Until it does, the release matrix claims a row it cannot build.

## In scope

- **0056 — One handle, one identity.** Read the credential's identity through a stable interface, from the same handle that reads its content and the same handle its owner and permissions are checked on. Decide that handle deliberately: either the identity capability opens the object and the content and security checks are taken from that handle, or the standard-library handle is kept and the identity is obtained through an interface that accepts it. Prove the sameness rather than assert it: an object replaced between the identity read and the content read is refused, and the suite demonstrates the refusal by replacing it.
- **0057 — The second time, or its absence.** The evidence tuple carries two times, and on Windows one of them has no stable source. Decide what the row reports and record the decision where a reader meets it: a second time that Windows can actually produce and that changes when the contract needs it to, or a tuple that names one time on that row and says why. Whichever it is, the platform-independent shape stays a shape a reader can compare, and the fixtures pin what each row reports.

## Out of scope

The other two rows are unchanged: this plan touches Windows behaviour only, and a change that altered what Linux or macOS reports is a defect in it. The security check itself is not reopened - which principals may hold which rights, and the refusals around them, stay exactly as they are. Whether Windows is a supported target at all is an owner's decision recorded in the platform matrix, not something this plan argues.
