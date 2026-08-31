# Plan 0010 — Operational Command Surface

## Architectural boundary

`slingshot-domain` keeps every rule this plan adds. It performs no filesystem and no network input and output, its public command values stay valid by construction, and a result is trusted as the answer to a request only after `validate_result_for_command` compares the variant and every fact the result echoes or derives. Nothing in this plan executes a command, contacts an author, or decides how an agent should implement one.

The three boundaries above the domain change only where they must. `slingshot-command-line` gains one builder module per new family and the options those builders read. `slingshot-agent-protocol`, `slingshot-daemon`, `slingshot-agent-connection`, and the Model Context Protocol server iterate the registry and therefore inherit fifty-two rows without a per-command change; the compatibility snapshot and the reference documents are refreshed because they record what the registry currently publishes.

## What `Read` and `Write` mean after this plan

Plan 0003 defined `Read` as "changes no repository or replicated content" because nothing else was reachable through a command. Half the commands here reach something else: a bundle's lifecycle state, a workflow instance's state, a Sling job's disposition, an authorizable's existence, a replication queue's contents, an Open Service Gateway Initiative configuration's effective properties.

This plan widens the definition to state what it was always standing in for:

- `Read` means no state the author retains after the command returns is different because the command ran. Operation bookkeeping, artifact publication, and evaluation cost still do not make a command a `Write`.
- `Write` means some retained state is different. Retained state includes repository content, replicated content, configuration admin state, bundle and component lifecycle state, workflow instance state, Sling job disposition, authorizable existence and membership, and replication queue contents.
- `Destructive` keeps its meaning and gains the same widening: a success can replace or remove something that was already visible or already in effect. Refusing an existing target is not destructive; replacing an effective configuration, stopping an active bundle, terminating a running instance, or emptying a queue is.
- `IntrinsicallyIdempotent` keeps its meaning exactly. Every command this plan classifies as a `Write` is not intrinsically idempotent and therefore requires the caller's operation key, including the deletions: a deletion here refuses an absent target rather than reporting success, so running it twice is not running it once.

Every affected row is written down under the widened definition. Nothing is inferred from a name, and the twelve existing rows keep the classifications they already have, because widening the definition does not change any answer for them.

## What no command in this plan carries

Three rules are absolute across the new families, and each exists because the alternative leaks a credential through a contract that is otherwise careful about them.

- No argument accepts a password, a private key, a token, or a transport address, and no result echoes one. `create_user` therefore sets no password and produces an account that cannot authenticate until an administrator supplies a credential through a channel this contract does not provide, which is said in the command's own documentation rather than left to be discovered.
- A listing reports keys, states, counts, and closed kinds, never a value that a deployment may have chosen to put a secret in. `find_open_service_gateway_initiative_configurations` reports persistent identifiers and property-key counts; `inspect_sling_job` reports sorted property keys; `list_replication_agents` reports a closed transport kind derived from the agent's serialization type and never the agent's transport address. Reading one configuration's values stays the business of `inspect_open_service_gateway_initiative_configuration`, which does it behind the metatype evidence and redaction Plan 0003 requires.
- `update_open_service_gateway_initiative_configuration` accepts values because writing a configuration is the point of it, and answers with counts alone. What went in never comes back out.

## Shared rules

Every rule Plan 0003 set still holds: validated wrappers for non-empty lists, a named byte limit on every caller-controlled string, a named item limit on every collection, absolute repository paths under the closed grammar, closed `Initial` or `Continuation` result windows, rejected unknown fields, omitted rather than null optional members, canonical fixtures free of credentials and host paths and runtime-generated values, and errors that name the invalid field and the violated bound without echoing the value. Six rules are added by this plan.

