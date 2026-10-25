# Fixed implementation contracts

## C00. Status and source precedence

The architecture in [SPEC.md](./SPEC.md) is mandatory.
The implementation choices below make that architecture executable.
These choices are new plan decisions unless a specification reference states otherwise.
They are not claims about existing OpenShell APIs.

P02 maps each requirement to actual upstream source.
P03 freezes the internal schemas before parallel implementation.
Change a frozen contract only through a recorded planner review.
Do not change a security requirement through an implementation convenience decision.

## C01. Source and compatibility pins

Use these starting references:

| Item | Reference |
| --- | --- |
| Jackin source baseline | `faa2ab6c12e3de45a76ab6b98df0c64dac7aac6e` |
| OpenShell candidate source | `c4688aa8715ea75d6c39fe468361dbbc16c691fa` |
| OpenShell release comparator | `v0.1.5`, `bdb956f2cd65a635f1294fccf1d138d263bf650f` |
| Historical experiment source | `a7689b48e2b6ef2ca5a4367ee27527887cf45ce8` |

The candidate source contains SDK mTLS support. It is not a qualified release merely because the code exists.
Use it as the initial integration candidate, not as an automatic production approval.

Create `compatibility.lock.toml` at the repository root.
Record the OpenShell source commit, patch hashes, SDK commit, protocol digest, and all runtime image digests.
Record the gateway driver, host kernel, architecture, storage, and network configuration.
Record the official chart and Agent Sandbox versions for Kubernetes.
Record the role ABI and Capsule protocol versions.

Use one coherent tuple for each qualified deployment.
Do not combine release and development components without an explicit compatibility test and approval.
Use dedicated Kubernetes supervisors initially.
Block Kubernetes qualification when the selected tuple cannot supply that topology.

Upstream changes create a new candidate tuple.
Re-run affected qualification tests for that tuple.
Do not use `main`, `latest`, or a moving development tag as a durable compatibility pin.

## C02. Production crate graph

Use these fourteen production crates.
The paths below are target paths for the rewrite.
They are not a claim that these crates already exist.

| Path | Responsibility |
| --- | --- |
| `crates/core/jackin-domain` | Typed records, IDs, errors, receipts, and application ports. |
| [crates/core/jackin-config](../../crates/core/jackin-config) | Strict v2 configuration and validation. |
| `crates/core/jackin-session-protocol` | Bounded Capsule frames and protocol types. |
| `crates/services/jackin-control` | Durable operations, admission, host effects, and application coordination. |
| `crates/services/jackin-roles` | Role resolution, artifacts, and cache decisions. |
| `crates/services/jackin-workspaces` | Materialization, checkpoints, export, and handoff data. |
| `crates/services/jackin-accounts` | Account selection, grants, leases, and usage interpretation. |
| `crates/services/jackin-sessions` | Domain sessions, attachments, and input ownership. |
| `crates/adapters/jackin-openshell` | The only sandbox execution adapter. |
| `crates/adapters/jackin-buildx` | Explicit image builders and registry delivery. |
| `crates/adapters/jackin-state` | SQLite records and transactions. |
| `crates/adapters/jackin-agents` | Agent commands, native-login formats, usage parsers, and resume formats. |
| [crates/apps/jackin](../../crates/apps/jackin) | CLI, Console, and per-user control service. |
| [crates/apps/jackin-capsule](../../crates/apps/jackin-capsule) | Unprivileged session service and relay. |

Use modules inside these crates before adding more production crates.
Keep test support and `jackin-xtask` outside the production count.
Do not retain old crates merely to satisfy an old dependency edge.

The domain crate must not import OpenShell protobufs, Docker types, or SQLite types.
Service crates depend on domain ports, not concrete adapters.
Application composition connects ports to adapters.
Capsule must not depend on host accounts, host control, builder, or gateway authentication code.

Use one OpenShell adapter. Do not define a public Docker/Apple/OpenShell backend enum.
Native driver names can remain capability data inside target qualification records.
Mock ports are test tools, not alternative production runtimes.

## C03. Configuration and identifiers

Use schema version `2`.
Use `<config-root>/jackin/v2/config.toml` for user settings.
Use `jackin.role.toml` for role recipes.
Use `.jackin/project.toml` for project settings.
Resolve platform roots through one host-path adapter.

