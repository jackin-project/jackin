# Plan 051: Decide whether rootless DinD can become the `standard` default

> **Executor instructions**: This is research plus a decision. Do not change
> profile defaults unless the evidence clearly supports it and a maintainer
> accepts the tradeoff.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: plan 049
- **Category**: direction (DIRECTION-03)
- **Planned at**: current PR branch, 2026-07-04

## Why this matters

Rootless DinD is already selectable and fails closed on hosts that cannot
support it. The remaining question is whether it is stable enough across
supported hosts and common Docker workflows to become the `standard` profile's
default DinD tier.

## Steps

1. Reuse the plan 049 matrix for Compose, BuildKit, and Testcontainers workloads.
2. Add host coverage notes for Docker Desktop macOS, Linux cgroup v2, and Linux
   cgroup v1 failure behavior.
3. Record gaps where rootless DinD cannot support a workflow that privileged
   DinD supports.
4. Decide one of:
   - keep `standard` default DinD as `none`;
   - make rootless DinD an explicit recommended grant;
   - make rootless DinD the `standard` default.
5. Update docs and roadmap with the decision.

## Done criteria

- [ ] Rootless DinD decision is documented with evidence.
- [ ] Docs say whether rootless DinD is default, recommended opt-in, or still
      experimental.
- [ ] Any follow-up implementation plan is written if the decision requires
      code changes.
- [ ] `plans/README.md` row updated.

## Verification

```sh
mise exec -- cargo fmt --check
mise exec -- cargo xtask docs repo-links
mise exec -- cargo xtask roadmap audit
cd docs && mise exec -- bun run build
```

## STOP conditions

- The supported-host evidence is incomplete. Keep rootless DinD opt-in and
  document what host data is missing.
