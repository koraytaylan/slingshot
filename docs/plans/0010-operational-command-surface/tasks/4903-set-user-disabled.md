---
id: set-user-disabled
title: "Disable and Enable a User"
workstream: "0049"
kind: task
depends_on:
  - update-user-profile
gated: false
touches:
  - crates/slingshot-domain/src/command/set_user_disabled.rs
  - crates/slingshot-domain/src/command/mod.rs
  - crates/slingshot-domain/tests/fixtures/command-module-inventory.txt
  - crates/slingshot-domain/tests/set_user_disabled.rs
  - "crates/slingshot-domain/tests/fixtures/commands/set_user_disabled/**"
status: planned
merged_as: ""
---
# Disable and Enable a User

Disabling an account is the administrative action with the shortest time pressure and the least room for ambiguity, so it is one command with a boolean rather than two commands that could disagree about what the other means.

**Steps:**

1. Commit canonical accepted and refused argument fixtures and exact no-effect failure documents before the implementation, one line per vector, each carrying the note that says what it proves.
2. Implement `SetUserDisabledCommand` with an `authorizable_identifier`, a required `disabled` decision, and an optional `reason` bounded by `MAXIMUM_AUTHORIZABLE_DISABLED_REASON_BYTES`.
3. Refuse a reason when the request enables rather than disables, because a reason for an enabling is a value the author would keep and nobody would read.
4. Implement the result carrying the identifier and the disabled state observed afterwards.
5. Allow exactly `authorizable_not_found`, `authorizable_kind_mismatch`, `authorizable_access_denied`, `platform_control_rejected`, and `platform_control_outcome_unknown`.
6. Supply request-context validation that refuses a result naming another identifier.

**Tests:**

- Both decisions round-trip byte-identically, and an absent `disabled` member is refused.
- A reason beside an enabling request is refused; a reason beside a disabling request is accepted and proved at its bound and one past it.
- A group identifier produces `authorizable_kind_mismatch` rather than a success.
- Each failure document carries exactly its discriminator and `authorizable_identifier` and proves no effect.
- A result naming another identifier is refused.

- **Done when:** `cargo test -p slingshot-domain --test set_user_disabled` proves both decisions, the reason rule in both directions, both sides of the reason bound, and every closed failure.
