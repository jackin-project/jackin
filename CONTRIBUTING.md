# Commits, Branching & Contributing

## License

Apache 2.0. Contributions license under same terms (Section 5).

## DCO

All contributions signed off under [DCO v1.1](https://developercertificate.org/). Enforced by [DCO2 GitHub App](https://github.com/cncf/dco2) — unsigned commit blocks PR.

Full DCO v1.1 text:

> Developer Certificate of Origin
> Version 1.1
>
> By making a contribution to this project, I certify that:
>
> (a) The contribution was created in whole or in part by me and I have the right to submit it under the open source license indicated in the file; or
>
> (b) The contribution is based upon previous work that, to the best of my knowledge, is covered under an appropriate open source license and I have the right under that license to submit that work with modifications, whether created in whole or in part by me, under the same open source license (unless I am permitted to submit under a different license), as indicated in the file; or
>
> (c) The contribution was provided directly to me by some other person who certified (a), (b) or (c) and I have not modified it.
>
> (d) I understand and agree that this project and the contribution are public and that a record of the contribution (including all personal information I submit with it, including my sign-off) is maintained indefinitely and may be redistributed consistent with this project or the open source license(s) involved.

Employer contributions: if employer has rights to inventions, confirm authorization before submitting. Use personal email in commit author + sign-off. Non-trivial contributions: maintainers may ask confirm work outside employment agreement.

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
- Wrong remote branch created: `git push origin --delete <wrong-branch>` immediately.

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

## Trailer Preservation (agent)

Amending or rebasing: **always keep all existing trailers** (`Co-Authored-By:`, prior `Signed-off-by:`). Append new `Signed-off-by:` after — never remove any trailer.

```
# Correct order:
Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
Signed-off-by: Alexey Zhokhov <alexey@zhokhov.com>
```

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
