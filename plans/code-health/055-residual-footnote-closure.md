# Plan 055: Close named residual footnotes (014/023/028/033/038/049)

> Wave 8 residual program: acceptance criterion 3 requires named residuals
> fixed in tree **or** executed as new numbered plans — not README notes only.

## Status

- **Priority**: P1
- **Effort**: M
- **Depends on**: plans 014, 023, 028, 033, 038, 049 DONE spines
- **Category**: tech-debt
- **Planned at**: 2026-07-11 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Steps

1. **028 host turso** — publicize `jackin_usage::store_backend`; route
   `crates/jackin/src/cli/usage/store.rs` through `connect_local`; drop host
   `turso` dependency.
2. **049 repo-link generator** — add bun + `gen-crate-pages.ts` to
   `docs.yml` `repo-link-check` before `docs repo-links`.
3. **014 / 023 / 033 / 038** — record measured DEFER rows in
   `RESIDUAL_LEDGER.md` (R-014-*, R-023-*, R-033-suite-a, R-038-env-console-tail).
4. Update plan 028/049/014/023/033/038 status lines in README to point at this
   plan + ledger (no open “residual: …” as sole disposition).
5. Mark this plan DONE.

## Done criteria

- [x] Host crate has zero direct `use turso::` imports
- [x] `repo-link-check` generates crate pages before link check
- [x] Every former named residual is CLOSED or DEFER in `RESIDUAL_LEDGER.md`
- [x] README status rows reference 055/ledger instead of open footnotes only
