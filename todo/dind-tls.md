# DinD Running Without TLS Authentication

**Status**: Deferred — option 1 (TLS with auto-generated certs) is the correct long-term solution

## Problem

The Docker-in-Docker sidecar runs with `DOCKER_TLS_CERTDIR=` (empty, disabling TLS) and listens on `tcp://0.0.0.0:2375` without any authentication. Docker itself warns this will become a hard failure in a future release.

## Why It Matters

- Any container on the same Docker network (`jackin-*-net`) has unauthenticated root access to the DinD daemon
- While the per-agent network provides some isolation, this is still an unnecessary attack surface
- A future Docker release will break this entirely — DinD will refuse to start without TLS or an explicit `--tls=false` flag

## Options

1. **Enable TLS with auto-generated certificates**: Let DinD generate certs in a shared volume, mount the client certs into the agent container, and set `DOCKER_TLS_VERIFY=1` + `DOCKER_CERT_PATH`. This is the Docker-recommended approach. Adds complexity: need a shared volume for certs and a wait for cert generation before the agent starts.

2. **Use Docker socket mounting instead of TCP**: Mount the DinD socket via a shared volume (`/var/run/docker.sock`) instead of TCP. Eliminates the network exposure entirely. May require changes to the DinD container setup.

3. **Explicitly pass `--tls=false`**: Acknowledge the risk and suppress the warning/future breakage. Quick fix but doesn't address the security concern. Only appropriate if the per-agent network is considered sufficient isolation.

4. **Use Docker's built-in `--link` or socket proxy**: More complex but eliminates TCP exposure.

## Related Files

- `src/runtime.rs` — `launch_agent_runtime` DinD startup
- `docs/src/content/docs/guides/security-model.mdx`
- `docs/src/content/docs/reference/architecture.mdx`