Reject unknown fields, duplicate identifiers, and unknown schema versions.
Do not search for old configuration files as an implicit fallback.
Do not import an old database or old resource manifest.
A later v2 database migration is distinct from a v1 importer.

Keep gateway authentication in the upstream registration system.
Store only credential references in Jackin configuration.
Use native OpenShell policy documents. Do not add a second policy language.

Use typed UUID identifiers for projects, targets, accounts, grants, instances, domains, sessions, attachments, and operations.
Keep upstream sandbox IDs in a distinct type.
Keep local target IDs separate from gateway aliases and verified server identities.
Do not invent a globally immutable upstream gateway UUID.

Generate each canonical sandbox name once.
Use `jk-` followed by sixteen lowercase base32 characters from secure randomness.
Validate that format against the pinned gateway in P02.
Persist the name before creation. Reject a collision.
Keep name tombstones in the v2 state store.
Never reuse a name intentionally. Do not rely on this rule instead of server-side identity checks.

Resolve human selectors at the UI boundary.
Return an ambiguity error when more than one resource matches.
Do not choose the first match.

## C04. Domain and authority records

Implement every entity in the specification's product table.
Use these required relationships:

```text
Instance -> one or more ExecutionDomain records
ExecutionDomain -> ResolvedRole + Materialization + AuthorityScopeBinding + CredentialAuthority
CredentialAuthority -> AccountBound(AccountGrant) | CredentialFree
ExecutionDomain -> TargetProfile + SandboxBinding + admitted capability revision
SandboxBinding -> explicit gateway reference + OpenShell workspace + name + immutable ID + observed generation
Session -> one ExecutionDomain + one Capsule boot ID
Attachment -> one Session + replay cursor + geometry + input lease generation
Operation -> resolved immutable inputs + request IDs + phase + receipts + recovery disposition
```

A changed account, credential exposure, policy, tenant, or trust class requires a new domain.
Do not treat different environment variables, homes, or panes as security isolation.
Use separate writable materializations by default.
Require an explicit collaboration grant for shared writable source.

Account-bound domains carry a complete account grant. Credential-free domains carry no
account ID, grant generation, provider credential, or provider binding. Do not represent
either variant by omitting only selected account fields.

The default shell uses a separate credential-free domain with no coding-agent account.
An explicit in-domain shell inherits that domain's authority.
Show that authority in trusted host labels.

Map each domain to an authorized OpenShell workspace.
A Jackin project can use several OpenShell workspaces.
Workspace membership determines who can exercise provider and sandbox authority.
A private account requires a suitably restricted scope.
Local account leases do not constrain other computers or administrators.

## C05. Durable operation protocol

Use SQLite with one application write coordinator.
Enable transactional recovery and foreign-key checks.
Store secrets outside ordinary database records.
Do not hold a write transaction during a network call.

Keep application state, runtime observations, and Capsule observations separate.
Use these application phases:

```text
Resolve -> Build -> Deliver -> Create -> Observe -> Materialize -> Admit -> Activate -> Attach -> Complete
```

Resolve includes initial account, scope, and capability authorization.
Admit rechecks the final activation inputs.
Do not create a domain or stage credentials before initial authorization.

Use these operation outcomes:

```text
Pending | Running | Waiting | Complete | Failed | OutcomeUnknown | Cancelled
```

`Cancelled` describes local intent. It does not prove that a remote effect stopped.
An operation can contain a confirmed partial result and an uncertain later effect.
Record each effect separately.

Before each effect, commit its canonical inputs, request ID, intended binding, and recovery rule.
After the effect, commit its receipt or explicit unknown outcome.
Use a transaction for each local publication boundary.
Do not generate a new mutation request ID merely because the response was lost.

| Effect | Retry rule |
| --- | --- |
| Read | Retry within the configured budget. Preserve scope and error classification. |
| Sandbox creation | Use the original request ID and request digest when upstream replay supports this. |
| Stop, start, delete, or policy change | Require server-side expected identity and applicable generation checks. |
| Effectful exec or file publication | Use the qualified replay contract. Keep an uncertain effect unresolved. |
| New relay attachment | Use a new attachment ID and a new exec request ID. |
| Capsule StartSession | Reuse its operation ID only within the same Capsule boot. |

