# Commits

This file covers commit message format, DCO sign-off, agent attribution
trailers, and pre-commit verification. All four apply to every commit in
this repository.

## Commit Messages

All commits in this repository MUST follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/).

Subject format: `<type>[optional scope][!]: <description>`

Allowed types:

| Type       | Use for                                                |
| ---------- | ------------------------------------------------------ |
| `feat`     | New user-visible feature                               |
| `fix`      | Bug fix                                                |
| `docs`     | Documentation-only change                              |
| `style`    | Formatting, whitespace; no logic change                |
| `refactor` | Internal restructuring; no behavior change             |
| `perf`     | Performance improvement                                |
| `test`     | Adding or updating tests                               |
| `build`    | Build system, tooling, dependencies                    |
| `ci`       | CI configuration                                       |
| `chore`    | Routine maintenance (release, merge, deps)             |
| `revert`   | Reverts a prior commit                                 |

Scope is optional but encouraged when it clarifies the change area, e.g., `feat(launch): preview resolved mounts per agent in TUI`.

Breaking changes use `!` after the type/scope (`feat!:` or `feat(api)!:`) and include a `BREAKING CHANGE:` footer in the body.

PR squash-merge: the PR title becomes the commit subject, so PR titles must also follow this convention.

## Sign-off (DCO)

Every commit in this repository MUST include a `Signed-off-by` trailer
matching the commit author. The `jackin-project` org enforces the
Developer Certificate of Origin via the
[DCO2 GitHub App](https://github.com/cncf/dco2); any PR containing an
unsigned commit will be blocked by the required `DCO` status check.

Create commits with `-s`:

```sh
git commit -s -m "feat(scope): description"
```

When amending, re-sign:

```sh
git commit --amend -s --no-edit
```

If you forget `-s` and the `DCO` check fails, fix it before doing
anything else. Amend with sign-off and force-push the branch (force-push
requires explicit operator approval per [BRANCHING.md](BRANCHING.md)):

```sh
git commit --amend -s --no-edit
git push --force-with-lease origin <branch>
```

The `Signed-off-by` trailer must match the commit author, not any
`Co-authored-by` trailer (Claude, Codex, etc.). If `git config user.email`
is not set to the expected personal address, correct it **before**
committing — do not paper over a wrong-author commit with an unrelated
sign-off.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full DCO v1.1 text.

Agent-specific attribution trailer requirements (e.g., for the Codex agent) are in [AGENTS.md](AGENTS.md).

## Pre-commit Verification

Before committing **any** change, run all three checks and ensure zero warnings and zero failures:

```sh
cargo fmt -- --check && cargo clippy -- -D warnings && cargo nextest run
```

The `-- -D warnings` flag promotes clippy warnings to hard errors, matching what CI runs. Without it, lints like `clippy::branches_sharing_code` and `clippy::doc_markdown` exit clippy with status 0 locally but fail CI — wasting a round-trip. Use the strict invocation locally so CI never catches a lint your local check missed.

If formatting fails, run `cargo fmt` to fix it, then re-run the checks.

See [TESTING.md](TESTING.md) for test runner setup, commands, and additional details.
