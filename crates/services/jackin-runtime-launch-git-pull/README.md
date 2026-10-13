# jackin-runtime-launch-git-pull

Git pull helpers
for workspace repos.

## What this crate owns

- Sources
  (`git_pull`):
  `git_pull_sources`,
  `pull_workspace_repos_with_git` —
  repo discovery
  from mounts.
- Pull (`git_pull`):
  `pull_git_sources_with_git`,
  `GitPullResult` —
  threaded pull
  execution.
- Report
  (`git_pull`):
  `print_git_pull_results`,
  `record_git_pull_results` —
  result output.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`git_pull.rs`](src/git_pull.rs) | pull + report | — |

## Public API

`git_pull::GitPullResult`,
`git_pull::pull_git_sources_with_git`,
`git_pull::git_pull_sources`,
`git_pull::print_git_pull_results`,
`git_pull::record_git_pull_results`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-git-pull
cargo clippy -p jackin-runtime-launch-git-pull --all-targets -- -D warnings
```
