# Coordination

Use this file to avoid collisions between agents on `feature/tui-architecture` / PR #495.

## Active Work

| Agent | Started | Area | Files / refs | Status |
| --- | --- | --- | --- | --- |
| Angela | 2026-06-07 | PR #495 merge/conflict lane plus Round 3 checklist close-out audit | `COORDINATION.md`; `docs/content/docs/reference/roadmap/post-restructure-fixes.mdx`; `docs/content/docs/reference/roadmap/post-restructure-fixes-checklist.mdx`; roadmap status pages; merge commit `7fbece87c`; coordination commit `762e089a`; CI runs `27093316615`, `27093316603`, `27093316611`, `27093316602` | In progress. `origin/main` conflicts are resolved and pushed; GitHub no longer reports `DIRTY`. Local merge validation passed before the push: `cargo fmt --check`, `cargo check --workspace --all-targets --all-features --locked`, `cargo audit`, `cargo deny check licenses bans sources`, focused Codebook, and a conflict-marker scan. Current checklist audit shows the remaining open items are the Defect 54 live smoke/run-id ledger, B.5 auth source-folder smoke, live jackin-term performance run ids, final roadmap close-out after those proofs, the intentional license-ruling item, and Laris's DCO back-history lane. PR-attached current-head CI: Docs/Construct/Renovate are green; CI run `27093316615` has audit + MSRV green and `check` still running. |
| Laris | 2026-06-07 | PR #495 DCO back-history repair and Codebook verification | `COORDINATION.md`; commit history checks; `.github/workflows/docs.yml`; `.codebook.toml`; `mise.toml`; roadmap CI tooling docs | Complete and pushed. Laris rewrote the PR branch history to add exact author `Signed-off-by` trailers where GitHub DCO required them, preserved existing Claude/Codex co-author trailers, added Codex only where no agent trailer existed, verified local DCO scans (`missing=0`, `author_signoff_missing=0`), verified Codebook docs/source locally, and force-pushed with lease to head `45bdceab`. GitHub DCO now passes; remaining checks for that head are normal CI jobs. |
| Laris | 2026-06-07 | Split oversized CI `check` job and fix current CI failures | `COORDINATION.md`; `.github/workflows/ci.yml`; `docs/lychee.toml` | Implemented locally; pending GitHub CI after push. Laris replaced the monolithic Rust `check` job with parallel `fmt`, global `check-all-features`, global `clippy`, `dependency-policy`, package-matrix `check-default`, single full-suite `test`, `fuzz`, matrixed `bench-build`, plus existing `audit`, `msrv`, and `build-validator` behind the stable `ci-required` aggregate gate. Every Cargo job now restores Cargo registry/git cache; compile-heavy jobs use `mold` and GitHub-backed `sccache`, with `sccache --show-stats` emitted for verification. Previous pushed run `27095850519` was green for Rust/construct/DCO with four test partitions; current local edits restore a single test job and global check/clippy per operator request. Docs run `27095850514` failed `docs-link-check` on a `biggo.com` timeout, so Laris excluded that flaky external URL in lychee config rather than editing Angela-owned roadmap content. Local validation: workflow YAML parses, `git diff --check` passes, and lychee accepts the updated config. |

## Observed State

