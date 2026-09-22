# Plan 0021 — Child Listings

> Read one level of a tree whatever its children are, instead of only when they are pages.

## Why this plan

`list_child_pages` answers the common case and only that case: a folder, a
fragment, an asset, and a component beside those pages are all children an
operator needs to see, and a listing that drops them is a listing that lies about
the shape of a tree. The registry had no general read for the children of an
anchor, so the one listing it had was also the only question a caller could ask.

A page listing is a typed listing with `cq:Page` as the type, so the general read
and the typed read are one contract with one filter, and the page listing is that
contract's own specialization. Adding both gives a caller every child, the
children of one primary type, or the pages, without a second question shape to
learn.

## In scope

- **0077 — The child listings.** Add `list_child_nodes` and
  `list_child_nodes_by_type` as registry commands with committed schemas,
  canonical fixtures, both sides of every bound, classification rows, a
  command-line leaf, and a protocol tool, and make `list_child_pages` the typed
  listing whose type is `cq:Page`. The three share one anchor, one window, and one
  ordering, and differ only in what they admit.

## Out of scope

The agent that carries these commands out is built in a separate repository and
is not part of this plan. No new failure category is introduced: both listings
register the shared discovery failures and the shared root-anchor failures that
`list_child_pages` already reports.
