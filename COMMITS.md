# Commits

Covers commit message format, DCO sign-off, push cadence, and merge-readiness verification.

## Commit Messages

All commits in this repository must follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/).

Subject format:

```text
<type>[optional scope][!]: <description>
```

Allowed types:

| Type | Use |
|---|---|
| `feat` | New user-visible feature |
| `fix` | Bug fix |
| `docs` | Documentation-only change |
| `style` | Formatting or whitespace with no logic change |
| `refactor` | Internal restructuring with no behavior change |
| `perf` | Performance improvement |
| `test` | Adding or updating tests |
| `build` | Build system, tooling, or dependencies |
| `ci` | CI configuration |
| `chore` | Maintenance, release, merge-sync commits, or dependencies |
| `revert` | Reverts a prior commit |

Scope is optional but encouraged when it clarifies the changed area, for example `feat(launch): preview resolved mounts`.

Breaking changes use `!` after type or scope, such as `feat!:` or `feat(api)!:`, and include a `BREAKING CHANGE:` footer in the body.

PR squash-merge titles become commit subjects, so PR titles must follow the same convention.

## Sign-Off

Every commit must include a `Signed-off-by` trailer matching the commit author. Contributions are signed under [DCO v1.1](https://developercertificate.org/) and enforced by the [DCO2 GitHub App](https://github.com/cncf/dco2); an unsigned commit blocks the PR.

Create commits with `-s`:

```sh
git commit -s -m "feat(scope): description"
```

When amending, re-sign:

```sh
git commit --amend -s --no-edit
```

If a DCO check fails, fix it before doing other work. Amending and force-pushing requires explicit operator approval for that branch or PR; see [BRANCHING.md](BRANCHING.md).

The `Signed-off-by` trailer must match the commit author.

## Push After Every Commit

After every `git commit`, immediately run `git push`. Do not leave commits in local-only state. This applies to every branch unless the operator explicitly says to hold the push.

```sh
git commit -s -m "feat(scope): description"
git push
```

## Merge-Readiness Verification

Do not run the full verification suite before every commit by default. Run it when a pull request is ready to merge, or earlier only when the operator explicitly asks.

Use the aggregate local CI gate:

```sh
mise exec -- mbx +1.99.1 xtask ci
```

To skip the docs build, run `--fast` or `--no-docs` (both skip only that step):

```sh
mise exec -- mbx +1.99.1 xtask ci --fast
```

To run named steps only, once each in plan order, use `--only` with a comma-delimited list from `fmt clippy nextest deny shear comments layers pins containers errors baseline screen demo-gate reuse docs`. `--only` cannot combine with `--fast` or `--no-docs`. A usage error exits with status 2, and a failed step exits with status 1:

```sh
mise exec -- mbx +1.99.1 xtask ci --only comments,layers,pins,containers
```

`xtask e2e` runs the live journey against a gateway. It is not part of `xtask ci`.

`xtask baseline-branch` checks with `gh` that the pinned `terminal-components-baseline` rev is on `feature/openshell-native` (`ahead` or `identical`). It needs the network, so it is not part of `xtask ci`. Run it at every pin bump.

Set `JACKIN_TOOLCHAIN_GUARD` to an absolute executable path when local policy requires a guard for toolchain commands. The shared xtask command launcher prefixes Cargo and mbx commands with that executable and `-- mise exec --`, including commands launched by nested xtasks. The guard must admit the requested commands; workspace doctests use `mbx test --doc --workspace --locked`.

If formatting fails, run `mise exec -- mbx +1.99.1 fmt`, then re-run `mise exec -- mbx +1.99.1 fmt --check`.
