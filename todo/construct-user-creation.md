# Construct Image: User Creation Responsibility

**Status**: Deferred — needs design work

## Problem

The construct image (`projectjackin/construct:trixie`) creates the `claude` user at UID/GID 1000 during its build. The derived image then has to remap the user with `usermod`/`groupmod` to match the host user's UID/GID. This is a hack — the construct makes assumptions about the user that the derived layer immediately undoes.

## Why It Matters

- `groupmod`/`usermod` remapping is fragile and adds build time
- The construct's `ARG UID=1000` / `ARG GID=1000` are meaningless since they always get overwritten
- Files created during the construct build are owned by UID 1000, then `chown -R` has to fix them
- The pre-published construct adds a release step and version drift risk

## Options

1. **Move user creation to the derived layer**: The construct becomes a pure toolchain image (Debian + packages, no user). The derived Dockerfile creates the `claude` user with the correct host UID/GID from the start. No remapping needed. Tradeoff: agent authors can't `docker build .` their repo standalone without adding a user — but they could use a multi-stage approach or a test target.

2. **Build construct locally instead of pulling**: jackin' builds the construct from source as a build stage, passing UID/GID at build time. No published image needed. Tradeoff: first build is slower (installs all system packages), but Docker caches it locally. Breaks the agent repo standalone build contract.

3. **Keep construct as-is, just pull latest before build**: `docker pull projectjackin/construct:trixie` before each build to keep it fresh. Doesn't fix the user remapping problem but prevents stale base images. Least disruptive.

4. **Hybrid**: Construct stays published without a user. A thin `jackin-base` local image adds the user. Agent Dockerfiles still `FROM projectjackin/construct:trixie` for their build stages, but the final runtime stage uses the local base. Preserves the agent author contract while fixing the user problem.

## Related Files

- `docker/construct/Dockerfile`
- `src/derived_image.rs`
- `docs/src/content/docs/developing/construct-image.mdx`
