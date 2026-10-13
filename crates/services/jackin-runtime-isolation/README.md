# jackin-runtime-isolation

Mount-isolation facade: the
strategy sub-modules re-exported
for runtime call sites, with the
`MountIsolation` enum.

## What this crate owns

- Facade (`isolation`): `branch`,
  `cleanup`, `materialize`,
  `state`, `finalize`,
  `git_inspect`, `safe_remove`
  re-exported from
  `jackin-isolation`.
- Enum (`isolation`):
  `MountIsolation` +
  `ParseMountIsolationError`
  re-exported from `jackin-core`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`isolation.rs`](src/isolation.rs) | facade root | `isolation/tests/` |

## Public API

`isolation::branch`,
`isolation::cleanup`,
`isolation::materialize`,
`isolation::state`,
`isolation::finalize`,
`isolation::git_inspect`,
`isolation::safe_remove`,
`isolation::MountIsolation`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-isolation
cargo clippy -p jackin-runtime-isolation --all-targets -- -D warnings
```
