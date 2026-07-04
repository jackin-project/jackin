# Plan 050: Flip the compiled Docker default to `standard`

> **Executor instructions**: Do this only after plan 049 reports a green
> compatibility matrix or a maintainer explicitly accepts the documented
> breakage. This plan changes the default; it does not broaden `standard`.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: HIGH
- **Depends on**: plan 049
- **Category**: direction (DIRECTION-03)
- **Planned at**: current PR branch, 2026-07-04
- **Completed**: 2026-07-04 — compiled default moved to `standard`;
  `compat` remains available through explicit profile selection.

## Why this matters

`compat` preserves legacy behavior, but it grants full sudo and privileged DinD
by default. Once the workload evidence is green, jackin❯ should default to the
profile that matches the hardening contract: `standard`.

## Steps

1. Change `DockerSecurityProfile::default()` from `Compat` to `Standard`.
2. Keep `profile_base_grants(Standard)` unchanged: sudo off, DinD none,
   resource limits on, writable root, open network.
3. Confirm `no-new-privileges` is still enabled when sudo is off.
4. Update user docs, configuration docs, schema/reference wording, and release
   notes/changelog material to call out the behavior change and the opt-back to
   `compat`.
5. Update any tests that assert the compiled default.
6. Update the Docker hardening roadmap and `plans/README.md`.

## Done criteria

- [x] Default profile is `standard`.
- [x] `compat` remains available through CLI/config/workspace/role settings.
- [x] Docs and release notes explain the breaking behavior and opt-back.
- [x] Compatibility matrix from plan 049 is referenced.
- [x] `plans/README.md` row updated.

## Verification

```sh
mise exec -- cargo fmt --check
mise exec -- cargo test -p jackin-core -p jackin-runtime docker_profile --locked
mise exec -- cargo xtask docs repo-links
mise exec -- cargo xtask roadmap audit
cd docs && mise exec -- bun run build
```

## STOP conditions

- Plan 049 is not green and the maintainer has not accepted the documented
  incompatibility.
- A fix requires changing `standard` to silently grant sudo or privileged DinD.
