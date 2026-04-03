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