- **A mutation answers with the address it changed.** Writes whose whole answer is one repository address share `ResourceMutationResult`, so sixteen commands do not grow sixteen ways of saying the same thing. A write that has more to report - a deletion's removed-node count, a move's adjusted-reference count, a state change's observed state - declares its own result and still carries the address.
- **A deletion refuses an absent target.** `not_found` is a failure, never a success with nothing to do, because a caller that meant a different path learns that here rather than from the content that is still present later.
- **A destructive command states its reference policy.** `delete_page`, `delete_asset`, `delete_content_fragment`, and `delete_experience_fragment` take a closed `RefuseWhenReferenced` or `IgnoreReferences` decision, and `move_page` and `move_asset` take an explicit reference-adjustment decision. There is no default, because both defaults are wrong for somebody.
- **A guard is an argument, not a convention.** `delete_authorizable` names the kind it expects, `flush_replication_queue` may name the entry count it expects, and both refuse on mismatch. A command that can remove something the caller did not mean to remove is given a way to say what it meant.
- **A listing that is not anchored in the repository is still a page.** `OperationalListingPage` reuses `ResultWindow` and `ContinuationToken` and orders its matches strictly ascending by the text identity of the row, so a listing of bundles, queues, jobs, or agents resumes exactly the way a content search does.
- **A closed state set is one enumeration.** Bundle state, component state, workflow instance state, Sling job state, replication action, and replication transport kind are each one closed enumeration in the domain, used by every command in their family, and never spelled as a free string.

## Module layout under the size rule

Sixty-four commands do not fit the shape twelve fit. Three files that are single tables today are split, and the split is by what a reader looks for rather than by line count.

- `command/catalog.rs` keeps the classification enumerations, `CommandDescriptor`, `CommandCatalog`, the two parallel `Command` and `CommandResult` enumerations the family macro builds, and `validate_result_for_command`. Every path other crates already import stays where it is.
- `command/classification.rs` holds `ClassificationRow`, the failure-category constants more than one row shares, and the single ordered sixty-four-entry table. Each entry names one constant, so the table is one place and one ascending order.
- `command/classification_foundation.rs`, `classification_authoring.rs`, `classification_platform.rs`, `classification_process.rs`, and `classification_administration.rs` declare those constants, one per command, beside the family they belong to. The original twelve move to the foundation leaf unchanged.
- `command/result_context.rs` holds every `AnswersCommand` implementation, which is one impl per command and nothing else.
- `command/schema.rs` keeps the dialect, the identifier and file-name rules, `COMMAND_WIRE_NAMES`, the shared body helpers, and the dispatch. `schema_authoring.rs`, `schema_platform.rs`, `schema_process.rs`, and `schema_administration.rs` hold the per-family bodies the dispatch asks in order.
- `slingshot-command-line` gains `commands/page_lifecycle.rs`, `asset_lifecycle.rs`, `content_fragment.rs`, `experience_fragment.rs`, `platform_configuration.rs`, `resource_mapping.rs`, `workflow.rs`, `sling_job.rs`, `authorizable.rs`, and `replication_queue.rs`, and keeps its option table in `invocation.rs`, which the thirty-eight new options leave inside the size rule.

## Operational vocabulary

Workstream 0040 lands five leaves before any command uses them.

`command/authorizable_identity.rs` declares `AuthorizableIdentifier`, which is the `rep:authorizableId` an authorizable is addressed by: non-empty, at most `MAXIMUM_AUTHORIZABLE_IDENTIFIER_BYTES`, already in normalization form C, and refusing a solidus, a control, a leading or trailing space, and the reserved forms a repository name refuses. `AuthorizableKind` is the closed `User` or `Group`. `AuthorizableIntermediatePath` is a bounded relative path of repository names under the authorizable root, refusing absolute and traversal forms.

`command/platform_service_identity.rs` declares `BundleSymbolicName` under the Open Service Gateway Initiative token grammar of full-stop-separated tokens over letters, digits, hyphen-minus, and low line; `BundleVersion` as a bounded major, minor, micro, and optional qualifier; `DeclarativeServiceComponentName` as a bounded non-empty name; and `ReplicationAgentIdentifier` and `ReplicationQueueEntryIdentifier` as bounded non-empty opaque values.

`command/process_identity.rs` declares `WorkflowModelIdentifier` and `WorkflowInstanceIdentifier` and `WorkItemIdentifier` as bounded non-empty opaque values, because an author spells them as paths today and is not required to; `SlingJobTopic` under the solidus-separated token grammar Sling requires; and `SlingJobIdentifier` and `SlingJobQueueName` as bounded non-empty values.

