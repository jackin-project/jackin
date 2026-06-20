# Commits, Branching & Contributing

## License

Apache 2.0. Contributions license same terms (Section 5).

## DCO

All contributions sign off under [DCO v1.1](https://developercertificate.org/). Enforced by [DCO2 GitHub App](https://github.com/cncf/dco2) — unsigned commit blocks PR.

Employer contributions: confirm authorization before submit. Use personal email in commit author + sign-off.

## How to Submit

1. Fork. Branch feature off `main`.
2. Change. Sign every commit: `git commit -s`.
3. Open PR describing problem solved. CI must pass.

## Branching

Never commit to `main`. All work on own branch.

Names: `feature/`, `fix/`, `refactor/`, or `chore/` prefix + short lowercase hyphen description.

### Sync with main: rebase only, never merge

```bash
git fetch origin
git rebase origin/main
git push --force-with-lease origin <branch>   # requires operator approval
```

Never `git merge main`. Merge commits drag bot/renovate commits into PR diff → break DCO + squash-merge. Rebase keep history clean, DCO scoped to author own commits.

## Commit Format

[Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/). Subject: `<type>[scope][!]: <desc>`

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
| `chore` | Maintenance (release, merge, deps) |
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
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets
cargo nextest run --all-features
```

Fmt fail → `cargo fmt`, re-check. See [TESTING.md](TESTING.md).