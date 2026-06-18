# Commits

Covers commit message format, DCO sign-off, agent attribution trailers, pre-commit verification. All four apply to every commit in this repository.

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

Scope optional but encouraged when it clarifies change area, e.g., `feat(launch): preview resolved mounts per agent in TUI`.

Breaking changes use `!` after type/scope (`feat!:` or `feat(api)!:`) and include a `BREAKING CHANGE:` footer in the body.

PR squash-merge: PR title becomes commit subject, so PR titles must also follow this convention.

## Sign-off (DCO)

Every commit in this repository MUST include a `Signed-off-by` trailer matching commit author. The `jackin-project` org enforces Developer Certificate of Origin via [DCO2 GitHub App](https://github.com/cncf/dco2); any PR containing an unsigned commit blocked by required `DCO` status check. See [cert-manager's sign-off guide](https://cert-manager.io/docs/contributing/sign-off/) for the DCO rationale and workflow.

Sign off every commit as you make it — `-s` is short for `--signoff`:

```sh
git commit -s -m "feat(scope): description"
# -s is shorthand for --signoff; both add the Signed-off-by trailer:
git commit --signoff -m "feat(scope): description"
```

When amending, re-sign:

```sh
git commit --amend -s --no-edit
```

If you forget `-s` and `DCO` check fails, fix it before doing anything else. Amend with sign-off and force-push branch (force-push requires explicit operator approval per [BRANCHING.md](BRANCHING.md)):

```sh
git commit --amend -s --no-edit
git push --force-with-lease origin <branch>
```

The `Signed-off-by` trailer must match commit author.

See [CONTRIBUTING.md](CONTRIBUTING.md) for full DCO v1.1 text.

## Push after every commit (agent rule)

After every `git commit`, immediately run `git push`. Never leave commits in local-only state. Applies to every commit on every branch — feature branches, fix branches, everything. No "push later" batching.

```sh
git commit -s -m "feat(scope): description"
git push
```

Only exception is explicit operator instruction to hold off.

## Merge-readiness Verification

Do not run full verification suite before every commit by default. Run it when a pull request is ready to merge, or earlier only when operator explicitly asks. Merge-readiness check:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check --all-targets
cargo nextest run --all-features
```

CI runs clippy across all targets with all enabled features, runs `cargo check --all-targets` to verify default-features compile path, then runs nextest with all enabled features (including feature-gated integration tests). The `e2e` feature is purely additive — `cargo nextest run --all-features` is a strict superset of default-features test suite, so a separate default-features `cargo nextest run` would re-execute every non-feature-gated test for no extra coverage. The `-- -D warnings` flag promotes clippy warnings to hard errors, matching what CI runs. Without it, lints like `clippy::branches_sharing_code` and `clippy::doc_markdown` exit clippy with status 0 locally but fail CI — wasting a round-trip. Use strict invocation locally so CI never catches a lint your local check missed.

If formatting fails, run `cargo fmt` to fix it, then re-run checks.

See [TESTING.md](TESTING.md) for test runner setup, commands, additional details.
