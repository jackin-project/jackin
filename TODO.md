# TODO

Individual items are tracked in [`todo/`](todo/). Each file is a self-contained design document with problem statement, options, and related files.

## Open Items

- [Construct Image: User Creation Responsibility](todo/construct-user-creation.md) — UID/GID remapping hack in derived images
- [DinD Running Without TLS Authentication](todo/dind-tls.md) — unauthenticated Docker daemon on agent network
- [Orphaned DinD Container on Agent Launch Failure](todo/orphaned-dind-cleanup.md) — sidecar left running when agent fails to start
- [1Password Integration for Agent Secrets](todo/onepassword-integration.md) — first-class secret injection at launch time
- [Agent Source Trust Model](todo/agent-source-trust.md) — trust-on-first-use for third-party agent repos
- [Migrate Docker CLI to Bollard API Client](todo/bollard-migration.md) — replace string-matched error handling with typed API
- [Rootless DinD Research](todo/rootless-dind.md) — reduce privileged container attack surface
- [Selectable Sandbox Backends: DinD and MicroVM](todo/selectable-sandbox-backends.md) — operator-selectable runtime modes with backend-neutral lifecycle design
- [Reproducibility and Provenance Pinning](todo/reproducibility-pinning.md) — commit SHA pinning for agent repos
- [Interactive Env Vars and Resolution](todo/env-var-interpolation.md) — interactive prompts, workspace overrides, and secret resolution for env vars

## Resolved

- [Sensitive Mount Path Warnings](todo/sensitive-mount-warnings.md) — warn before mounting `~/.ssh`, `~/.aws`, etc.
- [Custom Plugin Marketplace Support](todo/custom-plugin-marketplace.md) — auto-install custom Claude marketplaces and plugins from `jackin.agent.toml`
- [Expose DinD Hostname as `JACKIN_DIND_HOSTNAME`](todo/dind-hostname-env-var.md) — agents can reach DinD-backed services without parsing `DOCKER_HOST`