Authentication failure, masked permission failure, and incomplete inventory are not resource absence.
A gateway timeout must not create a replacement sandbox.
OpenShell owns runtime restart and infrastructure reconciliation.

## C06. Activation barrier

Store an activation receipt before StartSession.
The receipt contains these matching values:

| Value | Required source |
| --- | --- |
| Target and authority scope | Trusted host resolution and authorized upstream connection. |
| Sandbox ID and runtime generation | Authoritative OpenShell observation. |
| Executed role artifact and platform | Qualified gateway or operator observation. |
| Policy source and loaded revision | Authoritative policy observation. |
| Provider bindings and accepted revision | Account-bound domain: authoritative observation for each admitted binding. Credential-free domain: absent. |
| Account ID and grant generation | Account-bound domain: both values from trusted Jackin account admission. Credential-free domain: absent. |
| Materialization epoch and digest | Verified project publication receipt. |
| Capsule boot and protocol | Helper handshake, used only for application compatibility. |

The receipt records the domain's credential-authority variant. An account-bound receipt has a
complete matching grant and both account values. A credential-free receipt has no account
values, grant, provider credential, or provider binding. Missing one account value or mixing
the variants blocks activation. All other C06 observations remain required and matching.

Record the role manifest separately from an OpenShell-prepared image or VM root filesystem.
Require a qualified derivation receipt when the runtime transforms the role artifact.
Do not compare different artifact identity types as if they were identical.

Every value must refer to the same intended domain.
Block activation when a required authoritative observation is unavailable.
Do not replace it with helper output or a submitted configuration document.

Re-admit after a relevant account, policy, provider, image, or runtime change.
A helper identity echo is not attestation.
A host preflight is not enforcement against a direct OpenShell caller.
Mandatory ceilings belong upstream.

## C07. Role and build contract

Build the complete declared selectable agent set into each role image.
Validate every advertised executable.
Reject unknown or missing versions before a reproducible artifact receives a cache identity.

Include these inputs in the build identity:

```text
role revision; effective build context; Dockerfile; base-image digest; platform;
agent/tool/plugin versions; hooks; nonsecret build arguments; Capsule artifact;
role schema; Capsule protocol; declared secret-output cache policy
```

Apply the actual Docker ignore rules before hashing the effective context.
Keep secret bytes out of hashes exposed as ordinary metadata, logs, and image layers.
Use supported BuildKit secret inputs where required.
Require explicit trust before a role sends source or secrets to a builder.

Bind each build to an explicit Docker context and builder endpoint.
Reject an unexpected ambient `DOCKER_HOST` override.
Distinguish local engine image IDs, OCI platform manifests, and OCI indexes.
Use same-engine delivery only after endpoint identity matches.
Use immutable registry references for remote delivery.

Perform executable probes through OpenShell or verified artifact metadata.
Do not use `docker run` for a version probe.
A tag or Capsule echo does not prove the executed image identity.

## C08. Role ABI

Use ordinary Linux OCI images.
Install Capsule at `/opt/jackin/bin/jackin-capsule`.
Install application tools as root-owned executable assets during the build.
Use a named `sandbox` account with a nonzero UID and GID.
Support the actual identity assigned by the qualified target.

Use these paths:

```text
/sandbox/project       project materialization
/sandbox/.jackin        domain-private application and agent state
/sandbox/cache         separately retained, bounded caches
/sandbox/.jackin/run    Capsule socket and nonsecret run records
```

Set HOME, XDG paths, and tool paths explicitly.
Do not rely on identical OCI USER, WORKDIR, or ENTRYPOINT behavior across drivers.
Use an explicit unprivileged bootstrap command.
Do not use chown, sudo, setuid, namespace setup, or firewall setup at workload startup.
Block an incompatible UID, filesystem, syscall, or architecture requirement.

## C09. Capsule transport and process ownership

Launch `jackin-capsule serve` as the canonical workload.
Use upstream canonical restart policy `Never` for the initial interactive profile.
Start with zero sessions.
Require a new activation epoch after a Capsule boot change.
Do not replay prior agent-start commands automatically.

For each attachment, execute `/opt/jackin/bin/jackin-capsule relay --stdio` through OpenShell.
Set outer exec `tty=false` and `no_login_shell=true`.
Carry protocol frames on stdout/stdin. Write diagnostics to stderr.
Capsule creates the inner PTYs.
Relay EOF and cancellation close only the attachment.

