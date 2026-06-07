# Coordination

Use this file to avoid collisions between agents on `feature/tui-architecture` / PR #495.

## Active Work

| Agent | Started | Area | Files / refs | Status |
| --- | --- | --- | --- | --- |
| Angela | 2026-06-07 | Merge `origin/main` into PR #495 and resolve conflicts; Defect 63 docs consistency + branch coordination | `COORDINATION.md`; `docs/content/docs/reference/roadmap/post-restructure-fixes-checklist.mdx`; conflict files discovered during the `origin/main` merge; CI run `27092817551` for head `40c44b9df` | In progress. Fixed one stale Round 3 checklist summary so Defect 63 consistently names `cargo-audit`, `cargo-deny`, and `cargo-hack`; local docs gate and docs Codebook passed. Next: fetch `origin/main`, merge it into `feature/tui-architecture`, resolve all conflicts, run required gates, then push normal fast-forward commits only unless this file records otherwise. |
| Laris | 2026-06-07 | PR #495 CI/CD verification and DCO/Codebook follow-up | `COORDINATION.md`; commit history checks; `.github/workflows/docs.yml`; `.codebook.toml`; `mise.toml`; roadmap CI tooling docs | Paused for coordination. Earlier narrow history rewrite/DCO verification found merge-base `522ee2077574a2a7a7c690fc632894a1197bbf8a` and local DCO `missing=0`; Angela currently owns the coordination/checklist lane. Laris will not force-push or modify Angela-owned docs files without a fresh coordination update. |

## Observed State

| Item | Status |
| --- | --- |
| `cargo-audit` lane | Preserve it. The operator explicitly said not to remove cargo-audit; the branch intentionally includes `.cargo/audit.toml`, CI `cargo audit`, `mise.toml` pinning, and matching docs. |
| Remote branch | Local was realigned to `origin/feature/tui-architecture` after an equivalent-history divergence where HEAD and origin trees matched. Do not force-push without checking this file and fetching first. |
| Main merge | Angela is taking the requested `origin/main` merge/conflict-resolution lane. Other agents should not edit conflict files while the merge is in progress unless they first update this file. |
| CI run `27092817551` | Prior-head run for `40c44b9df`; `audit` and `msrv` passed, `check` was still running at last poll. |
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

Laris local verification already completed before pausing:

- `git merge-base HEAD origin/main` was `522ee2077574a2a7a7c690fc632894a1197bbf8a`.
- Local scan over `origin/main..HEAD` reported `missing=0` for `Signed-off-by`.
- `.github/workflows/docs.yml` parsed successfully as YAML.

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
