# Plan 052: Cover network egress behavior for DinD inner containers

> **Executor instructions**: This is the parallel security-hardening item from
> plan 043. Focus on observable behavior and named residual risk; do not
> overstate Docker firewall coverage for inner containers.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: plan 043
- **Category**: direction (DIRECTION-03)
- **Planned at**: current PR branch, 2026-07-04
- **Completed**: 2026-07-04 — DinD-inner egress is covered as a
  repeatable residual-risk assertion: allowlist networking plus DinD reports
  `partial (DinD inner containers bypass host iptables)` in the debug session
  contract and role-container enforcement label.

## Why this matters

The outer role container can enforce an allowlist firewall, but DinD introduces
inner containers with a separate network path. The launch summary already
reports partial enforcement when DinD is active; this plan turns that residual
risk into tested, documented behavior.

## Steps

1. Add an e2e or integration scenario that launches with allowlist networking
   and DinD active.
2. Verify and document which traffic is blocked by the outer firewall and which
   inner-container paths remain outside that enforcement.
3. Ensure the launch summary and `--debug` output clearly report partial
   enforcement when DinD is active.
4. If a proxy-sidecar or inner-Docker firewall plan is required, write a scoped
   follow-up plan instead of smuggling it into this verification pass.
5. Update the network-egress roadmap item and Docker profile docs.

## Done criteria

- [x] DinD-inner egress behavior is covered by a repeatable scenario.
- [x] Partial enforcement is asserted in launch/debug output.
- [x] Docs and roadmap state the residual risk plainly.
- [x] `plans/README.md` row updated.

## Verification

```sh
mise exec -- cargo fmt --check
mise exec -- cargo xtask docs repo-links
mise exec -- cargo xtask roadmap audit
cd docs && mise exec -- bun run build
```

## STOP conditions

- The test requires host networking privileges unavailable in the supported
  local/CI environments. Document the manual reproduction and CI gap.
