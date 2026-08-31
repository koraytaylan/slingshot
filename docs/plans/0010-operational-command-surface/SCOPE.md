# Plan 0010 — Operational Command Surface

> Grow the published command registry from twelve rows to sixty-four, so that authoring, platform, process, and administration work an operator does through the Adobe Experience Manager consoles is expressible as one bounded typed command.

## Why this plan

Plan 0003 published twelve commands. They cover reading content, finding it, packaging it, replicating it, and creating a page or a component. That is one console out of many. An operator who has to look at an Open Service Gateway Initiative configuration, work out why a request resolved to the wrong resource, restart a stalled workflow, cancel a Sling job, disable a user, or unblock a replication queue leaves this executable and opens a browser, which is exactly the boundary Slingshot exists to remove.

Twelve is also lopsided in a way that shows in use. `create_page` exists and there is no way to change or remove what it created; assets can be searched and never written; a configuration can be read one persistent identifier at a time and never listed; replication can be offered content and never asked what happened to it. Each of those is a half of a pair, and the missing half is the one an operator reaches for second.

This plan adds fifty-two commands across ten families, keeps every one of them inside the contract Plan 0003 established, and changes exactly one thing about that contract: what `Read` and `Write` mean. Twelve commands could define access as whether repository or replicated content changes, because nothing else was reachable. Starting a bundle, cancelling a job, and disabling a user change none of that and are plainly not reads, so the classification is widened to any state the author retains after the command returns, and every affected row is written down under the widened definition rather than inferred from it.

Nothing here executes a command. The domain crate holds meaning and the separately built Java agent holds behavior, exactly as Plan 0003 arranged it. What lands here is the vocabulary, the bounds, the schemas, the registry rows, the command-line surface, and the reference documentation, so that a command that exists in the build appears in the reference and in the Model Context Protocol tool list or the build does not pass its own gate.

## In scope

- **0040 — Operational vocabulary.** Validated identifiers for the things the new families address and cannot borrow from a repository path: authorizables and their intermediate paths, bundle symbolic names and versions, declarative service component names, replication agent and queue-entry identifiers, workflow model and instance and work-item identifiers, Sling job topics and identifiers and queue names. One shared bounded listing page for results that are not anchored in the repository, one shared mutation result for writes whose whole answer is the address they changed, and one bounded inline binary payload for the single command that carries bytes inward.
- **0041 — Page and component lifecycle.** `update_page`, `delete_page`, `move_page`, `list_child_pages`, `update_component`, `delete_component`, and `reorder_component`. Deletion and movement state a reference policy rather than assuming one, and every mutation answers with the address it changed.
- **0042 — Asset lifecycle.** `create_asset_folder`, `create_asset`, `update_asset_metadata`, `delete_asset`, `move_asset`, and `list_asset_renditions`. Asset creation carries its bytes inline under an exact bound and refuses anything larger rather than silently truncating or inventing a staging protocol this commit does not have.
- **0043 — Content fragments.** `create_content_fragment`, `read_content_fragment`, `update_content_fragment`, and `delete_content_fragment`, addressed by fragment path and variation name, with element values held to the model the fragment declares.
- **0044 — Experience fragments.** `create_experience_fragment`, `update_experience_fragment`, and `delete_experience_fragment`, addressed by fragment path and variation path.
- **0045 — Open Service Gateway Initiative platform.** `find_open_service_gateway_initiative_configurations`, `update_open_service_gateway_initiative_configuration`, `delete_open_service_gateway_initiative_configuration`, `list_open_service_gateway_initiative_bundles`, `set_open_service_gateway_initiative_bundle_state`, and `list_open_service_gateway_initiative_components`. Listing exposes keys, states, and counts and never a configuration value, because the redaction evidence Plan 0003 requires before a value is read is a per-identifier judgement and a listing has not made it.
- **0046 — Resource mapping and resolution.** `list_resource_mappings`, `resolve_resource_path`, and `map_resource_path`, so the question "why did this address reach that resource" is answerable with the entries that decided it.
- **0047 — Workflow management.** `list_workflow_models`, `start_workflow`, `find_workflow_instances`, `inspect_workflow_instance`, `terminate_workflow_instance`, and `set_workflow_instance_suspension`. Running, suspended, aborted, completed, and stale are one closed state set, so an archived instance is found by the same command that finds a live one.
- **0048 — Sling job management.** `list_sling_job_queues`, `find_sling_jobs`, `inspect_sling_job`, and `cancel_sling_job`. A job's own properties are reported as sorted keys and never as values, for the reason the configuration listing reports none.
- **0049 — Users, groups, and membership.** `create_user`, `create_group`, `update_user_profile`, `set_user_disabled`, `delete_authorizable`, `add_group_member`, `remove_group_member`, and `list_group_members`. No command in this family transports a credential, and creation therefore sets no password.
- **0050 — Replication agents and queues.** `list_replication_agents`, `inspect_replication_agent`, `inspect_replication_queue`, `flush_replication_queue`, and `retry_replication_queue_entry`. An agent's transport is reported as a closed kind and never as its address, because an agent's transport address carries its credentials.
- **0051 — Registry, schemas, surfaces, and reference.** One sixty-four-row registry in ascending wire-name order under the widened access definition, both role schemas for every new command with their committed bytes and digests, the command-line options and builders that construct them, and the reference tables the build renders and checks.

## Out of scope

- Executing any of these commands. The Java agent implements them; this plan defines what it is held to, and the conformance vectors it is held to.
- Transporting a credential. No command here accepts a password, a private key, a token, or a transport address, and no result echoes one. A user created here cannot authenticate until an administrator supplies a credential through a channel this contract does not provide.
- Uploading a binary larger than the inline payload bound. An inbound artifact staging protocol is a transport question, not a vocabulary question, and inventing one here would put an unproven file-transfer path inside a command contract.
- Free-form scripting consoles, repository query passthrough, and anything that hands an author a program to run. The registry stays a closed set of structured requests.
- Changing what the twelve existing commands mean. Their arguments, results, schemas, and semantic versions are untouched; only their neighbours in the table are new.
- Reindexing, translation projects, live copies, and audit purging. They are recorded below as considered and deferred rather than dropped.

## Considered and deferred

Named here so a later plan inherits the reasoning rather than the idea. Access control inspection and effective-permission evaluation; Oak index listing and query explanation; health-check listing and execution; content versioning; tag taxonomy administration; content package listing, building, installing, and deleting; dispatcher cache invalidation as a command distinct from a flush agent; asset rendition creation and reprocessing; and Multi Site Manager rollout. Each is a coherent family on its own and none of them is needed to close a pair this plan opens.

## Plan dependencies

Plan 0003 provides the command model, the limits manifest, the canonical byte contract, the schema machinery, and the registry this plan extends. Plan 0006 provides the command-line adapter whose option table and builder list this plan adds to. Plan 0007 derives its tool list from the registry and needs no per-command change. Plans 0004, 0005, 0008, and 0009 consume the registry through interfaces that iterate it, so they inherit the new rows without a change of their own beyond the compatibility snapshot this plan refreshes.
