/goal

Rewrite Jackin as an OpenShell-only product.
Make a breaking change.
Do not preserve backward compatibility.
Complete the full plan in [docs/openshell-v2/PLAN.md](./PLAN.md).

## Read the inputs

Read [SPEC.md](./SPEC.md) fully.
Read [SOURCE-AUDIT.md](./SOURCE-AUDIT.md), [CONTRACTS.md](./CONTRACTS.md), [SUBAGENTS.md](./SUBAGENTS.md), [PLAN.md](./PLAN.md), [ACCEPTANCE.md](./ACCEPTANCE.md), and [TERMS.md](./TERMS.md) in the same directory.
Use [task-graph.json](./task-graph.json) for task dependencies.
Do not treat earlier chat answers as source evidence.

The specification is the product authority.
Use the qualified OpenShell source for actual API behavior.
Treat historical experiments as regression inputs only.
Record source conflicts before you change code.

## Required result

Make OpenShell the only execution control plane for agents and independent verification workers.
Keep roles, selected agents, accounts, projects, Capsule sessions, retained work, CLI commands, and Console workflows.
Use one OpenShell sandbox for each account, policy, writable-data, and trust set.
Use a separate credential-free domain for the default project shell.

Keep Docker and Buildx as trusted image builders.
Do not use them as another agent runtime.
Build the complete declared agent set into each resolved role image.
Use immutable image identities and explicit builder endpoints.

Rewrite Capsule as an unprivileged resident session service.
Use OpenShell exec for short-lived Capsule relays.
Keep PTYs alive after client detach.
Do not restart old agent commands after a Capsule boot change.

Support explicit local, VM, and Kubernetes target profiles.
Keep gateway registration and authentication in OpenShell.
Pass an explicit gateway and workspace with every operation.
Do not change the global active gateway to route concurrent requests.

Preserve project data and approved conversation state.
Use verified materialization, conflict-aware export, independent checkpoints, and explicit handoff.
Protect the original checkout and unrelated resources.

Implement the required upstream contracts.
These contracts include mutation fencing, strict verification, rebuilt network programs, and the managed engine companion.
Do not remove these requirements to make the release pass.

## Remove the old implementation

Delete direct Docker and Apple agent execution.
Delete backend selectors, old runtime records, compatibility shims, old-state importers, and legacy configuration parsers.
Delete the old DinD manager and its cleanup code.
Delete Docker version probes and the direct Docker verification worker.
Delete Capsule root setup, UID switching, firewall enforcement, sudo setup, and Linux peer-PID authority checks.
Delete obsolete scripts, tests, dependencies, build rules, generated files, packaging entries, and documentation.

Do not keep dead legacy code behind feature flags.
Do not rename a Docker wrapper and call it an OpenShell adapter.
Keep an old pure helper only after its task records a contract review and independent tests.
Do not delete user data, uncommitted work, or unrelated runtime resources.

## Use subagents

Use the runner's real subagent tools.
Use up to eight concurrent subagents when tasks and write ownership permit this.
Run the initial source reviews in parallel.
Assign separate subagents to independent implementation tasks.
Assign a different subagent to each implementation review.
Use [SUBAGENTS.md](./SUBAGENTS.md) for task packets, write leases, and evidence reports.

Do not claim that this prompt changes the runner configuration.
Do not invent subagent calls or subagent reports.
Record unavailable delegation and continue serial work when necessary.

## Follow the plan

Start with P00.
Finish the required inputs before you start a dependent task.
Use the task's exact write scope.
Let the integration subagent change root manifests, lockfiles, shared schemas, and CI definitions.
Do not let concurrent subagents edit the same file.

Use the fixed contracts in this pack.
Do not improvise API fields, security claims, retry rules, resource ownership, or compatibility behavior.
Inspect the pinned source when a symbol is unknown.
Record the actual file, symbol, and source revision.

Persist each operation and mutation request ID before the external effect.
Use server-side identity and generation checks.
Do not claim that a client-side read creates an atomic mutation guard.
Keep unknown outcomes visible.
Do not repeat an effect with a new request ID after a lost response.

Block activation until all required receipts and authoritative observations match.
Do not treat helper output, `SandboxReady`, or agent status as a security proof.
Do not copy credentials into ordinary records, logs, checkpoints, images, or shared project storage.

## Handle a blocked contract

Stop the affected task when a required upstream contract is absent.
Write a blocker report with the source evidence and smallest failing test.
Name the required upstream change and affected task IDs.
Continue independent tasks.

Do not introduce a host execution fallback, Docker fallback, private runtime controller, or weaker isolation claim.
Do not mark a blocked task as complete.
Do not replace a real integration test with a mock and report the same result.
Do not call an unsupported feature a successful implementation.

## Test and review

Run every test assigned to each task.
Run the complete acceptance matrix before release approval.
Test real agents with approved disposable accounts.
Test two accounts, the default shell, reconnect, host actions, recovery, and cleanup in one combined journey.
Repeat the critical journeys from fresh resources through an independent test agent.

Test wrong scopes, stale generations, lost responses, same-name replacements, target outages, and interrupted transfers.
Test strict read-only metadata, engine cleanup, rebuilt binaries, dirty work, and image identity.
Keep failures and raw sanitized evidence.
Report unavailable infrastructure as `BLOCKED`, not `PASS`.

## Finish

Remove every legacy execution path from the final production graph and source tree.
Update fresh-install instructions and the qualified compatibility matrix.
Prepare the v2 release candidate without an old-state migration path.
Do not publish releases or change production infrastructure without explicit authorization.

Report completed tasks, open blockers, changed files, removed paths, executed tests, and evidence locations.
Report the actual OpenShell tuple and remaining unsupported capabilities.
Mark the goal complete only when every mandatory task and release gate passes.
