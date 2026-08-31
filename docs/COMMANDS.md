# Commands

What the `slingshot` executable offers, how it reads a command line, what it
answers with, and what each way of ending means. Every table below is rendered
from the same metadata the executable itself reads, so a command, an option, a
failure category, or an exit that exists in the build appears here or the build
does not pass its own gate.

The daemon this reference describes is the one in [DAEMON.md](DAEMON.md); the
configuration it reads is the one in [CONFIGURATION.md](CONFIGURATION.md).

## Reading a command line

A leaf may be written as one hyphenated word or as its words separated by
spaces: `slingshot daemon start` and `slingshot daemon-start` are the same
invocation. Options may come before the leaf or after it. `--version` and
`--help` name their leaves.

Standard output carries the answer and standard error carries everything else,
so either may be redirected without losing the other. With `--machine`, a run
writes exactly one envelope to standard output and nothing else; without it, a
run writes one line a person can read.

An option is given once. An option that belongs to another leaf is refused by
name rather than ignored, because an ignored option is a caller believing
something that is not happening.

## Commands this build offers

These reach configuration, a daemon, or nothing at all.

<!-- generated: local-leaves -->

| Leaf | Options it takes |
|---|---|
| `check-configuration` | `--profile`, `--environment`, `--machine`, `--runtime-root` |
| `protocol-serve` | `--profile`, `--environment`, `--runtime-root` |
| `daemon-ping` | `--profile`, `--environment`, `--machine`, `--runtime-root` |
| `daemon-start` | `--profile`, `--environment`, `--machine`, `--runtime-root` |
| `daemon-status` | `--profile`, `--environment`, `--machine`, `--runtime-root` |
| `daemon-stop` | `--profile`, `--environment`, `--machine`, `--runtime-root` |
| `help` | none |
| `maintenance-apply` | `--profile`, `--environment`, `--machine`, `--author-target-digest`, `--reviewed-digest`, `--runtime-root` |
| `maintenance-preview` | `--profile`, `--environment`, `--machine`, `--author-target-digest`, `--limit`, `--before`, `--runtime-root` |
| `maintenance-result` | `--profile`, `--environment`, `--machine`, `--author-target-digest`, `--result-identifier`, `--expected-digest`, `--destination`, `--runtime-root` |
| `operation-artifact` | `--profile`, `--environment`, `--machine`, `--author-target-digest`, `--expected-digest`, `--destination`, `--operation`, `--artifact`, `--runtime-root` |
| `operation-list` | `--profile`, `--environment`, `--machine`, `--author-target-digest`, `--limit`, `--before`, `--continuation-token`, `--runtime-root` |
| `operation-restart` | `--profile`, `--environment`, `--machine`, `--expected-revision`, `--expected-category`, `--operation`, `--runtime-root` |
| `operation-result` | `--profile`, `--environment`, `--machine`, `--author-target-digest`, `--operation`, `--runtime-root` |
| `operation-status` | `--profile`, `--environment`, `--machine`, `--author-target-digest`, `--operation`, `--runtime-root` |
| `operation-wait` | `--profile`, `--environment`, `--machine`, `--operation`, `--runtime-root` |
| `verify-live-author` | `--profile`, `--environment`, `--machine`, `--runtime-root`, `--enable-live-author`, `--content-root` |
| `version` | none |

<!-- end generated: local-leaves -->

One leaf is not a command at all. `protocol-serve` hands the standard streams
to the Model Context Protocol server and holds them until its input ends, so it
takes the target and nothing else: while it runs, this executable writes
protocol messages and nothing a person would read.

## Commands the registry publishes

These are submitted to a daemon, which runs them against an author. A command
the registry classifies as not intrinsically idempotent requires
`--operation-key`: without one, a rerun after a lost answer would be a second
piece of work rather than the same one. A command that is intrinsically
idempotent refuses the option, because a key that changed nothing would suggest
it did.

<!-- generated: registry-commands -->

