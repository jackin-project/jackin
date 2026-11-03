# Commits, Branching & Contributing

## License

Apache 2.0. Contributions licensed under same terms (Section 5).

## DCO

All contributions signed off under [DCO v1.1](https://developercertificate.org/). Enforced by [DCO2 GitHub App](https://github.com/cncf/dco2) — unsigned commit blocks PR.

Employer contributions: confirm authorization before submitting. Use personal email in commit author + sign-off.

## How to Submit

1. Fork. Branch feature off `main`.
2. Run `mise install` from the repo root to install the pinned toolchain and dev tools.
3. Run `mise exec -- hk install --mise` once per checkout to install the Git hooks (see "Git hooks" below).
4. Change. Sign every commit: `git commit -s`.
5. Open PR describing problem solved. CI must pass.
6. Optional blame hygiene: `git config blame.ignoreRevsFile .git-blame-ignore-revs` so `git blame` skips mass layout/fmt sweeps listed in that file.

## Git hooks

Hooks run through [hk](https://hk.jdx.dev). `hk.pkl` at the repo root defines
the steps; `mise.toml` and `mise.lock` pin hk and every tool the steps start.
Install the hooks once per checkout. The command is idempotent, so it is safe
to re-run:

```sh
mise exec -- hk install --mise
```

`--mise` runs each hook through `mise x`, so the pinned tools are on `PATH`
without shell activation. Requirements: Git 2.54 or newer (hk 2.11.0 registers
config-based hooks, `hook.<name>.command`; older Git falls back to
`.git/hooks` shims) and `mise` on the `PATH` that Git uses for hooks. If hk is
already installed in your global Git config, the per-checkout install is
skipped; pass `--force-local` to install it anyway. Linux and macOS use the
same command.

`pre-commit` only checks (`fix = false`, no stash, no auto-staging). It runs
these steps over the staged files:

- `mise` and `gitleaks` (hk builtins);
- `cargo_fmt`, `cargo_clippy` (workspace, all targets, all features, `-D warnings`);
- `xtask_repo_gates` (`xtask ci --only comments,layers,pins,containers,errors`);
- `rumdl` (Markdown, except `rewrite/` and `README.md`);
- `jactionlint` and `zizmor` (workflows and composite actions);
- `kingfisher-staged` (Kingfisher on the staged change, with `.kingfisher-baseline.yml`).

`pre-push` runs `betterleaks git . --redact` over all refs and `mise run ci:fast`
(`xtask ci --fast`, the repository gate without the docs build).

The full-history `security-audit` hook never fires from Git. Run it with
`mise exec -- hk run security-audit`: Kingfisher, betterleaks and gitleaks over
all refs.

CI runs the same three scanners as the `secrets-scan` check. Its local
equivalent is `mise run check:secrets-scan`: gitleaks, betterleaks and
Kingfisher over the tracked tree and over the history reachable from `HEAD`.
A finding that a scanner reports is either removed from the source or reviewed
and recorded in that scanner's own file. `.gitleaks.toml` holds the path and
pattern allowlists that gitleaks and betterleaks both read. `.gitleaksignore`
holds fingerprints for gitleaks. `.betterleaksignore` holds fingerprints for
betterleaks, which ignores `.gitleaksignore` while `.betterleaksignore`
exists. `.kingfisher-baseline.yml` holds the Kingfisher baseline; refresh it
with `--manage-baseline` in a clean clone that has no linked worktrees, and
review the diff.

If a hook step fails, apply the fixes explicitly and commit again:

```sh
git commit -m "..."       # pre-commit fails
mise exec -- hk fix       # apply fixes in your checkout
git diff                  # review all unstaged changes
git add -p && git commit -m "..."
```

`mise exec -- hk check --all` runs every check step over all tracked files
outside a commit. Bypass a hook for an emergency with
`HK_SKIP_HOOK=pre-commit git commit`.

## Branching

Never commit to `main`. All work on own branch.

Names: `feature/`, `fix/`, `refactor/`, or `chore/` prefix + short lowercase hyphen description.

### Sync with main: merge by default

```bash
git fetch origin
git merge --no-ff origin/main -m "chore(merge): sync main into <branch>"
git push
```

When updating an active PR branch from `main`, use a normal merge commit by default. This preserves the branch's review history and avoids a force-push cycle.

Do not rebase, amend, squash, or otherwise rewrite the branch unless the operator explicitly approves that rewrite for the branch. If the merge has conflicts, resolve them in the merge commit and keep the subject conventional.

Recommended merge-sync subject:

```text
chore(merge): sync main into <branch>
```

## Commit Format

[Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/). Subject: `<type>[optional scope][!]: <desc>`, where scope is written as `(scope)`.

| Type | Use |
|---|---|
| `feat` | New user-visible feature |
| `fix` | Bug fix |
| `docs` | Docs-only |
| `style` | Formatting, no logic |
| `refactor` | Restructure, no behavior change |
| `perf` | Performance |
| `test` | Tests |
| `build` | Build/tooling/deps |
| `ci` | CI config |
| `chore` | Maintenance (release, merge-sync commits, deps) |
| `revert` | Reverts prior commit |

Breaking: `feat!:` or `feat(scope)!:` + `BREAKING CHANGE:` footer. PR title = squash-merge subject — same rules.

## Signing

```sh
git commit -s -m "feat(scope): description"
```

For a published commit without a sign-off, the original contributor can append an individual DCO remediation commit. Use the original commit's full 40-character SHA:

```sh
GIT_AUTHOR_NAME="ORIGINAL AUTHOR" \
GIT_AUTHOR_EMAIL="author@email" \
GIT_COMMITTER_NAME="ORIGINAL AUTHOR" \
GIT_COMMITTER_EMAIL="author@email" \
git commit -s --allow-empty \
  -m "DCO remediation for ORIGINAL AUTHOR <author@email>" \
  -m "I, ORIGINAL AUTHOR <author@email>, hereby add my Signed-off-by to this commit: <full 40-character commit SHA>"
```

This remediation is only for your own contribution. Use the exact original author name and email from the target commit in the declaration, remediation commit author and committer, and `Signed-off-by` trailer. Set all four identity variables explicitly so inherited Git identity settings cannot change them. The certification commit may be empty, without unrelated file changes. This adds an append-only certification and preserves the published PR history. Third-party remediation is disabled, and DCO sign-off remains required for organization members.

## Merge-Readiness Check

Run when PR ready to merge (not before every commit):

```sh
mise exec -- mbx +1.99.1 xtask ci
# or
mise exec -- mbx +1.99.1 xtask ci
```

For a faster local pass that skips feature-powerset and Docker-backed smoke tests:

```sh
mise exec -- mbx +1.99.1 xtask ci --fast
```

`mise exec -- mbx +1.99.1 xtask ci --e2e` includes the Docker-backed lane. It first checks that Docker is running, builds and exports the local capsule binary, then runs `mise exec -- mbx +1.99.1 nextest run -p jackin --features e2e --profile docker-e2e --no-tests warn`. The `crates/apps/jackin/tests/` corpus was removed, so that nextest step currently selects no tests. In PR checkouts, `jackin-dev pr sync <PR_NUMBER>` still prepares the isolated env and capsule export for manual smoke tests; source `$(jackin-dev pr path <PR_NUMBER>)/env.sh` before manual `jackin` commands.

Local builds outside CI default to the package version for `JACKIN_VERSION` / `JACKIN_CAPSULE_VERSION` so each commit does not invalidate every build-meta consumer and capsule cache entry. GitHub Actions sets `CI`, so release, preview, construct, and CI builds still stamp the real `<version>+<sha>`. Set `JACKIN_VERSION_OVERRIDE=<value>` only when you need an explicit local version.

Fmt fail → `mise exec -- mbx +1.99.1 fmt`, then re-check with `mise exec -- mbx +1.99.1 fmt --check`.
