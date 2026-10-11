# jackin-runtime-repo-cache

Role-repo clone, validation,
and cache.

## What this crate owns

- Resolve (`repo_cache`):
  `register_agent_repo`,
  `resolve_agent_repo_with`,
  `RepoResolveOptions`.
- Cache (`repo_cache`):
  `RepoError`,
  `normalize_github_url`,
  `RepoLock`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`repo_cache.rs`](src/repo_cache.rs) | resolve + cache | `repo_cache/tests/` |
| [`repo_cache/`](src/repo_cache/) | test suites | `repo_cache/tests/` |

## Public API

`repo_cache::register_agent_repo`,
`repo_cache::resolve_agent_repo_with`,
`repo_cache::RepoError`,
`repo_cache::normalize_github_url`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-repo-cache
cargo clippy -p jackin-runtime-repo-cache --all-targets -- -D warnings
```