| Command | What it does | Access | Operation key | Result bound |
|---|---|---|---|---|
| `add_component` | Add a component | Write | required | 16384 bytes |
| `add_group_member` | Add a group member | Write | required | 16384 bytes |
| `cancel_sling_job` | Cancel a Sling job | Write | required | 16384 bytes |
| `create_asset` | Create an asset | Write | required | 16384 bytes |
| `create_asset_folder` | Create an asset folder | Write | required | 16384 bytes |
| `create_content_fragment` | Create a content fragment | Write | required | 16384 bytes |
| `create_experience_fragment` | Create an experience fragment | Write | required | 16384 bytes |
| `create_group` | Create a group | Write | required | 16384 bytes |
| `create_page` | Create a page | Write | required | 16384 bytes |
| `create_user` | Create a user | Write | required | 16384 bytes |
| `delete_asset` | Delete an asset | Write | required | 16384 bytes |
| `delete_authorizable` | Delete an authorizable | Write | required | 16384 bytes |
| `delete_component` | Delete a component | Write | required | 16384 bytes |
| `delete_content_fragment` | Delete a content fragment | Write | required | 16384 bytes |
| `delete_experience_fragment` | Delete an experience fragment | Write | required | 16384 bytes |
| `delete_open_service_gateway_initiative_configuration` | Delete a configuration | Write | required | 16384 bytes |
| `delete_page` | Delete a page | Write | required | 16384 bytes |
| `download_content_package` | Download a content package | Read | required | 1048576 bytes |
| `find_assets_by_metadata` | Find assets by metadata | Read | refused | 1048576 bytes |
| `find_assets_referenced_by_page` | Find assets referenced by a page | Read | refused | 1048576 bytes |
| `find_open_service_gateway_initiative_configurations` | Find configurations | Read | refused | 1048576 bytes |
| `find_pages_by_template` | Find pages by template | Read | refused | 1048576 bytes |
| `find_pages_containing_phrase` | Find pages containing a phrase | Read | refused | 1048576 bytes |
| `find_pages_using_components` | Find pages using components | Read | refused | 1048576 bytes |
| `find_sling_jobs` | Find Sling jobs | Read | refused | 1048576 bytes |
| `find_workflow_instances` | Find workflow instances | Read | refused | 1048576 bytes |
| `flush_replication_queue` | Flush a replication queue | Write | required | 16384 bytes |
| `inspect_open_service_gateway_initiative_configuration` | Inspect a configuration | Read | refused | 1048576 bytes |
| `inspect_replication_agent` | Inspect a replication agent | Read | refused | 262144 bytes |
| `inspect_replication_queue` | Inspect a replication queue | Read | refused | 1048576 bytes |
| `inspect_sling_job` | Inspect a Sling job | Read | refused | 262144 bytes |
| `inspect_workflow_instance` | Inspect a workflow instance | Read | refused | 262144 bytes |
| `list_asset_renditions` | List asset renditions | Read | refused | 1048576 bytes |
| `list_child_pages` | List child pages | Read | refused | 1048576 bytes |
| `list_group_members` | List group members | Read | refused | 1048576 bytes |
| `list_open_service_gateway_initiative_bundles` | List bundles | Read | refused | 1048576 bytes |
| `list_open_service_gateway_initiative_components` | List components | Read | refused | 1048576 bytes |
| `list_replication_agents` | List replication agents | Read | refused | 1048576 bytes |
| `list_resource_mappings` | List resource mappings | Read | refused | 1048576 bytes |
| `list_sling_job_queues` | List Sling job queues | Read | refused | 1048576 bytes |
| `list_workflow_models` | List workflow models | Read | refused | 1048576 bytes |
| `load_content_as_json` | Load content as JSON | Read | required | 1048576 bytes |
| `map_resource_path` | Map a resource path | Read | refused | 262144 bytes |
| `move_asset` | Move an asset | Write | required | 16384 bytes |
| `move_page` | Move a page | Write | required | 16384 bytes |
| `query_paths` | Query paths | Read | refused | 1048576 bytes |
| `read_content_fragment` | Read a content fragment | Read | refused | 262144 bytes |
| `remove_group_member` | Remove a group member | Write | required | 16384 bytes |
| `reorder_component` | Reorder a component | Write | required | 16384 bytes |
| `replicate_content` | Replicate content | Write | required | 16384 bytes |
| `resolve_resource_path` | Resolve a resource path | Read | refused | 262144 bytes |
| `retry_replication_queue_entry` | Retry a replication queue entry | Write | required | 16384 bytes |
| `set_open_service_gateway_initiative_bundle_state` | Set a bundle state | Write | required | 16384 bytes |
| `set_user_disabled` | Disable or enable a user | Write | required | 16384 bytes |
| `set_workflow_instance_suspension` | Suspend or resume a workflow instance | Write | required | 16384 bytes |
| `start_workflow` | Start a workflow | Write | required | 16384 bytes |
| `terminate_workflow_instance` | Terminate a workflow instance | Write | required | 16384 bytes |
| `update_asset_metadata` | Update asset metadata | Write | required | 16384 bytes |
| `update_component` | Update a component | Write | required | 16384 bytes |
| `update_content_fragment` | Update a content fragment | Write | required | 16384 bytes |
| `update_experience_fragment` | Update an experience fragment | Write | required | 16384 bytes |
| `update_open_service_gateway_initiative_configuration` | Update a configuration | Write | required | 16384 bytes |
| `update_page` | Update a page | Write | required | 16384 bytes |
| `update_user_profile` | Update a user profile | Write | required | 16384 bytes |

