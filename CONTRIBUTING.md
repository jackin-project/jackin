# Commits, Branching & Contributing

## License

Apache 2.0. Contributions license under same terms (Section 5).

## DCO

All contributions signed off under [DCO v1.1](https://developercertificate.org/). Enforced by [DCO2 GitHub App](https://github.com/cncf/dco2) — unsigned commit blocks PR.

Employer contributions: confirm authorization before submitting. Use personal email in commit author + sign-off.

## How to Submit

1. Fork. Create feature branch from `main`.
2. Make changes. Sign every commit: `git commit -s`.
3. Open PR describing problem solved. Ensure CI passes.

## Branching

Never commit to `main`. All work on dedicated branch.

Names: `feature/`, `fix/`, `refactor/`, or `chore/` prefix + short lowercase hyphen-separated description.

### Stay on active branch (agent)

**Never create new branch when existing feature branch or open PR is in scope.**

- Session start: `git branch --show-current` + `gh pr list --head <branch>`. Open PR → all work on that branch.
- On `main`: propose `<prefix/name>`, ask: "This is on `main`. I suggest `<branch>`. Should I create it?" Wait for confirmation.
- Work feels like different branch: ask first. Default: stay on active branch.
- Never push to remote branch other than what local tracks. Local `pr-435` vs remote `fix/foo` → `git push origin HEAD:<remote-branch>`. Don't create extra remote branches.

### Sync with main: rebase only, never merge

```bash
git fetch origin
git rebase origin/main
git push --force-with-lease origin <branch>   # requires operator approval
```

Never `git merge main`. Merge commits drag bot/renovate commits into PR diff → breaks DCO + squash-merge. Rebase keeps history clean, DCO scoped to author's own commits.

### Force pushes (agent)

Never `git push --force` / `--force-with-lease` without explicit operator approval for that branch/PR in current conversation.

Normal pushes (new commits): no approval needed. History rewrites (amend DCO, rebase, squash): ask first, name branch + reason. Prefer follow-up commit unless operator requests rewrite.

`git fetch -f` OK — updates local remote-tracking refs only, not remote branch.

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

DCO fail on PR: fix before anything else.

## Push After Every Commit (agent)

Push immediately after every `git commit`. No local-only commits.

```sh
git commit -s -m "feat(scope): description"
git push
```

Exception: explicit operator instruction to hold.

## Merge-Readiness Check

Run when PR ready to merge (not before every commit):

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets
cargo nextest run --all-features
```

Fmt fail → `cargo fmt`, re-check. See [TESTING.md](TESTING.md).
