# Plan 053: Run signed-release end-to-end verification

> **Executor instructions**: This is the independent verification item from the
> security-hardening cluster. Exercise the release artifacts end to end; do not
> replace this with unit-level signature checks only.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: plan 043
- **Category**: direction (DIRECTION-03)
- **Planned at**: current PR branch, 2026-07-04

## Why this matters

Signed-release plumbing only earns trust when the produced artifacts can be
verified the same way an operator or installer verifies them. The hardening
cluster called out that this path had not been exercised end to end.

## Steps

1. Identify the current release artifact set and the supported local rehearsal
   command from the release docs and CI workflows.
2. Run or script a local verification that checks artifact digest, signature,
   certificate identity/issuer, and transparency-log inclusion where applicable.
3. Ensure the verification path fails closed on a tampered artifact or mismatched
   identity.
4. Document the exact operator-facing verification command and CI/release gate.
5. Update release docs, roadmap/status docs, and `plans/README.md`.

## Done criteria

- [ ] Release artifact verification is exercised end to end.
- [ ] A tamper/mismatch case fails closed.
- [ ] Docs tell operators how to verify artifacts.
- [ ] `plans/README.md` row updated.

## Verification

Use the release-specific commands discovered in Step 1, plus:

```sh
mise exec -- cargo fmt --check
mise exec -- cargo xtask docs repo-links
mise exec -- cargo xtask roadmap audit
cd docs && mise exec -- bun run build
```

## STOP conditions

- Required signing credentials or release-only infrastructure are unavailable
  locally. Document the gap and add the smallest CI/release rehearsal hook that
  can prove the path before shipping.
