# Plan 049: Run the `standard` compatibility matrix

> **Executor instructions**: This is the evidence gate for making `standard`
> the default Docker profile. Do not flip the default in this plan. Produce the
> matrix, failures, fixes, and release-gate recommendation in this PR lineage.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: plan 043
- **Category**: direction (DIRECTION-03)
- **Planned at**: current PR branch, 2026-07-04
- **Completed**: 2026-07-04 — `cargo xtask profile-matrix standard`
  added and passed on the local cgroup-v2 Docker host; intentional rejects are
  named in the matrix and Docker profile docs.

## Why this matters

The default profile cannot move from `compat` to `standard` on code review
alone. `standard` turns sudo off, disables privileged DinD by default, applies
resource limits, and enables `no-new-privileges` while sudo is off. The next
step is empirical compatibility evidence across the workflows users actually
expect to run.

## Scope

Run and document a Tier-2 matrix for the built-in roles and representative role
workloads under:

- `--docker-profile standard`
- sudo off by default
- `no-new-privileges` on
- DinD disabled unless explicitly granted
- cgroup v2 and cgroup v1 host behavior documented separately

Minimum scenarios:

- Build/test a Rust workspace without runtime package installation.
- Build/test a Node or docs workspace without runtime package installation.
- Git and GitHub CLI operations using the supported credential path.
- Runtime package-install attempt without sudo: assert named failure and docs
  guidance to opt into `compat` or a sudo grant.
- Docker Compose / Testcontainers without DinD: assert named failure.
- Docker Compose / Testcontainers with `dind = "rootless"` on cgroup v2.
- The same rootless DinD scenario on cgroup v1: assert fail-closed message.
- Built-in roles `the-architect` and `agent-smith` can launch far enough to
  reach their agent command under `standard`.

## Steps

1. Create or extend the existing test/matrix harness in the smallest supported
   location. Prefer `xtask` or existing e2e infrastructure over shell-only docs.
2. Record each scenario with command, expected outcome, and host prerequisites.
3. Fix product bugs discovered by the matrix when the fix is small and directly
   required for `standard`.
4. For each legitimate incompatibility, add a named error or docs guidance
   instead of broadening `standard`.
5. Update [Docker Security Profiles](/guides/docker-profiles/) and the Docker
   hardening roadmap with the matrix result.

## Done criteria

- [x] Matrix scenarios are automated or explicitly documented when automation is
      blocked by host prerequisites.
- [x] All green scenarios pass under `standard`.
- [x] Intentional rejects emit named, documented failures.
- [x] The roadmap says whether the default flip is ready or blocked.
- [x] `plans/README.md` row updated.

## Verification

Use the final commands produced by the matrix, plus:

```sh
mise exec -- cargo fmt --check
mise exec -- cargo xtask docs repo-links
mise exec -- cargo xtask roadmap audit
cd docs && mise exec -- bun run build
```

## STOP conditions

- A core built-in role cannot launch under `standard` for a reason that would
  require granting full sudo or privileged DinD by default. Document the blocker
  and keep the default flip blocked.