<!-- end generated: registry-commands -->

## What a command can answer

A machine-readable run writes exactly one envelope, tagged with one of these
and nothing else. A consumer that meets a tag it does not know has met a build
newer than itself, and can say so precisely.

<!-- generated: outcome-tags -->

- `operation_receipt`
- `operation_status`
- `operation_result`
- `operation_terminal_error`
- `operation_recovery_required`
- `operation_resume_receipt`
- `operation_list_page`
- `command_artifact_access`
- `structured_result_artifact_access`
- `maintenance_result_access`
- `maintenance_preview`
- `configuration_report`
- `daemon_control`
- `local_application_error`

<!-- end generated: outcome-tags -->

An answer larger than the envelope allows is not truncated. It is published as
an artifact and the envelope says where to fetch it, because a truncated
envelope is not a smaller answer but an unparseable one.

## How a command can fail

Each command registers the failure categories it may report. A category travels
with the answer and never decides the exit: renaming one changes no script's
behaviour.

<!-- generated: failure-categories -->

| Command | Failure categories it registers |
|---|---|
| `add_component` | `page_not_found`, `page_invalid`, `parent_not_found`, `parent_access_denied`, `parent_not_orderable`, `target_already_exists`, `property_rejected`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `add_group_member` | `group_not_found`, `member_not_found`, `authorizable_kind_mismatch`, `authorizable_access_denied`, `membership_cycle_refused`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `cancel_sling_job` | `job_not_found`, `job_not_cancellable`, `platform_control_rejected`, `platform_control_outcome_unknown` |
| `create_asset` | `parent_not_found`, `parent_access_denied`, `target_already_exists`, `payload_rejected`, `payload_too_large`, `media_type_unsupported`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `create_asset_folder` | `parent_not_found`, `parent_access_denied`, `target_already_exists`, `property_rejected`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `create_content_fragment` | `parent_not_found`, `parent_access_denied`, `target_already_exists`, `model_not_found`, `model_invalid`, `element_unknown`, `element_value_rejected`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `create_experience_fragment` | `parent_not_found`, `parent_access_denied`, `target_already_exists`, `template_not_found`, `template_invalid`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `create_group` | `authorizable_already_exists`, `identifier_rejected`, `intermediate_path_rejected`, `property_rejected`, `authorizable_access_denied`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `create_page` | `target_already_exists`, `parent_not_found`, `parent_access_denied`, `template_not_found`, `template_invalid`, `property_rejected`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `create_user` | `authorizable_already_exists`, `identifier_rejected`, `intermediate_path_rejected`, `property_rejected`, `authorizable_access_denied`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `delete_asset` | `asset_not_found`, `asset_access_denied`, `asset_invalid`, `asset_is_referenced`, `deletion_budget_exceeded`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `delete_authorizable` | `authorizable_not_found`, `authorizable_kind_mismatch`, `authorizable_access_denied`, `group_has_members`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `delete_component` | `component_not_found`, `component_access_denied`, `component_invalid`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `delete_content_fragment` | `fragment_not_found`, `fragment_access_denied`, `fragment_invalid`, `fragment_is_referenced`, `deletion_budget_exceeded`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `delete_experience_fragment` | `fragment_not_found`, `fragment_access_denied`, `fragment_invalid`, `fragment_is_referenced`, `deletion_budget_exceeded`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `delete_open_service_gateway_initiative_configuration` | `configuration_lookup_failed`, `configuration_lookup_mismatch`, `configuration_lookup_ambiguous`, `platform_control_rejected`, `platform_control_outcome_unknown` |
| `delete_page` | `target_not_found`, `target_access_denied`, `target_not_a_page`, `target_is_referenced`, `deletion_budget_exceeded`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `download_content_package` | `pattern_rejected`, `filevault_profile_unsupported`, `filevault_filter_unrepresentable`, `root_not_found`, `root_access_denied`, `repository_read_failed`, `filevault_package_failed`, `staging_cleanup_failed`, `artifact_publication_failed`, `artifact_publication_outcome_unknown`, `evaluation_budget_exceeded` |
| `find_assets_by_metadata` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `root_not_found`, `root_access_denied` |
| `find_assets_referenced_by_page` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `page_not_found`, `page_access_denied`, `page_invalid` |
| `find_open_service_gateway_initiative_configurations` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `configuration_lookup_failed`, `configuration_lookup_budget_exceeded` |
| `find_pages_by_template` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `root_not_found`, `root_access_denied` |
| `find_pages_containing_phrase` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `root_not_found`, `root_access_denied` |
| `find_pages_using_components` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `root_not_found`, `root_access_denied` |
| `find_sling_jobs` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `job_inventory_failed` |
| `find_workflow_instances` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `workflow_inventory_failed` |
| `flush_replication_queue` | `agent_not_found`, `agent_access_denied`, `queue_expectation_mismatch`, `platform_control_rejected`, `platform_control_outcome_unknown` |
| `inspect_open_service_gateway_initiative_configuration` | `configuration_lookup_failed`, `configuration_lookup_mismatch`, `configuration_lookup_ambiguous`, `configuration_lookup_budget_exceeded`, `configuration_value_unsupported`, `configuration_value_malformed`, `configuration_value_budget_exceeded`, `configuration_result_budget_exceeded` |
| `inspect_replication_agent` | `agent_not_found`, `agent_access_denied`, `agent_inventory_failed` |
| `inspect_replication_queue` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `agent_not_found`, `agent_access_denied`, `queue_inventory_failed` |
| `inspect_sling_job` | `job_not_found`, `job_inventory_failed`, `result_budget_exceeded` |
| `inspect_workflow_instance` | `instance_not_found`, `instance_access_denied`, `workflow_inventory_failed`, `result_budget_exceeded` |
| `list_asset_renditions` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `asset_not_found`, `asset_access_denied`, `asset_invalid` |
| `list_child_pages` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `root_not_found`, `root_access_denied` |
| `list_group_members` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `group_not_found`, `authorizable_kind_mismatch`, `authorizable_access_denied` |
| `list_open_service_gateway_initiative_bundles` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `bundle_inventory_failed` |
| `list_open_service_gateway_initiative_components` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `component_inventory_failed` |
| `list_replication_agents` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `agent_inventory_failed` |
| `list_resource_mappings` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `mapping_inventory_failed` |
| `list_sling_job_queues` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `job_inventory_failed` |
| `list_workflow_models` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `workflow_inventory_failed` |
| `load_content_as_json` | `not_found`, `access_denied`, `unsupported_repository_value`, `load_budget_exceeded` |
| `map_resource_path` | `resolution_failed`, `resolution_budget_exceeded` |
| `move_asset` | `source_not_found`, `source_access_denied`, `destination_parent_not_found`, `destination_already_exists`, `destination_inside_source`, `reference_adjustment_budget_exceeded`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `move_page` | `source_not_found`, `source_access_denied`, `destination_parent_not_found`, `destination_already_exists`, `destination_inside_source`, `reference_adjustment_budget_exceeded`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `query_paths` | `discovery_budget_exceeded`, `continuation_token_malformed`, `continuation_token_integrity_invalid`, `continuation_token_wrong_target`, `continuation_token_wrong_query`, `continuation_token_expired`, `root_not_found`, `root_access_denied` |
| `read_content_fragment` | `fragment_not_found`, `fragment_access_denied`, `fragment_invalid`, `variation_not_found`, `result_budget_exceeded` |
| `remove_group_member` | `group_not_found`, `member_not_found`, `authorizable_kind_mismatch`, `authorizable_access_denied`, `membership_cycle_refused`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `reorder_component` | `component_not_found`, `component_access_denied`, `parent_not_orderable`, `sibling_not_found`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `replicate_content` | `source_not_found`, `source_access_denied`, `candidate_limit_exceeded`, `traversal_budget_exceeded`, `admission_rejected`, `admission_budget_exceeded`, `admission_outcome_unknown` |
| `resolve_resource_path` | `resolution_failed`, `resolution_budget_exceeded`, `request_address_rejected` |
| `retry_replication_queue_entry` | `agent_not_found`, `agent_access_denied`, `entry_not_found`, `platform_control_rejected`, `platform_control_outcome_unknown` |
| `set_open_service_gateway_initiative_bundle_state` | `bundle_not_found`, `bundle_transition_refused`, `platform_control_rejected`, `platform_control_outcome_unknown` |
| `set_user_disabled` | `authorizable_not_found`, `authorizable_kind_mismatch`, `authorizable_access_denied`, `platform_control_rejected`, `platform_control_outcome_unknown` |
| `set_workflow_instance_suspension` | `instance_not_found`, `instance_access_denied`, `instance_not_suspendable`, `platform_control_rejected`, `platform_control_outcome_unknown` |
| `start_workflow` | `model_not_found`, `model_invalid`, `payload_not_found`, `payload_access_denied`, `metadata_rejected`, `platform_control_rejected`, `platform_control_outcome_unknown` |
| `terminate_workflow_instance` | `instance_not_found`, `instance_access_denied`, `instance_not_terminable`, `platform_control_rejected`, `platform_control_outcome_unknown` |
| `update_asset_metadata` | `asset_not_found`, `asset_access_denied`, `asset_invalid`, `property_rejected`, `property_not_removable`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `update_component` | `component_not_found`, `component_access_denied`, `component_invalid`, `property_rejected`, `property_not_removable`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `update_content_fragment` | `fragment_not_found`, `fragment_access_denied`, `fragment_invalid`, `variation_not_found`, `element_unknown`, `element_value_rejected`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `update_experience_fragment` | `variation_not_found`, `variation_access_denied`, `variation_invalid`, `property_rejected`, `property_not_removable`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `update_open_service_gateway_initiative_configuration` | `configuration_lookup_failed`, `configuration_lookup_mismatch`, `configuration_lookup_ambiguous`, `configuration_value_unsupported`, `configuration_value_malformed`, `platform_control_rejected`, `platform_control_outcome_unknown` |
| `update_page` | `page_not_found`, `page_access_denied`, `page_invalid`, `property_rejected`, `property_not_removable`, `repository_commit_failed`, `mutation_outcome_unknown` |
| `update_user_profile` | `authorizable_not_found`, `authorizable_kind_mismatch`, `authorizable_access_denied`, `property_rejected`, `property_not_removable`, `repository_commit_failed`, `mutation_outcome_unknown` |

