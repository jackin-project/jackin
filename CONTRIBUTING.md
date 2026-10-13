# Commits, Branching & Contributing

## License

Apache 2.0. Contributions licensed under same terms (Section 5).

## DCO

All contributions signed off under [DCO v1.1](https://developercertificate.org/). Enforced by [DCO2 GitHub App](https://github.com/cncf/dco2) — unsigned commit blocks PR.

Employer contributions: confirm authorization before submitting. Use personal email in commit author + sign-off.

## How to Submit

1. Fork. Branch feature off `main`.
2. Run `mise install` from the repo root to install the pinned toolchain and dev tools.
3. Run `hk install` once per checkout to install the git hooks (see "Git hooks" below).
4. Change. Sign every commit: `git commit -s`.
5. Open a PR describing the problem solved. CI must pass.

## Git hooks

Pre-commit and pre-push checks run via [hk](https://hk.jdx.dev) (`hk.pkl` at
the repo root; the version is pinned in `mise.toml`/`mise.lock`). Install per
checkout — idempotent, safe to re-run:

```sh
mise install
hk install
```

Rust formatting and Clippy use hk's builtins, once per Cargo workspace:

- `cargo-fmt`: `cargo fmt --manifest-path Cargo.toml` (check and fix)
- `cargo-clippy`: `cargo clippy --locked --manifest-path Cargo.toml --all-targets --all-features -- -D warnings`

`actionlint` lints `.github/workflows/*.yml`. `swiftlint` and `swift-format`
cover `native/**` on macOS and skip themselves where those tools are absent.

```sh
hk check          # checks changed files
hk check --all    # checks every matching file
hk fix            # apply fixes in the checkout
```

The pre-commit hook stashes unstaged changes, runs fixes, and stages the
fixed files. Bypass with `HK=0 git commit` (emergencies only).

Linux developers use the same setup (`mise install`, `hk install`). CI on
`ubuntu-26.04` runs workspace `cargo fmt --check` and
`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.

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
