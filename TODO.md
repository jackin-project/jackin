# TODO

## Construct Image: User Creation Responsibility

**Problem**: The construct image (`donbeave/jackin-construct:trixie`) creates the `claude` user at UID/GID 1000 during its build. The derived image then has to remap the user with `usermod`/`groupmod` to match the host user's UID/GID. This is a hack — the construct makes assumptions about the user that the derived layer immediately undoes.

**Why it matters**:
- `groupmod`/`usermod` remapping is fragile and adds build time
- The construct's `ARG UID=1000` / `ARG GID=1000` are meaningless since they always get overwritten
- Files created during the construct build are owned by UID 1000, then `chown -R` has to fix them
- The pre-published construct adds a release step and version drift risk

**Options to consider**:

1. **Move user creation to the derived layer**: The construct becomes a pure toolchain image (Debian + packages, no user). The derived Dockerfile creates the `claude` user with the correct host UID/GID from the start. No remapping needed. Tradeoff: agent authors can't `docker build .` their repo standalone without adding a user — but they could use a multi-stage approach or a test target.

2. **Build construct locally instead of pulling**: jackin builds the construct from source as a build stage, passing UID/GID at build time. No published image needed. Tradeoff: first build is slower (installs all system packages), but Docker caches it locally. Breaks the agent repo standalone build contract.

3. **Keep construct as-is, just pull latest before build**: `docker pull donbeave/jackin-construct:trixie` before each build to keep it fresh. Doesn't fix the user remapping problem but prevents stale base images. Least disruptive.

4. **Hybrid**: Construct stays published without a user. A thin `jackin-base` local image adds the user. Agent Dockerfiles still `FROM donbeave/jackin-construct:trixie` for their build stages, but the final runtime stage uses the local base. Preserves the agent author contract while fixing the user problem.

**Decision**: Deferred. Needs design work to evaluate the agent author experience impact of each option.

## DinD Running Without TLS Authentication

**Problem**: The Docker-in-Docker sidecar runs with `DOCKER_TLS_CERTDIR=` (empty, disabling TLS) and listens on `tcp://0.0.0.0:2375` without any authentication. Docker itself warns this will become a hard failure in a future release:

```
level=warning msg="Binding to IP address without --tlsverify is insecure and gives root access on this machine to everyone who has access to your network."
level=warning msg="Support for listening on TCP without authentication or explicit intent to run without authentication will be removed in the next release"
```

**Why it matters**:
- Any container on the same Docker network (`jackin-*-net`) has unauthenticated root access to the DinD daemon
- While the per-agent network provides some isolation, this is still an unnecessary attack surface
- A future Docker release will break this entirely — DinD will refuse to start without TLS or an explicit `--tls=false` flag

**Current state**: The DinD container is started with `-e DOCKER_TLS_CERTDIR=` (in `launch_agent_runtime`) and the agent connects via `DOCKER_HOST=tcp://{dind}:2375`. No certificates are generated or mounted.

**Options to consider**:

1. **Enable TLS with auto-generated certificates**: Let DinD generate certs in a shared volume, mount the client certs into the agent container, and set `DOCKER_TLS_VERIFY=1` + `DOCKER_CERT_PATH`. This is the Docker-recommended approach. Adds complexity: need a shared volume for certs and a wait for cert generation before the agent starts.

2. **Use Docker socket mounting instead of TCP**: Mount the DinD socket via a shared volume (`/var/run/docker.sock`) instead of TCP. Eliminates the network exposure entirely. May require changes to the DinD container setup.

3. **Explicitly pass `--tls=false`**: Acknowledge the risk and suppress the warning/future breakage. Quick fix but doesn't address the security concern. Only appropriate if the per-agent network is considered sufficient isolation.

4. **Use Docker's built-in `--link` or socket proxy**: More complex but eliminates TCP exposure.

**Decision**: Deferred. Option 1 (TLS with auto-generated certs) is the correct long-term solution. Option 3 is the minimum viable fix to prevent breakage when Docker enforces TLS.

## Orphaned DinD Container on Agent Launch Failure

**Problem**: When an agent container fails to start (e.g. entrypoint error, Claude install failure, or the user exits immediately), the DinD sidecar container remains running indefinitely. The current cleanup logic only runs when the `docker run -it` command completes and the agent is no longer in the running container list, or on explicit error. But if the agent container never starts successfully (exits during entrypoint before attaching), the DinD container is left behind.