`command/operational_listing.rs` declares `OperationalListingPage`, the strict ascending order rule for text-keyed rows, and `ListingResultFailure` with `NotStrictlyAscending` and `NotThisRequest`.

`command/resource_mutation.rs` declares `ResourceMutationResult`, `DeletedResourceResult` with its removed-node count, `MovedResourceResult` with its source, destination, and adjusted-reference count, `ReferencePolicy` as the closed `RefuseWhenReferenced` or `IgnoreReferences`, and `InlineBinaryPayload` as a media type and standard Base64 with mandatory padding, bounded both encoded and decoded, decoded through the workspace's existing Base64 capability rather than a second decoder.

## New normative limits

Added to `schemas/command-contract-limits-1.json`, which stays the sole authority. Every value is exact, unsigned, base ten, and named once.

| Identity limit | Exact value |
|---|---:|
| `MAXIMUM_AUTHORIZABLE_IDENTIFIER_BYTES` / `MAXIMUM_AUTHORIZABLE_INTERMEDIATE_PATH_BYTES` | 255 bytes / 1,024 bytes |
| `MAXIMUM_BUNDLE_SYMBOLIC_NAME_BYTES` / `MAXIMUM_BUNDLE_VERSION_BYTES` | 512 bytes / 128 bytes |
| `MAXIMUM_DECLARATIVE_SERVICE_COMPONENT_NAME_BYTES` | 512 bytes |
| `MAXIMUM_REPLICATION_AGENT_IDENTIFIER_BYTES` / `MAXIMUM_REPLICATION_QUEUE_ENTRY_IDENTIFIER_BYTES` | 128 bytes / 256 bytes |
| `MAXIMUM_WORKFLOW_MODEL_IDENTIFIER_BYTES` / `MAXIMUM_WORKFLOW_INSTANCE_IDENTIFIER_BYTES` / `MAXIMUM_WORK_ITEM_IDENTIFIER_BYTES` | 1,024 bytes each |
| `MAXIMUM_SLING_JOB_TOPIC_BYTES` / `MAXIMUM_SLING_JOB_IDENTIFIER_BYTES` / `MAXIMUM_SLING_JOB_QUEUE_NAME_BYTES` | 512 bytes / 256 bytes / 255 bytes |

| Value and collection limit | Exact value |
|---|---:|
| `MAXIMUM_INLINE_BINARY_DECODED_BYTES` / `MAXIMUM_INLINE_BINARY_ENCODED_BYTES` | 524,288 bytes / 699,052 bytes |
| `MAXIMUM_INLINE_BINARY_MEDIA_TYPE_BYTES` / `MAXIMUM_RENDITION_NAME_BYTES` | 128 bytes / 255 bytes |
| `MAXIMUM_REMOVED_PROPERTY_NAMES` | 256 names |
| `MAXIMUM_CONTENT_FRAGMENT_ELEMENTS` / `MAXIMUM_CONTENT_FRAGMENT_ELEMENT_NAME_BYTES` | 256 elements / 255 bytes |
| `MAXIMUM_CONTENT_FRAGMENT_ELEMENT_VALUES` / `MAXIMUM_CONTENT_FRAGMENT_VARIATION_NAME_BYTES` | 256 values / 255 bytes |
| `MAXIMUM_EXPERIENCE_FRAGMENT_VARIATION_NAME_BYTES` | 255 bytes |
| `MAXIMUM_WORKFLOW_COMMENT_BYTES` / `MAXIMUM_WORKFLOW_METADATA_ENTRIES` | 4,096 bytes / 64 entries |
| `MAXIMUM_WORKFLOW_WORK_ITEMS` / `MAXIMUM_WORKFLOW_INSTANCE_STATES` | 256 items / 8 states |
| `MAXIMUM_SLING_JOB_STATES` / `MAXIMUM_SLING_JOB_PROPERTY_KEYS` | 8 states / 512 keys |
| `MAXIMUM_RESOURCE_MAPPING_PATTERN_BYTES` / `MAXIMUM_RESOURCE_MAPPING_REPLACEMENTS` | 2,048 bytes / 16 replacements |
| `MAXIMUM_REQUEST_ADDRESS_BYTES` / `MAXIMUM_RESOLUTION_TRACE_ENTRIES` | 4,096 bytes / 64 entries |
| `MAXIMUM_AUTHORIZABLE_DISABLED_REASON_BYTES` | 1,024 bytes |
| `MAXIMUM_BUNDLE_STATES` / `MAXIMUM_COMPONENT_STATES` | 8 states / 8 states |

