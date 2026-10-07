# jackin-usage-provider-claude

`Claude` / `Anthropic` usage snapshot collection: OAuth credentials,
CLI fallback, spend windows, and refresh waves. Consumed by
`jackin-usage` (credential snapshots, host discovery).

## What this crate owns

- CLI (`cli`): `Claude` CLI usage fallback + user agent.
- Credentials (`credentials`): OAuth credential loading, account
  email, organization type.
- Diagnostics (`diagnostic`): usage-output parsing and the usage
  diagnostic runner.
- Keychain (`keychain`, macOS): Keychain reads with injectable
  state for refresh waves.
- OAuth types (`oauth_types`): usage responses, limits, spend.
- Refresh (`refresh`): file probes, env tokens, wave resolution.
- Snapshots (`snapshot`): identity + profile snapshots.
- Spend (`spend`): OAuth spend buckets and dollar windows.
- Waves (`wave`): wave policy, scope restriction labels.
- Windows (`windows`): session/weekly window constants.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | [`tests.rs`](src/tests.rs) |
| [`cli.rs`](src/cli.rs) · [`credentials.rs`](src/credentials.rs) · [`diagnostic.rs`](src/diagnostic.rs) | CLI + credentials + diagnostics | — |
| [`keychain.rs`](src/keychain.rs) | Keychain reads | — |
| [`oauth_types.rs`](src/oauth_types.rs) | OAuth types | — |
| [`refresh.rs`](src/refresh.rs) · [`snapshot.rs`](src/snapshot.rs) | refresh + snapshots | — |
| [`spend.rs`](src/spend.rs) · [`wave.rs`](src/wave.rs) · [`windows.rs`](src/windows.rs) | spend + waves + windows | — |

## Public API

`claude_snapshot`, `claude_profile` helpers, OAuth/CLI fetch, and
usage types consumed by `jackin-usage`.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-claude
cargo clippy -p jackin-usage-provider-claude --all-targets -- -D warnings
```
