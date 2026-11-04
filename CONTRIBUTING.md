# Contributing

Jackin runs Codex in NVIDIA OpenShell sandboxes. [AGENTS.md](AGENTS.md) holds the rules for code, tests, and work. [COMMITS.md](COMMITS.md) holds the commit format and the merge-readiness gate. [BRANCHING.md](BRANCHING.md) holds the branch rules. This file covers what you do to submit a change.

## License

Apache 2.0. Contributions are licensed under the same terms (section 5 of the license).

## DCO

Sign off every commit under [DCO v1.1](https://developercertificate.org/). The [DCO2 GitHub App](https://github.com/cncf/dco2) enforces it, and an unsigned commit blocks the pull request.

Employer contributions: confirm that you are authorized before you submit. Use your personal email address in the commit author and the sign-off.

## How to submit

1. Fork the repository. Branch from `feature/openshell-native`. Never commit to `main`.
2. Run `mise install` in the repository root to install the pinned toolchain and the development tools.
3. Run `mise exec -- hk install --mise` once for each checkout to install the Git hooks.
4. Make the change. Sign every commit with `git commit -s`.
5. Open a pull request that describes the problem it solves. CI must pass.

Run every Cargo command through `mise exec -- mbx +1.99.1`, for example `mise exec -- mbx +1.99.1 build -p jackin`. Run tests with `mise exec -- mbx +1.99.1 nextest run`. Never use `cargo test`.

## Git hooks

Hooks run through [hk](https://hk.jdx.dev). `hk.pkl` defines the steps. `mise.toml` and `mise.lock` pin hk and every tool that the steps start.

```sh
mise exec -- hk install --mise
```

The command is safe to run again. `--mise` runs each hook through `mise x`, so the pinned tools are on `PATH` without shell activation. You need Git 2.54 or newer and `mise` on the `PATH` that Git uses for hooks. If hk is already installed in your global Git configuration, the per-checkout install is skipped. Pass `--force-local` to install it anyway.

`pre-commit` only checks. It does not fix files and does not stash. It runs these steps:

- `mise` and `gitleaks`
- `cargo_fmt` and `cargo_clippy` (workspace, all targets, all features, `-D warnings`)
- `xtask_repo_gates` (`xtask ci --only comments,layers,pins,containers,errors`)
- `rumdl` (Markdown, except `rewrite/` and `README.md`)
- `jactionlint` and `zizmor` (workflows and composite actions)
- `kingfisher-staged` (Kingfisher on the staged change, with `.kingfisher-baseline.yml`)

`pre-push` runs `betterleaks` over the commits that are not on a remote, and `mise run ci:fast` (`xtask ci --fast`).

If a hook step fails, apply the fixes and commit again:

```sh
mise exec -- hk fix       # apply fixes in your checkout
git diff                  # review all unstaged changes
git add -p && git commit -s
```

`mise exec -- hk check --all` runs every check step over all tracked files, outside a commit. To bypass a hook in an emergency, use `HK=0 git commit`.

## Secrets

Never write a secret in a file, a log, a label, a commit, or a test. Use `op` for 1Password references.

CI runs gitleaks, betterleaks, and Kingfisher as the `secrets-scan` check. Run the same scan locally with `mise run check:secrets-scan`. For a finding, either remove it from the source or review it and record it in the file of the scanner that reported it. `.gitleaks.toml` holds the path and pattern allowlists that gitleaks and betterleaks both read. `.gitleaksignore` holds fingerprints for gitleaks. `.betterleaksignore` holds fingerprints for betterleaks. `.kingfisher-baseline.yml` holds the Kingfisher baseline. Refresh it with `--manage-baseline` in a clean clone that has no linked worktrees, and review the diff.

The full-history `security-audit` hook never fires from Git. Run it with `mise exec -- hk run security-audit`.

## Documentation

The documentation site is in `docs/`. Its pages are in `docs/content/`. The page `docs/content/(public)/commands/reference.mdx` is generated from `jackin --help`. Regenerate it with `mise exec -- bun run --cwd docs gen:cli` after you change a command, an option, or its help text. Never edit it by hand.

## Merge-readiness check

Run the full gate when the pull request is ready to merge, not before every commit:

```sh
mise exec -- mbx +1.99.1 xtask ci
```

For a faster pass without the docs build, add `--fast`. [COMMITS.md](COMMITS.md) lists the steps and the options.

## Sign-off remediation

For a published commit without a sign-off, the original contributor can append a DCO remediation commit. Use the full 40-character SHA of the original commit:

```sh
GIT_AUTHOR_NAME="ORIGINAL AUTHOR" \
GIT_AUTHOR_EMAIL="author@email" \
GIT_COMMITTER_NAME="ORIGINAL AUTHOR" \
GIT_COMMITTER_EMAIL="author@email" \
git commit -s --allow-empty \
  -m "DCO remediation for ORIGINAL AUTHOR <author@email>" \
  -m "I, ORIGINAL AUTHOR <author@email>, hereby add my Signed-off-by to this commit: <full 40-character commit SHA>"
```

Use this for your own contribution only. Use the exact name and email from the original commit in the declaration, the author, the committer, and the `Signed-off-by` trailer. The commit may be empty. Third-party remediation is disabled, and DCO sign-off stays required for organization members.