| Budget and result limit | Exact value |
|---|---:|
| `MAXIMUM_OPERATIONAL_LISTING_RESULT_BYTES` | 1,048,576 bytes |
| `MAXIMUM_OPERATIONAL_INSPECTION_RESULT_BYTES` | 262,144 bytes |
| `MAXIMUM_DELETED_NODES` / `MAXIMUM_ADJUSTED_REFERENCES` | 100,000 nodes / 100,000 references |
| `MAXIMUM_OPERATIONAL_CANDIDATE_RECORDS` | 100,000 records |
| `MAXIMUM_REPLICATION_QUEUE_ENTRIES` | 100,000 entries |

Every new command has exact initial semantic version `1.0.0`, recorded in the manifest's version map beside the twelve that already have one.

## Shared failure categories

Four groups are named once and reused, so a caller learns them once.

- `MUTATION_COMMIT_FAILURES`: `repository_commit_failed`, `mutation_outcome_unknown`. Every repository write allows both, and the second exists because an author that stops answering mid-commit is a state nobody can report as either success or failure.
- `TARGET_ADDRESS_FAILURES`: `target_not_found`, `target_access_denied`. Every command addressed by one repository path allows both.
- `PLATFORM_CONTROL_FAILURES`: `platform_control_rejected`, `platform_control_outcome_unknown`. Every command that changes retained non-content state allows both.
- `DISCOVERY_FAILURES` and `ROOT_ANCHOR_FAILURES` are Plan 0003's and are reused unchanged by every rooted listing this plan adds.

## The sixty-four-row table

The twelve rows Plan 0003 published keep their access, destructive, and idempotency classifications and their failure categories exactly. The fifty-two rows this plan adds are classified as follows, where every `Write` requires an operation key and every `Read` refuses one.

| Command | Access | Destructive | Result bound |
|---|---|---|---|
| `add_group_member` | Write | Non-destructive | mutation success |
| `cancel_sling_job` | Write | Destructive | mutation success |
| `create_asset` | Write | Non-destructive | mutation success |
| `create_asset_folder` | Write | Non-destructive | mutation success |
| `create_content_fragment` | Write | Non-destructive | mutation success |
| `create_experience_fragment` | Write | Non-destructive | mutation success |
| `create_group` | Write | Non-destructive | mutation success |
| `create_user` | Write | Non-destructive | mutation success |
| `delete_asset` | Write | Destructive | mutation success |
| `delete_authorizable` | Write | Destructive | mutation success |
| `delete_component` | Write | Destructive | mutation success |
| `delete_content_fragment` | Write | Destructive | mutation success |
| `delete_experience_fragment` | Write | Destructive | mutation success |
| `delete_open_service_gateway_initiative_configuration` | Write | Destructive | mutation success |
| `delete_page` | Write | Destructive | mutation success |
| `find_open_service_gateway_initiative_configurations` | Read | Non-destructive | operational listing |
| `find_sling_jobs` | Read | Non-destructive | operational listing |
| `find_workflow_instances` | Read | Non-destructive | operational listing |
| `flush_replication_queue` | Write | Destructive | mutation success |
| `inspect_replication_agent` | Read | Non-destructive | operational inspection |
| `inspect_replication_queue` | Read | Non-destructive | operational listing |
| `inspect_sling_job` | Read | Non-destructive | operational inspection |
| `inspect_workflow_instance` | Read | Non-destructive | operational inspection |
| `list_asset_renditions` | Read | Non-destructive | discovery |
| `list_child_pages` | Read | Non-destructive | discovery |
| `list_group_members` | Read | Non-destructive | operational listing |
| `list_open_service_gateway_initiative_bundles` | Read | Non-destructive | operational listing |
| `list_open_service_gateway_initiative_components` | Read | Non-destructive | operational listing |
| `list_replication_agents` | Read | Non-destructive | operational listing |
| `list_resource_mappings` | Read | Non-destructive | operational listing |
| `list_sling_job_queues` | Read | Non-destructive | operational listing |
| `list_workflow_models` | Read | Non-destructive | operational listing |
| `map_resource_path` | Read | Non-destructive | operational inspection |
| `move_asset` | Write | Destructive | mutation success |
| `move_page` | Write | Destructive | mutation success |
| `read_content_fragment` | Read | Non-destructive | operational inspection |
| `remove_group_member` | Write | Destructive | mutation success |
| `reorder_component` | Write | Destructive | mutation success |
| `resolve_resource_path` | Read | Non-destructive | operational inspection |
| `retry_replication_queue_entry` | Write | Non-destructive | mutation success |
| `set_open_service_gateway_initiative_bundle_state` | Write | Destructive | mutation success |
| `set_user_disabled` | Write | Destructive | mutation success |
| `set_workflow_instance_suspension` | Write | Destructive | mutation success |
| `start_workflow` | Write | Non-destructive | mutation success |
| `terminate_workflow_instance` | Write | Destructive | mutation success |
| `update_asset_metadata` | Write | Destructive | mutation success |
| `update_component` | Write | Destructive | mutation success |
| `update_content_fragment` | Write | Destructive | mutation success |
| `update_experience_fragment` | Write | Destructive | mutation success |
| `update_open_service_gateway_initiative_configuration` | Write | Destructive | mutation success |
| `update_page` | Write | Destructive | mutation success |
| `update_user_profile` | Write | Destructive | mutation success |