<!-- end generated: failure-categories -->

## Exits

<!-- generated: exits -->

| Exit | What it means |
|---|---|
| `0` | The command finished and its answer is on standard output. |
| `2` | The invocation is wrong. Nothing was reached and nothing was changed. |
| `3` | The author refused the work. It provably did not run. |
| `4` | The work ran and failed. |
| `5` | Nobody can say whether the work ran. Running it again risks running it twice. |
| `6` | What the command needed was not there, or would not agree with this build. |
| `7` | Something on this machine failed. Nothing remote is claimed. |
| `130` | Somebody asked the run to stop. Nothing remote was asked to stop. |

<!-- end generated: exits -->

Two exits tell a script it may run the same command again: a refusal ran
nothing, and a usage mistake never reached anything. An indeterminate outcome
does not, because running it again is exactly the risk that disposition
describes.

## Interruption

An interrupt stops this process and nothing else. No remote work is cancelled,
no operation is abandoned, and an operation that was admitted keeps running
until it ends; running the same command again with the same operation key
returns to it.

What an interrupted run prints depends on how far it got, because that is what
it can honestly say:

<!-- generated: interruption-templates -->

- Before the daemon answered: `interrupted before the daemon answered; quoting {identifier} will say whether anything was admitted`
- After it answered: `interrupted while watching {identifier}; the operation is running and can be watched again`
- While fetching: `interrupted while fetching {identifier}; nothing was written where it was going, and running the same command again resumes it`