Capsule owns PTYs, process groups, signals, child reaping, replay, and session inventory.
Capsule does not own credentials, host effects, gateway identity, or security enforcement.
A compromised same-UID agent can tamper with Capsule.
Treat all Capsule events as untrusted application data.

## C10. Session protocol defaults

These are internal v2 defaults. They are not OpenShell protocol fields.

Use a four-byte big-endian payload length followed by a CBOR payload.
Use protocol major `2` and minor `0` initially.
Reject incompatible major versions before an effect.
Negotiate optional capabilities explicitly.

| Limit | Default |
| --- | --- |
| Encoded frame | 1 MiB |
| Control payload | 64 KiB |
| Output chunk | 32 KiB |
| Replay storage | 8 MiB per session |
| Slow-viewer queue | 256 KiB per attachment |
| Sessions | 8 per domain |
| Viewers | 16 per session |
| Terminal geometry | 1–512 columns and 1–256 rows |
| Input lease | 30 seconds; renew before 10 seconds remain |
| Decoder nesting | 16 levels |

Use bounded arithmetic and bounded allocations.
Reject excess lengths before allocation.
Chunk snapshots and output inside these limits.
Keep PTY draining independent of slow viewers.
Emit an explicit replay-gap event after retention loss.

Each boot has a fresh boot ID.
Each StartSession has a host-assigned session ID, operation ID, and request digest.
Return the existing result for an identical duplicate within the same boot.
Reject a reused operation ID with a different digest.

Use monotonic output sequence numbers.
Bind each snapshot to a sequence watermark.
Allow many viewers and one input writer.
Increment the input generation whenever ownership changes.
Reject stale-generation input.
Do not replay unacknowledged keystrokes after reconnect.
An input acknowledgment means accepted input, not completed shell work.

Keep durable terminal transcripts off by default.

## C11. Project transfer and retention

Model source isolation, transfer, storage, retention, and collaboration as separate axes.
Keep Shared, Worktree, and Clone semantics explicit.
Do not turn a live source mode into a snapshot silently.

Use a unique staging epoch for each materialization.
Verify the manifest and content before atomic activation.
Do not expose a partial staging directory as the active project.

A source manifest records the Git base, selected dirty files, selected untracked files, deletions, exclusions, and digests.
Use a Git bundle or equivalent verified object transfer for required unpushed commits.
Do not export a linked worktree's host `.git` pointer as a portable repository.
Detect nested repositories, submodules, and large-file pointers.
Preserve supported cases explicitly. Reject unsupported cases before data transfer.

Exclude credential stores from project transfer.
Reject path traversal, escaping links, special device files, and archive-size violations.
Do not trust archive ownership or target paths.

Return a baseline-bound patch, branch, or artifact through an explicit host action.
Check for host changes before apply.
Preserve both versions on conflict.
Do not use transparent bidirectional synchronization as the v2 foundation.

Use trusted-live-local only with operator-approved binds on the actual runtime host.
Show the disabled external-resource-label admission tradeoff.
Do not use raw host binds for isolated VM profiles.
Do not broaden a mount to fit a driver limit.

Use Kubernetes automatic `/sandbox` storage initially.
Do not add a child mount that silently disables that storage injection.
Record storage owners and deletion behavior for every target.

A checkpoint must survive deletion of the source sandbox and its owned storage.
Record an external retention owner, digest, completeness, and restore test.
Exclude known credentials and unrequested host files.
A secret scan supplements the allowlist. It does not prove that arbitrary project data contains no secrets.

Require a supported quiesce procedure for a consistent handoff checkpoint.
Do not claim application consistency from a checksum alone.
Do not claim that stop/start preserves process memory.
Keep the source until the destination accepts its verified checkpoint.

## C12. Accounts and host authority

Use a stable Account ID and explicit credential-source revision.
Do not use an email address as account identity.
Keep the saved account on reconnect.
A changed revision requires re-admission.

Prefer native OpenShell providers for compatible endpoint-bound substitution and refresh.
Do not assume substitution works through opaque protocols.
A provider placeholder can authorize real actions even without secret disclosure.

Treat native-login files as deliberate credential exposure inside one private domain.
Stage only approved files with atomic publication and revision checks.
Use one refresh writer within the declared deployment scope.
Reject unsupported cross-device refresh sharing.
Do not build a generic refresh broker to hide that limitation.