| Item | Status |
| --- | --- |
| `cargo-audit` lane | Preserve it. The operator explicitly said not to remove cargo-audit; the branch intentionally includes `.cargo/audit.toml`, CI `cargo audit`, `mise.toml` pinning, and matching docs. |
| `check` lane split | Preserve the same command coverage as the monolithic `check` job; only change scheduling so independent gates run in parallel and report through `ci-required`. This advances `docs/content/docs/reference/roadmap/ci-matrix-split.mdx`; Angela currently owns broad roadmap close-out files, so avoid roadmap edits without re-reading this file and coordinating first. |
| Remote branch | Local was realigned to `origin/feature/tui-architecture` after an equivalent-history divergence where HEAD and origin trees matched. Do not force-push without checking this file and fetching first. |
| Main merge | Angela completed the requested `origin/main` merge/conflict-resolution lane. Merge commit `7fbece87c` is pushed to `origin/feature/tui-architecture`; later coordination-only head `762e089a` kept the branch fast-forward. |
| Current merge state | GitHub reports `mergeStateStatus=BLOCKED`, not `DIRTY`, so conflicts are cleared. Remaining blockers are PR readiness/checks, the known DCO lane, and checklist smoke/ruling items. |
| Current-head PR CI | For head `762e089a`: Renovate Validate run `27093316602` passed; Docs run `27093316603` passed; Construct Image run `27093316611` passed; CI run `27093316615` has `audit` and `msrv` passed with `check` still running at last poll. |
| Current re-check | For head `45bdceab`, GitHub DCO passes. CI, Docs, Construct Image, and Renovate Validate are running or pending for the rewritten head. Codebook docs/source were verified locally after the rewrite and are expected to pass in GitHub. |
| Manual dispatch CI | Manual runs for the same head were dispatched as CI `27093323266`, Docs `27093323350`, Construct Image `27093323338`, and Renovate Validate `27093323366`; Docs/Construct/Renovate passed and CI remained in progress at last poll. |
| CI run `27092817551` | Prior-head run for `40c44b9df`; superseded by current-head runs above. |
| `.git-rewrite/` | Untracked scratch directory. Leave uncommitted unless an agent explicitly documents why it belongs in the PR. |

## Laris Handoff For Angela

Laris was fixing PR #495 CI/CD from the DCO / spell-check side. The intended change set is:

- Keep the Codebook spell checker, not Spellbook, as the CI tool.
- Keep two separate whole-branch spell checks in `.github/workflows/docs.yml`:
  - `spell-check-docs` for docs/prose files.
  - `spell-check-source` for Rust, TOML, shell, fish, and zsh source/config files.
- Keep Codebook configured through `.codebook.toml` and installed through `mise.toml` as `cargo:codebook-lsp = "0.3.41"`.
- Keep the roadmap CI tooling docs aligned with those jobs.
- Keep DCO trailers fixed across the PR branch. Existing `Co-authored-by: Claude ...` trailers must remain Claude; existing Codex trailers must remain Codex; commits with no visible agent trailer are treated as Codex-owned and should carry `Co-authored-by: Codex <codex@openai.com>`.

Laris verification completed:

- `git merge-base HEAD origin/main` is `6a891acea82b7de5490b2fd9863b4137131f6ed1`.
- Local DCO scans over `origin/main..HEAD` report `missing=0` and `author_signoff_missing=0`.
- `.github/workflows/docs.yml` parsed successfully as YAML.
- Codebook docs spell check passed locally: 219 files, 0 spelling errors.
- Codebook source spell check passed locally: 754 files, 0 spelling errors.
- `gh pr checks 495 --repo jackin-project/jackin` reports DCO passing for head `45bdceab`.

Current DCO-missing commits on this checkout: none.

Angela should react as follows:

- If Angela needs to push first, push a normal fast-forward only after fetching and confirming the active rows here are still accurate.
- If Angela edits `COORDINATION.md`, amend/include Laris's row rather than deleting it unless the DCO/Codebook work has been fully merged or intentionally superseded.
- If Angela touches `.github/workflows/docs.yml`, `.codebook.toml`, `mise.toml`, or the roadmap CI tooling docs, preserve the two separate Codebook jobs and update this handoff with the reason for any change.
- If Angela sees branch divergence or needs a force-push, stop and write the exact proposed push target and reason in this file before pushing. Laris is not currently authorized to force-push while Angela owns the coordination/checklist lane.
- Do not delete `.cargo/audit.toml`, the cargo-audit CI lane, or the cargo-audit docs references; the operator explicitly said to preserve that lane.

## Rules

- Check this file before editing, committing, rewriting history, or pushing.
- Add or update a row before starting work, and update the status before stopping.
- Do not overwrite files listed under another active row without coordinating first.
- Push normal fast-forwards only unless the Active Work table explicitly records that a coordinated force-push is being performed.
