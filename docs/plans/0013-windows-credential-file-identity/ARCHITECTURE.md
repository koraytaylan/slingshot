# Plan 0013 — Windows Credential File Identity

## Architectural boundary

The change is confined to the Windows module of the configuration crate. The evidence tuple, the failure vocabulary, and every rule about who may hold rights on a credential are unchanged, and no other crate learns that this row is built differently.

## What proves what

The capability probe already establishes that the identity library reports reparse evidence, link count, volume serial number, and volume-scoped identity from a handle it opened, and that the identity is stable across two opens. What it does not establish is the property this plan exists for: that the identity, the content, and the security decision all describe one object. That is proved by replacing the object between two of those reads and requiring a refusal, on the row itself, rather than by opening one handle and trusting the interval.

The two-handle question is settled by evidence rather than by preference. Whichever handle is chosen, the suite demonstrates that the other two readers accept it, because a design where the security check cannot be performed on the chosen handle is not a design.

## What stays outside

No unchecked block enters this workspace to reach an interface a library does not expose. If the identity cannot be read safely from the chosen handle, the answer is a different handle or a different library, never an exemption: the whole target inherits a forbidden unchecked-code lint and this plan does not spend it.