Twenty-eight of the sixty-four rows are reads and thirty-six are writes. The live-author verification leaf admits reads and nothing else, so its admissible set grows from nine to twenty-eight and the three it actually submits stay three.

## Page and component lifecycle

`update_page` takes `page_path`, an optional `title`, a `properties` document under Plan 0003's JCR mutation property model, and a bounded `removed_property_names` list; it applies them to the page's content resource and answers with that resource's address. A property named in both documents is refused rather than ordered. Failures are `page_not_found`, `page_access_denied`, `page_invalid`, `property_rejected`, `property_not_removable`, and the shared commit pair.

`delete_page` takes `page_path` and a `reference_policy`. It answers with the removed address and the number of nodes removed, bounded by `MAXIMUM_DELETED_NODES`. Failures are `target_not_found`, `target_access_denied`, `target_not_a_page`, `target_is_referenced`, `deletion_budget_exceeded`, and the shared commit pair.

`move_page` takes `source_path`, `destination_path`, and `adjust_references`, and answers with both addresses and the number of references adjusted. A destination inside the source is refused before anything moves. Failures are `source_not_found`, `source_access_denied`, `destination_parent_not_found`, `destination_already_exists`, `destination_inside_source`, `reference_adjustment_budget_exceeded`, and the shared commit pair.

`list_child_pages` is a rooted discovery command over immediate children only. It takes `root_path` and a `result_window`, answers with Plan 0003's `PageMatch` page, and allows the shared discovery and root-anchor failures. Naming the argument `root_path` is deliberate: it makes the anchor failure the same failure every other rooted search reports, with the same single field.

`update_component` and `delete_component` address one component resource by absolute path. Update carries the same property and removal documents as `update_page`; delete answers with the removed address and node count. Failures are `component_not_found`, `component_access_denied`, `component_invalid`, and, for update, `property_rejected` and `property_not_removable`, each with the shared commit pair.

`reorder_component` takes `component_path` and a closed `placement` of either `before` with a `sibling_name` or `last`, and answers with the component address and the name it now follows, if any. Failures are `component_not_found`, `component_access_denied`, `parent_not_orderable`, `sibling_not_found`, and the shared commit pair.

## Asset lifecycle

`create_asset_folder` takes `parent_path`, a repository `name`, and an optional `title`. `create_asset` takes `parent_path`, `name`, an `InlineBinaryPayload`, and an optional `metadata` property document, and answers with the created address and the original rendition's byte length. The payload is refused before any decoding when its encoded length exceeds `MAXIMUM_INLINE_BINARY_ENCODED_BYTES`, and refused after decoding when its decoded length exceeds `MAXIMUM_INLINE_BINARY_DECODED_BYTES`; both bounds are checked at the limit and one step beyond it. Failures add `payload_rejected`, `payload_too_large`, and `media_type_unsupported` to the parent and target failures every creation has.

