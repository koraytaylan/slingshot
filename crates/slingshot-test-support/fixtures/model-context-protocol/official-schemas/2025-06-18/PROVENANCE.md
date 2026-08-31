# Revision 2025-06-18

This directory is where the official revision artifact belongs: the complete
unmodified document, its source location, its immutable retrieval identity, and
its cryptographic digest.

It holds no copy of that document. This environment has no network access, and
a hand-written file presented as the official artifact would be a forgery -
worse than an absent one, because every later test would cite it as authority.

What is here instead is `served-shapes.json`: this build's own declaration of
the shapes it serves, written from the revision's requirements as this build
implements them. It is digest-pinned and used as the oracle in the official
artifact's place, so the mechanism that validates every request and every
response against a committed document is real and running. When the official
artifact is retrieved, it replaces the declaration and the same tests validate
against it.