Keep usage interpretation on the trusted host.
Key usage caches by Account ID and credential revision.
Show unavailable and stale data separately from zero usage.
Deny delayed requests from an ended run.

Host clipboard, URL opening, file export, and approval require a matching local operator action.
Do not execute host actions from terminal escape sequences or unsolicited helper events.
Use fixed worker images and exact argv for approved tool actions.
Bind approvals to domain, operation, image, arguments, input digests, destination, expiry, and replay identity.
A material change requires new approval.
Use a separate OpenShell domain for a secret-consuming tool action.
Do not send the secret to the coding-agent PTY.

## C13. Policy and target contracts

Role authors request capabilities. Authorized operators grant them.
Record requested policy and observed effective policy separately.
Record provider rules that add effective access.
A global policy can replace a sandbox policy.
Do not implement an imaginary intersection of every policy layer.

Treat filesystem or identity changes as recreation requirements when the runtime contract requires this.
Handle connection closure after a network-policy update.
Do not weaken a policy to restore a dropped stream automatically.

Use these target profiles:

```text
dev-container
trusted-live-local
isolated-vm-small
isolated-vm-large
team-kubernetes
verification
```

Use fixed VM resource shapes until upstream enforces per-sandbox sizing.
Reject an unsupported capability before sandbox creation.
Do not downgrade a requested VM boundary to a container.

Use official OpenShell and Agent Sandbox resources for Kubernetes.
Jackin must not create Pods, CRDs, controllers, or privileged cluster service accounts.
Qualify actual ingress and egress enforcement, not policy-object existence.
Keep provider-private data out of shared PVCs.

Use loopback-bound authenticated forwards for private developer services.
Treat public exposure as a separate grant with separate application authentication.
Never expose Capsule control as a public service.

## C14. Managed Docker capability

This capability is off by default.
When off, create no companion, endpoint, credentials, or engine prewarm.

Use one disposable engine VM for each admitted execution domain.
OpenShell owns admission, creation, restart, stop, deletion, and orphan reconciliation.
Jackin requests the capability and displays its receipts.
Jackin does not manage engine VMs or sidecars itself.

Grant access only to the owning domain and current runtime generation.
Use private engine storage and a confined projection of approved project data.
Exclude account stores and OpenShell control data from the projection.
Do not expose the physical host engine or a rootful daemon beside the trusted runtime.

Require API streaming, Compose, build, exec, logs, events, and real Testcontainers.
Require dynamic ports, callbacks, service-host discovery, and online pulls.
Enforce outer egress and aggregate child resource budgets.
Bind cleanup to immutable parent identity and authorized scope.
Stop revokes endpoints. Start issues new access. Recreation creates a new engine.
Delete remains pending until destruction and revocation are confirmed.

Initial support uses a qualified VM gateway companion extension.
Other profiles reject this capability until independently qualified.
Do not call an external engine fixture a managed companion.

## C15. Independent verification

Use a separate OpenShell sandbox without a coding-agent account.
Use immutable candidate input and a separate trusted check bundle.
Require read-only root and approved bounded writable control/scratch paths.
Test content and metadata immutability.
Do not substitute Landlock content denial for a read-only filesystem guarantee.

Freeze the worker image, candidate digest, bundle digest, exact argv, resource budget, and deadline.
Disallow ordinary network access.
Observe completion through the qualified upstream contract.
Keep verdict authority on the trusted host.
Distinguish code failure, timeout, infrastructure failure, cancellation, and unknown outcome.
No host or direct-Docker worker fallback is permitted.

## C16. Observation, resources, and completion

Use bounded fleet concurrency and explicit pagination.
Keep stale rows with last-seen times during outages.
Show scope coverage and target completeness.
Do not assume cross-workspace list permission.

Report requested, enforced, reserved, and measured resources separately.
Include supervisor and companion overhead in logical-instance accounting.
Do not merge account domains to reduce memory use.

Test status is one of `NOT_RUN`, `PASS`, `FAIL`, or `BLOCKED`.
A mock test does not qualify a runtime target.
A skipped environment cannot produce a passing release gate.

Full completion requires every task, mandatory target journey, and upstream gate.
An intermediate demo remains an intermediate demo.
