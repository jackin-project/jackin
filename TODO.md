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