**Observed behavior**:
```
$ docker ps
CONTAINER ID   IMAGE         NAMES
a9c92f511e94   docker:dind   jackin-the-architect-dind    ← orphaned, no agent container
```

The agent container (`jackin-the-architect`) is gone, but its DinD sidecar (`jackin-the-architect-dind`) and network (`jackin-the-architect-net`) remain.

**Why it happens**:
- The pre-launch cleanup (added to fix "network already exists") only runs at the start of `launch_agent_runtime`
- If the agent container starts but exits before the user gets an interactive session, `docker run -it` returns `Ok(())` but the container is already stopped
- The post-run check sees the container is not running and triggers cleanup — but this path may not always execute cleanly (e.g. if `list_running_agent_names` fails)
- The `LoadCleanup` guard is not a `Drop` impl — it requires explicit `.run()` calls

**Options to consider**:

1. **Make `LoadCleanup` implement `Drop`**: Automatically clean up on any exit path. Add an `armed` flag (already exists) and a stored runner reference. Challenge: `Drop` can't take `&mut impl CommandRunner` easily due to lifetime constraints.

2. **Add a background health-check thread**: After launching DinD, spawn a thread that monitors the agent container. If the agent exits, the thread cleans up DinD and the network. This handles all edge cases including crashes and signal kills.

3. **Use `docker run --rm` for the DinD container**: Make DinD auto-remove when stopped. Then just `docker stop` the DinD container during cleanup. Simpler but doesn't handle the case where DinD is running but the agent never started.

4. **Add a `jackin gc` command**: A garbage collection command that finds orphaned DinD containers and networks (DinD containers whose corresponding agent container no longer exists) and removes them. Could also run automatically at the start of `load_agent`.

5. **Automatic pre-launch garbage collection**: Before each `load`, scan for orphaned jackin-managed resources (DinD containers without a matching agent container) and clean them up. This is the most robust — handles all failure modes including hard kills and terminal closures.

**Decision**: Deferred. Option 5 (automatic GC) is probably the best UX. Option 4 (explicit `jackin gc`) is simpler to implement as a first step.

## 1Password Integration for Agent Secrets

**Problem**: Jackin does not yet have a first-class way to integrate with 1Password for secrets an agent may need at runtime, such as API tokens, cloud credentials, or project-specific environment values. Today the operator has to manage these manually through mounts, shell setup, or ad-hoc environment injection.

**Why it matters**:
- 1Password is a common source of truth for developer and team secrets
- Manual secret handling is error-prone and easy to make inconsistent across agents and workspaces
- A first-class integration could reduce the need to mount broad host directories just to make credentials available
- The operator should be able to decide which secrets enter which agent environment, matching jackin's boundary model

**Options to consider**:

1. **1Password CLI passthrough**: Install or expose the `op` CLI in agent classes and let operators authenticate inside the container. Simple and flexible, but pushes too much setup onto each agent session and weakens the "predefined environment" story.

2. **Workspace-managed secret references**: Let workspaces or global config declare references to 1Password items or fields, then resolve them at launch time into files, mounts, or environment variables. This fits jackin's operator-controlled boundary model, but needs careful UX and secret-handling rules.

3. **Ephemeral runtime injection**: Resolve 1Password secrets only at launch and inject them into the running container without persisting them into `~/.jackin/data`. Better from a persistence standpoint, but requires very clear rules around visibility, logging, and restart behavior.

4. **Read-only secret mount generation**: Materialize selected 1Password secrets into temporary files and mount them read-only into the container. This matches existing mount semantics well, but needs secure lifecycle cleanup and a good way to map secrets to destinations.

**Decision**: Deferred. The right direction is probably operator-controlled launch-time resolution with non-persistent secret injection, but it needs design work around UX, persistence, and how it fits workspace and global mount configuration.

## Agent Source Trust Model

**Problem**: `resolve_agent_source()` auto-constructs a GitHub URL from namespace/name and clones directly without any trust verification. This exposes a typosquatting and untrusted repo execution risk.

**Why it matters**:
- Any namespace/name pair is accepted and cloned without confirmation
- No mechanism to distinguish a trusted, previously-used agent from a novel one
- Agents execute in a build context with access to the Dockerfile and build instructions