`update_asset_metadata` applies a property document to the asset's metadata resource and answers with that resource's address. `delete_asset` and `move_asset` mirror `delete_page` and `move_page` with asset-named failures. `list_asset_renditions` is a windowed listing of one asset's renditions ordered strictly ascending by rendition name, each row carrying the rendition name, its address, its media type, and its byte length under `MAXIMUM_ASSET_BYTE_LENGTH`.

## Content fragments

A fragment is addressed by its repository path and a variation is addressed by name, with the absent name meaning the master variation. `ContentFragmentElementValues` is a bounded ascending set of element names, each carrying either one bounded text value or a bounded ordered list of them, and no element name appears twice.

`create_content_fragment` takes `parent_path`, `name`, `model_path`, an optional `title`, and element values. `read_content_fragment` takes `fragment_path` and an optional `variation_name` and answers with the fragment's model path, title, variation name, and elements. `update_content_fragment` takes `fragment_path`, an optional `variation_name`, an optional `title`, and element values, and answers with the address it changed. `delete_content_fragment` takes `fragment_path` and a `reference_policy`.

Failures across the family are `fragment_not_found`, `fragment_access_denied`, `fragment_invalid`, `variation_not_found`, `model_not_found`, `model_invalid`, `element_unknown`, `element_value_rejected`, `fragment_is_referenced`, `result_budget_exceeded`, and the shared commit pair, each row allowing exactly the ones that apply to it.

## Experience fragments

`create_experience_fragment` takes `parent_path`, `name`, `template_path`, an optional `title`, and a `variation_name`, and answers with the fragment address and the created variation's address. `update_experience_fragment` addresses one variation directly by path and carries a title and a property document. `delete_experience_fragment` takes `fragment_path` and a `reference_policy`. Failures reuse the fragment names above with `template_not_found` and `template_invalid` in place of the model pair.

## Open Service Gateway Initiative platform

`find_open_service_gateway_initiative_configurations` takes an optional `persistent_identifier_prefix` and a `result_window`, and deliberately not the lookup filter the inspection command defines: that filter is bound to an exact-match lookup which refuses more than two matches, and reusing it here would be two ways to ask one question that answer differently. Each row carries the persistent identifier, the factory persistent identifier when there is one, whether the configuration is bound to a bundle location, and how many property keys it has. No row carries a property value. Failures are `configuration_lookup_failed`, `configuration_lookup_budget_exceeded`, and the shared discovery failures.

`update_open_service_gateway_initiative_configuration` takes an exact `persistent_identifier`, an `assignments` document of typed scalars and bounded sequences under the property model the inspection command already maps, and a bounded `removed_property_keys` list. It answers with the persistent identifier and the number of keys it changed. Failures are `configuration_lookup_failed`, `configuration_lookup_mismatch`, `configuration_lookup_ambiguous`, `configuration_value_unsupported`, `configuration_value_malformed`, and the shared platform-control pair.

`delete_open_service_gateway_initiative_configuration` takes an exact `persistent_identifier` and answers with it. `list_open_service_gateway_initiative_bundles` takes an optional `symbolic_name_prefix`, an optional `state` filter over the closed `installed`, `resolved`, `starting`, `active`, `stopping`, and `uninstalled` set, and a `result_window`, and orders rows strictly ascending by symbolic name. `set_open_service_gateway_initiative_bundle_state` takes a `symbolic_name` and a closed requested transition of `start`, `stop`, or `refresh`, and answers with the symbolic name and the state observed after the transition. `list_open_service_gateway_initiative_components` takes an optional `name_prefix`, an optional state filter over the closed `unsatisfied`, `satisfied`, `active`, and `disabled` set, and a `result_window`, and reports each component's name, its declaring bundle's symbolic name, its state, and its service persistent identifier when it has one.

## Resource mapping and resolution

