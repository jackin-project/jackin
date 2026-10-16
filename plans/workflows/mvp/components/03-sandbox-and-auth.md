# C3: sandbox and authentication

## Purpose

Run each worker with the exact access that the host permits.
Keep real agent authentication usable without exposing broad host credentials.

## Existing code and changes

Reuse `jackin-runtime`, the Docker profile helpers, and existing credential services.
Reuse private repository materialization from `jackin-isolation`.
The current Docker grant code can increase resource limits and privileges.
Add a final restrictive workflow grant after existing role resolution.
Do not change the meaning of manual launch profiles to obtain workflow limits.

## API

| Operation | Input | Output |
|---|---|---|
| `admit_worker` | Plan request, host maximum grant, runtime capability facts | `WorkflowGrant` or rejection. |
| `launch_worker` | Grant, immutable image and binary selection, mounts, operation ID | Owned instance identity. |
| `inspect_worker` | Exact instance identity | Running state, actual limits, owned mounts, and session facts. |
| `freeze_worker` | Exact owned instance and capture operation ID | Confirmed paused state. |
| `unfreeze_worker` | Exact owned instance and capture operation ID | Confirmed resumed state. |
| `stop_worker` | Exact owned instance and preservation decision | Stop result without source deletion. |
| `resolve_auth` | Account ID, agent, auth mode, permitted purpose | Existing scoped credential envelope. |

`WorkflowGrant` is host-owned. The worker cannot modify it.
It contains mount permissions, endpoint rules, resource limits, execution deadline, and delegation policy.
It contains no unrestricted host execution authority.

## Grant calculation

For each capability, compare the request with the host limit and backend support.
Reject a required capability when either check fails.
Use the requested resource limit when it is within the host maximum.
Do not raise a low request to a more permissive role default.

Build the final runtime settings from this grant.
Do not merge lower-trust role settings after this point.
Inspect the resulting instance before the first agent prompt.
Stop the launch if the actual settings differ from the grant.

## Isolation rules

Use one independent mutable clone for implementation.
Use a separate instance for testing and another separate instance for review.
Test and review do not run at the same time in the MVP.
Never reuse the test worker as the reviewer without complete state removal.
Creating a fresh reviewer is the simpler default.

Mount only the worker's repository, private scratch space, and approved read-only inputs.
Do not mount shared writable Git metadata or object alternates.
Do not mount the host home, host Docker socket, or another worker's state.
Restrict control sockets to their actual supported operations and authenticated peers.

Disable DinD, privileged mode, sudo, and host namespaces.
Keep the existing trusted Capsule supervisor boundary.
The supervisor can perform its narrow control operations. The agent cannot inherit that authority.

## Resource and network limits

Enforce CPU, memory, and process limits with the existing Docker mechanisms.
The process limit includes child processes and threads as defined by the backend.
Add a worker deadline to the trusted supervisor path so host daemon loss cannot remove the deadline.
Stop the agent at expiry. Keep its source and required evidence.
A paused container must check an expired deadline immediately after resume.

Use the tested endpoint profile for each agent.
Pin the allowed destinations and protocols in the plan's effective profile.
Deny direct routes that bypass the selected enforcement mechanism.
Check IPv6 when enabled. Disable it when the profile cannot restrict it safely.
Do not add open networking as an authentication fallback.

Record whether enforcement uses IPs and ports, hostnames, or another specific mechanism.
Reject unsupported stronger guarantees.
Use disk admission checks, a disk-use monitor, and bounded artifact sizes in the default MVP profile.
Do not describe these checks as a hard quota.
Use an existing hard quota only when the selected backend provides a tested one.

## Authentication

Use existing account selection and credential staging.
Select the account before programmatic launch.
Permit only the chosen agent's authentication material in its private state.
Do not copy an unrestricted host credential directory or keychain into the worker.
Do not place Git publication credentials in an implementation or review worker.

Test a real authenticated task with the selected mode.
An installed binary and a version response do not prove authentication.
Record refresh support through the existing credential service.
Do not introduce a new token broker.
When supported refresh fails, report `authentication_required` and preserve work.
Do not restart the conversation to hide the failure.

## Vendor subagents

`within_worker` permits local delegation under the same limits and credential boundary.
It does not promise an exact number of logical subagents.
`forbid` requires tested adapter enforcement. Otherwise reject the request.
Disable exposed remote-agent tools. Reject a mode that needs remote workers.
Keep unobserved vendor activity marked unknown in the history.

## Build instructions

1. Add the final grant DTO and admission function.
2. Pass the grant through the existing headless launch path.
3. Add exact ownership labels and inspected settings.
4. Add pause and resume through the existing Docker boundary.
5. Reuse credential staging and refresh.
6. Add deadline and preservation behavior to the trusted supervisor path.

## Local verification

Run the authentication part of K1 and the sandbox part of K2.
Use one real profile. Do not build a full OS and provider matrix.
A denied write, endpoint, or privilege must stay denied after role settings change.

## Dependencies and sources

C1 provides the request. C2 uses the admitted session. C4 supplies the private source.
See [SOURCES.md](../SOURCES.md), J7 and E3.
