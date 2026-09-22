# Plan 0021 — Child Listings — ✅ Complete

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both.

- **Status:** ✅ Complete.
- **Goal:** read one level of a tree whatever its children are, instead of only when they are pages.
- **Root cause:** the registry had one child listing and it admitted pages alone, so a folder, fragment, asset, or component beside those pages could not be listed at all.
- **Approach:** add the general child listing and the typed child listing as registry commands with their schemas, classifications, leaves, and tools, and make the page listing the typed listing whose type is `cq:Page`.
- **Progress:** 1/1 task done; 0 blocked; 0 dropped. `list_child_nodes` and `list_child_nodes_by_type` are registered, projected, and reachable, and `list_child_pages` remains the typed listing with `cq:Page`.
- **Integration:** `planned`; run `develop`; base `main`; mode `sequential`.
- **Exceptions:** none recorded yet.
