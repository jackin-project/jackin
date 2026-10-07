# jackin-usage-store-backend

Single import chokepoint for the workspace `turso` `SQLite`
client: connections plus traced `DB` operations. Every
usage store opens connections only through `connect_local`
so a backend bump stays one file.

## What this crate owns

- Connections (`connect_local`): open a local `SQLite`
  database.
- Traced operations (`operation`, `DbOperation`):
  span + duration metric per `DB` call.
- Re-exports (`Connection`, `Row`, `params`): the
  only turso surface usage stores may name.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | connections + operations | [`tests.rs`](src/tests.rs) |

## Public API

`connect_local`, `operation`, `DbOperation`,
`Connection`, `Row`, `params`.

## How to verify

```sh
cargo nextest run -p jackin-usage-store-backend
cargo clippy -p jackin-usage-store-backend --all-targets -- -D warnings
```