**Desired behavior**: A trust-on-first-use model similar to `mise trust`:
- First time an agent source is encountered, clone the repo but prompt the user for confirmation before running it
- Store trusted sources in config (allowlist)
- Subsequent runs of trusted agents proceed without prompts
- Optional security mode: always show agent source output before running, allowing AI agent analysis of the content

**Decision**: Deferred. Needs design work on the trust store format, UX for confirmation prompts, and how it interacts with workspace config.

## Migrate Docker CLI to Bollard API Client

**Problem**: All Docker operations use `ShellRunner` which shells out to the `docker` CLI. Error handling relies on string-matching stderr text (e.g., `"No such container"`, `"No such network"` in `is_missing_cleanup_error()`), which is brittle across Docker versions and locales.

**Why it matters**:
- String-matched error detection can break silently on Docker updates or non-English locales
- No structured error codes from CLI — only exit code 1 for most failures
- Blocking process calls without native timeout support
- The `bollard` crate provides a typed Rust Docker API client over Unix socket/TCP with proper HTTP status codes (e.g., 404 for "not found" vs 500 for real errors)

**Options to consider**:

1. **Full migration to `bollard`**: Replace all `ShellRunner` Docker calls with `bollard` API calls. Gives structured responses, native async with timeouts, and proper error codes. Significant refactor.

2. **Incremental migration**: Start with cleanup/lifecycle operations (where string matching is most problematic), keep CLI for `docker build` and `docker run -it` (where interactive TTY is needed).

**Decision**: Deferred. Incremental migration (option 2) is the pragmatic path. Start with container/network lifecycle operations.

## Rootless DinD Research

**Problem**: The current design uses a privileged `docker:dind` sidecar container, which grants broad host-level capabilities to the DinD daemon.

**Why it matters**:
- Privileged mode gives the container nearly full host access
- A compromised DinD daemon could escape container isolation
- Docker provides `docker:dind-rootless` as an alternative with reduced privileges

**Research needed**:
- Evaluate `docker:dind-rootless` compatibility with jackin's build and runtime operations
- Identify limitations (e.g., certain storage drivers, network modes, build features)
- Test whether agent Dockerfiles build correctly under rootless DinD
- Assess performance impact
- Consider alternative isolation approaches (sysbox, Kata containers)
- Evaluate optional stricter network policy modes

**Decision**: Deferred. Needs hands-on testing to determine feasibility and compatibility.

## Sensitive Mount Path Warnings

**Problem**: Mount validation is structurally good, but there are no guardrails for sensitive host paths like `~/.ssh`, `~/.aws`, `~/.gnupg`, etc. An operator can accidentally expose credentials without any warning.

**Why it matters**:
- Mounting `~/.ssh` or `~/.aws` gives the agent container access to credentials that could be exfiltrated
- Mistakes in mount config are easy to make and hard to notice
- The isolation model is weakened if sensitive paths are mounted without awareness

**Desired behavior**: Warning-and-confirm model:
- When a mount path matches a known sensitive pattern, display a clear warning explaining the risk
- Require explicit user confirmation before proceeding
- Allow the mount if confirmed — operators who know what they're doing should not be blocked
- Sensitive path patterns: `~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.config/gcloud`, `~/.kube`, `~/.docker`, etc.

**Decision**: Deferred. Implement alongside the agent source trust model (#2 above) as part of a broader trust-and-confirm UX pattern.

## Reproducibility and Provenance Pinning

**Problem**: The current agent repo flow tracks moving branches (typically `main`) by default. There is no mechanism to pin to a specific commit, verify provenance, or control when updates are pulled.

**Why it matters**:
- An agent's behavior can change between runs without the operator's knowledge
- No way to reproduce a previous run's exact environment
- No audit trail of which commit was used for a given session

**Desired behavior**:
- Support lockfile-like pinning to commit SHAs in agent config
- Display the resolved commit SHA during agent launch
- Introduce explicit `--update` flag to pull latest (rather than auto-updating)
- Record the commit SHA used in runtime state for audit/debugging
- Integrate with the trust model: trust is granted at a specific commit, `--update` re-evaluates trust

**Decision**: Deferred. Implement after the agent source trust model is in place, as pinning and trust are closely related.