`list_resource_mappings` answers with the mapping entries in effect, ordered strictly ascending by entry address, each carrying its address, its pattern, a closed kind of `map`, `internal_redirect`, `redirect`, or `alias`, its ordered replacements, and its status code when the kind is a redirect.

`resolve_resource_path` takes a bounded `request_address` and an `include_trace` decision, and answers with the resolved repository path when the address resolves, the resource type when there is one, the selectors, extension, and suffix the resolution produced, and, when a trace was asked for, the ordered entry addresses that decided it. `map_resource_path` takes a `repository_path` and an optional `request_authority` and answers with the external address the author would emit and the ordered entry addresses that produced it. Both refuse an address that is not a bounded absolute request address, and both allow `resolution_failed` and `resolution_budget_exceeded`.

## Workflow management

`list_workflow_models` answers with model identifier, title, and version. `start_workflow` takes a `model_identifier`, a `payload_path`, an optional `title`, an optional `comment`, and a bounded `metadata` map of bounded text, and answers with the instance identifier, the model identifier, and the instance's state. `find_workflow_instances` takes an optional `model_identifier`, an optional `payload_prefix`, a non-empty ascending set of `states` over the closed `running`, `suspended`, `aborted`, `completed`, and `stale` set, and a `result_window`; completed and aborted are the archived instances, found by the same command that finds a live one. `inspect_workflow_instance` answers with the instance's model, payload, state, and its bounded ordered work items, each carrying its identifier, its node title, and its assignee when it has one.

`terminate_workflow_instance` and `set_workflow_instance_suspension` both answer with the instance identifier and the state observed after the change. Suspension takes a closed requested state of `suspended` or `running`, so resuming is the same command as suspending and cannot disagree with it. Failures across the family are `model_not_found`, `model_invalid`, `payload_not_found`, `payload_access_denied`, `metadata_rejected`, `instance_not_found`, `instance_access_denied`, `instance_not_terminable`, `instance_not_suspendable`, `workflow_inventory_failed`, `result_budget_exceeded`, and the shared platform-control pair.

## Sling job management

`list_sling_job_queues` answers with queue name, a closed queue state of `running` or `suspended`, the active job count, and the queued job count, ordered strictly ascending by queue name. `find_sling_jobs` takes an optional `topic`, a non-empty ascending set of `states` over the closed `queued`, `active`, `succeeded`, `cancelled`, `error`, and `dropped` set, and a `result_window`, and reports job identifier, topic, state, queue name, and retry count. `inspect_sling_job` answers with those facts, the maximum retry count, and the job's property keys in ascending order and nothing else. `cancel_sling_job` answers with the job identifier and the state observed after the cancellation. Failures are `job_not_found`, `job_not_cancellable`, `job_inventory_failed`, `result_budget_exceeded`, and the shared platform-control pair.

## Users, groups, and membership

`create_user` and `create_group` take an `authorizable_identifier`, an optional `intermediate_path`, and a bounded property document, and answer with the identifier and the repository address the authorizable now has. `update_user_profile` carries a property document and a removal list against an existing user's profile. `set_user_disabled` takes the identifier, a `disabled` decision, and an optional bounded `reason`, and answers with the identifier and the disabled state observed afterwards. `delete_authorizable` takes the identifier and the `expected_kind` it means to remove, and refuses on mismatch.

`add_group_member` and `remove_group_member` take a `group_identifier` and a `member_identifier` and answer with both plus whether the membership already existed or existed at all, so a caller can tell a change from a no-op without a second request. `list_group_members` takes a `group_identifier`, an `include_indirect` decision, and a `result_window`, and reports each member's identifier, kind, address, and whether the membership is direct, ordered strictly ascending by identifier.

Failures across the family are `authorizable_already_exists`, `authorizable_not_found`, `authorizable_kind_mismatch`, `authorizable_access_denied`, `identifier_rejected`, `intermediate_path_rejected`, `property_rejected`, `membership_cycle_refused`, `group_has_members`, and the shared commit pair.

## Replication agents and queues

`list_replication_agents` and `inspect_replication_agent` report an agent's identifier, address, title, whether it is enabled, a closed transport kind of `publish`, `flush`, `reverse`, or `static`, whether its queue is blocked, and how many entries are queued; inspection adds the configured retry delay. Neither reports the agent's transport address, because that address carries the agent's credentials.

