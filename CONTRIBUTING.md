# Commits, Branching & Contributing

## License

Apache 2.0. Contributions licensed under same terms (Section 5).

## DCO

All contributions signed off under [DCO v1.1](https://developercertificate.org/). Enforced by [DCO2 GitHub App](https://github.com/cncf/dco2) — unsigned commit blocks PR.

Employer contributions: confirm authorization before submitting. Use personal email in commit author + sign-off.

## How to Submit

1. Fork. Branch feature off `main`.
2. Run `mise install` from the repo root to install the pinned toolchain and dev tools.
3. Run `scripts/hooks-install` once per checkout to install the pre-commit hooks (see "Git hooks" below).
4. Change. Sign every commit: `git commit -s`.
4. Open PR describing problem solved. CI must pass.
5. Optional blame hygiene: `git config blame.ignoreRevsFile .git-blame-ignore-revs` so `git blame` skips mass layout/fmt sweeps listed in that file.

## Git hooks

Pre-commit checks run via [hk](https://hk.jdx.dev) (`hk.pkl` at the repo
root; the version is pinned in `mise.toml`/`mise.lock`). Install per checkout —
idempotent, safe to re-run:

```sh
scripts/hooks-install   # installs scripts/pre-commit-snapshot
```

No shell activation needed. Requirements: Git 2.54+ (config-based hooks;
Apple Git 2.54 meets the floor exactly) and `mise` on Git's runtime PATH —
true in terminals with mise shims, otherwise the hook fails closed with
`mise: command not found` (use a terminal or add the shims directory to
the GUI client's PATH). Each hook invocation builds in its own temporary Cargo target directory.
Compiler artifacts cannot collide with builds in the live checkout.
Python 3 (pinned in mise) provides process group isolation and symlink checks. A global hk pre-commit
hook is rejected because it would bypass the repository-owned snapshot
boundary.

What runs on every commit (see `hk.pkl` for the exact commands — each
mirrors the CI definition it cites):

- `fmt`: `mise exec -- mbx +1.99.0 fmt --check` (workspace; fix: `mise exec -- mbx +1.99.0 fmt`)
- `clippy`: `mise exec -- mbx +1.99.0 xtask clippy-affected` — Clippy with the exact CI flags
  over the affected closure (changed crates + reverse dependents +
  detached fuzz/arrayref packages + cross-crate file inputs; widens to
  the workspace when unprovable)
- `actionlint`: workflows (`*.yml` only, matching CI)
- `swiftlint`, `swift-format`: `native/**` on macOS; skipped elsewhere

Review mode, not auto-stage: the launcher validates the exact staged tree in
an isolated temporary repository. Original tracked, untracked, ignored files,
file metadata, the index, stash stack, and compiler outputs stay untouched.
The isolated repository preserves the original HEAD and merge index, so hk
and affected-package selection see the original staged diff. Unmerged index
entries or staged symlinks escaping the snapshot fail before tools run.

If a fixer changes source, the commit fails. The launcher saves a private
patch under the common Git directory's `jackin-hook-evidence/pre-commit.*/`
with directory mode 0700 and patch mode 0600. It prints the patch path, never
its contents. Review and apply the fixes explicitly:

```sh
git commit -m "..."       # isolated fix fails for review
mise x -- hk fix          # apply fixes in your checkout
git diff                 # review all unstaged changes
git add -p && git commit -m "..."
```

`hk check` runs checks on modified files outside a commit. Partial commits
remain safe: the hook never clears, restores, or cleans the live worktree.
Success, failure, and catchable signals remove only the invocation's temporary
repository and build outputs after terminating its tool process group. Review
patches remain until the author removes them. Bypass with `HK=0 git commit`
(emergencies only).

Two intentional divergences from CI: hook Clippy is closure-scoped while
CI lints the workspace (same flags; full coverage stays in CI /
`mise exec -- mbx +1.99.0 xtask ci --only lint`), and `hk check --all` scopes Clippy by `git status`
(skips green on a clean tree). The global install
(`hk install --global --mise`) is GUI-robust but forces `--staged`,
which disables stashing — hence the repo-owned bootstrap above. Do not
install hk globally for this repository: a global hk hook is rejected by
`scripts/hooks-install` because it does not use the repository-owned
snapshot boundary.

Linux developers: same setup (`mise install`, `scripts/hooks-install`;
hook commands are POSIX `sh` and the Swift steps skip themselves where
`swiftlint`/`xcrun` are absent). CI-on-Linux (the velnor Rust lane on
`ubuntu-26.04`) proves the shared pieces there: per-package `fmt`,
`clippy -- -D warnings`, and the `jackin-xtask` nextest suite including
the affected-closure tests. Hook firing/isolation behavior is verified on
macOS; it rides identical hk + mise artifacts on Linux (both pinned in
`mise.lock`).

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
git commit --amend -s --no-edit   # forgot -s → force-push after (operator approval required)
```

DCO fail on PR: fix first, before anything else.

## Merge-Readiness Check

Run when PR ready to merge (not before every commit):

```sh
mise exec -- mbx +1.99.0 xtask ci
# or
mise exec -- mbx +1.99.0 xtask ci
```

For a faster local pass that skips feature-powerset and Docker-backed smoke tests:

```sh
mise exec -- mbx +1.99.0 xtask ci --fast
```

`mise exec -- mbx +1.99.0 xtask ci --e2e` includes the Docker-backed lane. It first checks that Docker is running, builds and exports the local capsule binary, then runs `mise exec -- mbx +1.99.0 nextest run -p jackin --features e2e --profile docker-e2e`. In PR checkouts, `jackin-dev pr sync <PR_NUMBER>` still prepares the isolated env and capsule export for manual smoke tests; source `$(jackin-dev pr path <PR_NUMBER>)/env.sh` before manual `jackin` commands.

Local builds outside CI default to the package version for `JACKIN_VERSION` / `JACKIN_CAPSULE_VERSION` so each commit does not invalidate every build-meta consumer and capsule cache entry. GitHub Actions sets `CI`, so release, preview, construct, and CI builds still stamp the real `<version>+<sha>`. Set `JACKIN_VERSION_OVERRIDE=<value>` only when you need an explicit local version.

Fmt fail → `mise exec -- mbx +1.99.0 fmt`, then re-check with `mise exec -- mbx +1.99.0 fmt --check`. See [TESTING.md](TESTING.md).