<!-- end generated: interruption-templates -->

A pre-receipt interruption claims nothing about durability: the daemon may or
may not have admitted the work, and quoting the identifier is how a caller
finds out. A run interrupted before a receipt writes nothing to standard
output, so a pipeline cannot read a partial answer as an answer.

Once an artifact or a maintenance result has been published, publication is the
success: a signal at or after it cannot turn a finished thing into an
interrupted one, and an identical fresh invocation re-renders that success from
the receipt rather than fetching or publishing anything again.

## Examples

```sh
# What this build is.
slingshot --version

# What this account's configuration resolves to, reaching no daemon.
slingshot --profile local --environment author check-configuration

# Reach the daemon that owns a target, creating it if nobody has.
slingshot --profile local --environment author daemon start

# Report whether one already owns it. This never creates one.
slingshot --profile local --environment author daemon ping

# Read a subtree, naming the key that makes a rerun the same request.
slingshot --profile local --environment author load_content_as_json --path /content/site/en --operation-key one-read

# Publish a subtree, and return without waiting for it.
slingshot --profile local --environment author replicate_content --path /content/site/en --operation-key one-publication --detach

# Ask where an operation has got to.
slingshot --profile local --environment author operation-status --operation one-publication

# Wait for it, in a form a script can read.
slingshot --profile local --environment author operation-wait --operation one-publication --machine

# List what a target holds, a page at a time.
slingshot --profile local --environment author operation-list --limit 50

# Ask what a maintenance run would remove, before removing it.
slingshot --profile local --environment author maintenance-preview --limit 20

# Stop the daemon that owns the target, quoting the nonce it published.
slingshot --profile local --environment author daemon stop

# Verify the read path against a real author, which happens only when asked.
slingshot --profile local --environment author verify live-author --enable-live-author --content-root /content/site/en
```

## Verifying against a real author

`verify live-author` reaches the author the selection names and runs the read
path against it. Nothing about it is implicit: without `--enable-live-author`
the leaf is refused before a single byte of configuration is read, and the
`--content-root` it is given has to name authored content rather than a
directory on this machine.

What it may run is the registry's own answer. A command is admissible when the
registry calls it a read that replaces nothing, which is twenty-eight of the
sixty-four rows; the thirty-six that write are refused before anything is
dispatched. Whether
running a command twice is running it once never enters into that decision -
that column says whether a retry is safe, not whether a run may happen.

One run of it is evidence about the author it ran against and about nothing
else. It is not the release gate: the hermetic suite is, and the two kinds of
evidence are reported separately rather than added together.

## What is not here

This reference describes the executable. It does not describe the author it
eventually reaches, the shape of any command's result document, or the
protocol two Slingshot processes speak to each other - those are the registry's
schemas and [DAEMON.md](DAEMON.md) respectively.