`inspect_replication_queue` is a windowed listing of one agent's queue, ordered strictly ascending by entry identifier, each row carrying the entry identifier, the content path, a closed action of `activate`, `deactivate`, `delete`, or `test`, the attempt count, and the last failure category when the entry has one, with the queue's blocked state on the page itself. `flush_replication_queue` takes an `agent_identifier` and an optional `expected_entry_count` and answers with the number of entries removed, refusing on an expectation mismatch before anything is removed. `retry_replication_queue_entry` takes an agent and an entry and answers with both and whether the entry was resubmitted. Failures are `agent_not_found`, `agent_access_denied`, `entry_not_found`, `queue_inventory_failed`, `queue_expectation_mismatch`, and the shared platform-control pair.

## Command-line surface

Every new command is a leaf named exactly by its wire name, reached through one new builder module per family, added to the builder list the request assembler already asks in turn. New options are added to the one option table, and each is permitted on catalog leaves through the same rule the existing command options use.

The options are named after ideas rather than after commands. `--path` names what a command acts on wherever it appears - a page, a component, an asset, a fragment, a variation, an anchor, a move's source - which is the habit `create_page` already set, where the path is where a new thing goes and `--name` is what it is called. `--states` is the state set every listing that has one takes, `--prefix` is the prefix every listing that filters by one takes, and a document a command carries whole - a property document, an element document, a metadata map, a set of assignments - arrives as one value the domain already declares rather than as a grammar invented here. The alternative was one option per command per idea, which would have made the reference longer without making anything clearer.

The thirty-eight options this plan adds are `--sibling`, `--media-type`, `--payload`, `--elements`, `--variation`, `--model`, `--payload-path`, `--comment`, `--metadata`, `--states`, `--prefix`, `--instance`, `--suspension`, `--job`, `--topic`, `--authorizable`, `--member`, `--group`, `--expected-kind`, `--intermediate-path`, `--disabled`, `--reason`, `--include-indirect`, `--agent`, `--entry`, `--expected-entry-count`, `--symbolic-name`, `--transition`, `--assignments`, `--removed-keys`, `--request-address`, `--request-authority`, `--include-trace`, `--operation`, `--artifact`, `--runtime-root`, `--enable-live-author`, `--content-root`.

One leaf reads them all. Ten families needing the same handful of shapes off a command line is ten chances to accept a spelling the domain refuses or refuse one it accepts, so `commands/operational_values.rs` reads them once and hands over the domain's own values. A leaf that does not take an option still refuses it by name rather than ignoring it, which is the rule that already holds.

## Documentation and compatibility

`docs/COMMANDS.md` renders its registry table from the registry, so it grows to sixty-four rows and its generated block is checked by the test that already checks it. `docs/MODEL_CONTEXT_PROTOCOL.md` records the tool count and the option keys the same way. The protocol compatibility snapshot in `slingshot-development` records the published registry and is refreshed to the sixty-four-row set; a snapshot refresh that removed or altered an existing row would be a compatibility break and the snapshot assertion is what says so. The finite-state-machine handler template names commands it dispatches and gains the new names it can dispatch.

## Where a self-contradictory request is caught

`Command::require_usable` is the one question a boundary asks about a request
that has parsed: a move into its own subtree, a mutation that changes nothing, a
group asked to contain itself, a component asked to precede itself, a reason
given for an enabling, a title written twice. The command line asks it for every
command it builds, so those requests are refused before anything is submitted.

The daemon does not ask it, and that is the boundary rather than a gap: what the
daemon receives is a canonical command as bytes with a fingerprint derived from
them, and it forwards those bytes without parsing them into a typed command. A
daemon that parsed them to re-check would be a second reading of the same
document, which is the thing the fingerprint exists to make unnecessary. The
author is the last check, and every one of these commands allows the failure
category that says the author refused it.

## What this plan does not decide

Whether an author implements a command correctly. Whether a deployment's replication agents, workflow models, or job topics exist. What a rendition should be named, what a fragment model should contain, or which properties a profile should hold. Those are the author's answers and the operator's decisions, and a contract that pretended to make them would be wrong in a different deployment.
