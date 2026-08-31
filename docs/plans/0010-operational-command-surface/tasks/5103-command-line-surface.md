---
id: command-line-surface
title: "Command-Line Surface"
workstream: "0051"
kind: task
depends_on:
  - operational-command-schemas
gated: false
touches:
  - crates/slingshot-command-line/src/commands/mod.rs
  - crates/slingshot-command-line/src/commands/operational_values.rs
  - crates/slingshot-command-line/src/commands/page_lifecycle.rs
  - crates/slingshot-command-line/src/commands/asset_lifecycle.rs
  - crates/slingshot-command-line/src/commands/content_fragment.rs
  - crates/slingshot-command-line/src/commands/experience_fragment.rs
  - crates/slingshot-command-line/src/commands/platform_configuration.rs
  - crates/slingshot-command-line/src/commands/resource_mapping.rs
  - crates/slingshot-command-line/src/commands/workflow.rs
  - crates/slingshot-command-line/src/commands/sling_job.rs
  - crates/slingshot-command-line/src/commands/authorizable.rs
  - crates/slingshot-command-line/src/commands/replication_queue.rs
  - crates/slingshot-command-line/src/invocation.rs
  - crates/slingshot-command-line/src/daemon_request.rs
  - crates/slingshot-command-line/src/live_adobe_experience_manager.rs
  - crates/slingshot-command-line/tests/command_line_operational_surface.rs
status: done
merged_as: "027611437479306e2e6ba052ae770b621debcfc2"
---
# Command-Line Surface

A command nobody can type is a command that does not exist. This task gives every new registry row a leaf, the options it reads, and the builder that turns an invocation into the typed value - through the same one-list rule the existing surface already keeps, so no second table of leaf names appears beside the first.

**Steps:**

1. Add one builder module per family and register each in the builder list the request assembler asks in turn. Each module answers for its own commands and returns the another-command refusal for everything else, so the list stays the only list.
2. Add the options the architecture names to the one option table, keeping the permitted-options rule reading from the catalog rather than from a second enumeration. Split the table into its own leaf only if the option surface pushes `invocation.rs` past the source policy's line limit; a split made because a plan predicted one would be a second file for no reason.
3. Parse the composite values these commands need through validated domain constructors and never through a second grammar: the property document, the removal list, the element values, the metadata map, the state sets, the placement, the reference policy, and the inline payload each reach the domain as the value the domain declares. Read them in one leaf rather than in each family, because ten families reading the same shapes is ten chances to accept a spelling the domain refuses.
4. Refuse an option on a leaf that does not take it, by name, rather than ignoring it, which the existing rule already does and which this task must not weaken by widening an option to every catalog leaf where a family owns it.
5. Update the live-author leaf's admissible set, which reads the registry: it admits reads and nothing else, so it grows from nine to twenty-eight without a second list, and the three commands it submits stay three.

**Tests:**

- Every registry command is reachable as a leaf, in both its hyphenated and its spaced spelling, and produces the typed value the fixture states.
- Every option this plan adds is accepted on the leaves that take it and refused by name on one that does not.
- An operation key is required on every write leaf and refused on every read leaf, taken from the registry rather than a list.
- A composite value that the domain refuses is refused at the command line with the option named, for each composite kind.
- The live-author leaf admits exactly the registry's reads, refuses every write, and still refuses to run at all without its explicit enabling option.
- No file this task touches exceeds the source policy's line or complexity limits.

- **Done when:** `cargo test -p slingshot-command-line --test command_line_operational_surface` proves every new leaf, every new option's permitted set, the operation-key rule from the registry, refusal of every domain-invalid composite, and the widened live-author admissible set.
