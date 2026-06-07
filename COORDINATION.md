# Coordination

Use this file to avoid collisions between agents on `feature/tui-architecture` / PR #495.

## Active Work

| Agent | Started | Area | Files / refs | Status |
| --- | --- | --- | --- | --- |
| Angela | 2026-06-07 | Merge `origin/main` into PR #495 and resolve conflicts; Defect 63 docs consistency + branch coordination | `COORDINATION.md`; `docs/content/docs/reference/roadmap/post-restructure-fixes-checklist.mdx`; conflict files discovered during the `origin/main` merge; CI run `27092817551` for head `40c44b9df` | In progress. Fixed one stale Round 3 checklist summary so Defect 63 consistently names `cargo-audit`, `cargo-deny`, and `cargo-hack`; local docs gate and docs Codebook passed. `origin/main` merge conflicts have been resolved to the PR-side split-crate/ported-roadmap structure. Local merge validation passed: `cargo fmt --check`, `cargo check --workspace --all-targets --all-features --locked`, `cargo audit`, `cargo deny check licenses bans sources`, and focused Codebook. Next: commit and push the merge, then dispatch current-head CI. |
| Laris | 2026-06-07 | PR #495 CI/CD verification and DCO/Codebook follow-up | `COORDINATION.md`; commit history checks; `.github/workflows/docs.yml`; `.codebook.toml`; `mise.toml`; roadmap CI tooling docs | Paused for coordination while Angela validates the `origin/main` merge resolution. Current checkout verification before conflicts: Codebook docs check passed (219 files, 0 errors), Codebook source check passed (754 files, 0 errors), `.github/workflows/docs.yml` parses, GitHub PR checks report DCO failing, and local DCO scan reports `missing=13`. Laris will not rewrite history, force-push, or modify Angela-owned files without a fresh coordination update. |

## Observed State

| Item | Status |
| --- | --- |
| `cargo-audit` lane | Preserve it. The operator explicitly said not to remove cargo-audit; the branch intentionally includes `.cargo/audit.toml`, CI `cargo audit`, `mise.toml` pinning, and matching docs. |
| Remote branch | Local was realigned to `origin/feature/tui-architecture` after an equivalent-history divergence where HEAD and origin trees matched. Do not force-push without checking this file and fetching first. |
| Main merge | Angela is taking the requested `origin/main` merge/conflict-resolution lane. Other agents should not edit conflict files while the merge is in progress unless they first update this file. |
| Current merge state | `origin/main` merge conflicts are resolved in the index; local validation passed; Angela is committing and pushing the merge result. |
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
- Earlier rewritten checkout scan over `origin/main..HEAD` reported `missing=0` for `Signed-off-by`, but the current checkout has since changed and now reports `missing=13`.
- `.github/workflows/docs.yml` parsed successfully as YAML.
- Codebook docs spell check passed locally: 219 files, 0 spelling errors.
- Codebook source spell check passed locally: 754 files, 0 spelling errors.
- `gh pr checks 495 --repo jackin-project/jackin` currently reports DCO failing.

Current DCO-missing commits on this checkout:

- `b1955653667dd0ae97b5a94e4d3a9f666bc024ac` `docs(roadmap): add CI matrix split under Codebase health (#516)`
- `6c4c89569134ff7570da821b56c04602dc2d9d20` `docs(roadmap): add agent launch flags API under Agent runtimes & authentication (#515)`
- `1d1604ddc90827b593826efed7cf8e71ab06a510` `docs(roadmap): add security threat model & signed releases under Isolation & security (#512)`
- `e7290c709d5de6331b915d13189f8b3911b59f9d` `docs(roadmap): add test infrastructure & behavioral specs under Codebase health (#513)`
- `91f48cf2c8445ee3621cd1efa290160dfb15ef53` `docs(roadmap): add platform support policy & roadmap freshness under Infrastructure (#514)`
- `1a5f3109e57c8f5f61e7b61c55d43611d637891b` `docs(roadmap): add Operator CLI hygiene program under Operator surface (#511)`
- `f07db32eae4697fcf0b59019de5da189acff31ca` `docs(roadmap): add Rust CI tooling and dependency-hygiene item (#510)`
- `49395a7ade66c82e88d2c75464563a4a7999e9f0` `docs(roadmap): add terminal emulation crate (jackin-term) verification plan (#509)`
- `1f80915d492de5e28027dd48a9ee6b4a042c3ef6` `docs(roadmap): add structured tracing & metrics item under Codebase health (#508)`
- `21d037bc074bbcb541abeb248ba3d8877acd2159` `docs(roadmap): add Agent runtime trait item under Codebase health (#507)`
- `91d02578f26b8d8ddb4e1e15690ac019be629aac` `chore(deps): update actions/checkout action to v6.0.3 (#520)`
- `54338be499a56fcefdeeb82548fbcb659711ea5f` `chore(deps): lock file maintenance (#504)`
- `6d5d497c92169044291c2ad2e1702961a6140465` `fix(deps): update rust crate tabled to 0.21.0 (#506)`

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
