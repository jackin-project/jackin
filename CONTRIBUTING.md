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
5. Open PR describing problem solved. CI must pass.
6. Optional blame hygiene: `git config blame.ignoreRevsFile .git-blame-ignore-revs` so `git blame` skips mass layout/fmt sweeps listed in that file.

## Git hooks

Pre-commit checks run via [hk](https://hk.jdx.dev) (`hk.pkl` at the repo
root; the version is pinned in `mise.toml`/`mise.lock`). Install per checkout —
idempotent, safe to re-run:

```sh
scripts/hooks-install   # guarded build-and-install of the native hook owner and hk pre-push hook
```

No shell activation needed. Requirements: Git 2.56+ (config-based hooks with
named-hook disabling), `mise` on Git's runtime PATH, and the executable
`rust-build-guard` in that PATH or `$HOME/.local/bin`. The installer runs the
guarded locked Cargo command once, then publishes the resulting native owner at
a stable path inside this worktree's Git directory. Re-running installation
atomically replaces changed owner bytes and leaves identical bytes in place.
Each hook invocation qualifies the staged toolchain and builds in its own
private snapshot and temporary Cargo target directory. The native owner
handles process cleanup, source custody, and symlink checks. Compiler
artifacts cannot collide with builds in the live checkout.

The hook uses the developer’s shared Cargo cache and credentials and admitted
Rustup and Mise storage roots. Gate tools are qualified against the staged
configuration, and auxiliary commands use the controlled gate PATH. Ambient
Cargo configuration and global or system Mise configuration are not accepted
as arbitrary gate inputs. The owner does not provide a host sandbox or
authenticate how an already-built executable was compiled.

Git's [`extensions.worktreeConfig`](https://git-scm.com/docs/git-config#Documentation/git-config.txt-extensionsworktreeConfig)
must already be enabled for the repository;
the installer refuses to run otherwise and does not migrate existing config.
This keeps hook registration scoped to the current checkout while leaving
common config available to other worktrees. Review existing Git configuration
and repository/worktree layout before enabling this extension manually; Git's
documentation notes that existing `core.worktree` and `core.bare` settings may
need to move into the main worktree config. The installer does not migrate them.
The installer disables the named `hk-pre-commit` hook in this checkout's
worktree config so an inherited global hook cannot bypass the repository-owned
snapshot boundary. Global configuration and unrelated hooks remain unchanged.

The hook selects steps from the staged diff (see `hk.pkl` for exact commands).
It follows the owner hk standard: the pre-commit hook only checks (`fix = false`),
and the same step list backs `hk check` and `hk fix`:

- `mise` (hk builtin): `mise fmt --check` on `mise.toml`
- `gitleaks` (hk builtin) and `kingfisher-staged`: secret scans of the staged change
- `cargo_fmt`, `cargo_check`, `cargo_clippy`, `cargo_deny` (hk builtins) for affected Cargo manifests; format, check and Clippy use hk's workspace directory and the guarded Mise execution prefix
- `rust-toolchain-mirror`: `rust-toolchain.toml` must mirror the `mise.toml` rust entry
  (channel, profile, components, targets)
- `rumdl` (hk builtin): Markdown
- `jactionlint` (check-only, the generated CI's flags and `.github/jactionlint.yaml`): workflows and composite actions
- `swiftlint`, `swift-format`: `native/**` on macOS; skipped elsewhere

`pre-push` runs `betterleaks git . --redact` (`scripts/hooks-install` registers it as `jackin-pre-push`). The full-history `security-audit`
hook never fires from Git; run it with `mise x -- hk run security-audit`.

Checks run in the private snapshot. Strict whole-workspace Clippy and
`xtask ci --only lint` remain required merge-readiness gates.

Review mode, not auto-stage: the launcher validates the exact staged tree in
an isolated temporary repository. Original tracked, untracked, ignored files,
file metadata, the index, stash stack, and compiler outputs stay untouched.
The isolated repository preserves the original HEAD and merge index, so hk
and affected-package selection see the original staged diff. Unmerged index
entries or staged symlinks escaping the snapshot fail before tools run.
Before checkout, the launcher limits staged blob content to 1 GiB, counting each
path separately. Temporary parents must preserve rename custody; root- or
effective-user-owned sticky temporary directories are supported. Colon-containing scratch
paths retain their location, with private PATH aliases under `/tmp` when needed.

If a step still changes source, the commit fails. The launcher saves a private
patch under the common Git directory's `jackin-hook-evidence/pre-commit.*/`
with directory mode 0700 and patch mode 0600. It prints the patch path, never
its contents. Review and apply the fixes explicitly:

```sh
git commit -m "..."       # isolated check fails
mise x -- hk fix          # apply fixes in your checkout
git diff                 # review all unstaged changes
git add -p && git commit -m "..."
```

`hk check` runs checks on modified files outside a commit. Partial commits
remain safe: the hook never clears, restores, or cleans the live worktree.
A successfully completed gate removes the invocation's temporary repository
and build outputs after terminating its tool process group. Failed gates and
execution errors retain the private snapshot and diagnostics and print the
evidence path. The launcher still terminates its tool process group before
returning. Review patches remain until the author removes them. Bypass with
`HK=0 git commit` (emergencies only).

One intentional divergence from CI: `hk check --all` scopes steps by `git status`
(skips green on a clean tree). The global install
(`hk install --global --mise`) is GUI-robust but forces `--staged`,
which disables stashing — hence the repo-owned hook registration above.
`scripts/hooks-install` sets `hook.hk-pre-commit.enabled=false` in this
checkout's worktree config while keeping the repository-owned snapshot hook
enabled. A global hk installation can remain active for other projects;
reinstalling it does not remove this checkout's ownership setting. Event-wide
pre-commit disabling fails installation.

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

`mise exec -- mbx +1.99.1 xtask ci --e2e` includes the Docker-backed lane. It first checks that Docker is running, builds and exports the local capsule binary, then runs `mise exec -- mbx +1.99.1 nextest run -p jackin --features e2e --profile docker-e2e`. In PR checkouts, `jackin-dev pr sync <PR_NUMBER>` still prepares the isolated env and capsule export for manual smoke tests; source `$(jackin-dev pr path <PR_NUMBER>)/env.sh` before manual `jackin` commands.

Local builds outside CI default to the package version for `JACKIN_VERSION` / `JACKIN_CAPSULE_VERSION` so each commit does not invalidate every build-meta consumer and capsule cache entry. GitHub Actions sets `CI`, so release, preview, construct, and CI builds still stamp the real `<version>+<sha>`. Set `JACKIN_VERSION_OVERRIDE=<value>` only when you need an explicit local version.

Fmt fail → `mise exec -- mbx +1.99.1 fmt`, then re-check with `mise exec -- mbx +1.99.1 fmt --check`.
