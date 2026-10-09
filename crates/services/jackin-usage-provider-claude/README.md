# jackin-usage-provider-claude

`Claude` / `Anthropic` OAuth usage adapter and spend windows. Broker-owned
discovery supplies the selected account credential for each refresh.

## What this crate owns

- Credentials (`credentials`): OAuth credential parsing and account metadata.
- Keychain (`keychain`, macOS): serialized reads with explicit unattended or
  operator-initiated interaction policy.
- OAuth types (`oauth_types`): usage responses, limits, spend.
- Refresh (`refresh`): secret-bearing material supplied by discovery.
- Snapshots (`snapshot`): API-key route remains explicitly unsupported.
- Spend (`spend`): OAuth spend buckets and dollar windows.
- Waves (`wave`): one OAuth HTTP request, typed failure metadata and rate limits.
- Windows (`windows`): session/weekly window constants.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | [`tests.rs`](src/tests.rs) |
| [`credentials.rs`](src/credentials.rs) · [`keychain.rs`](src/keychain.rs) | credential parsing + policy-aware Keychain reads | — |
| [`oauth_types.rs`](src/oauth_types.rs) | OAuth types | — |
| [`refresh.rs`](src/refresh.rs) · [`snapshot.rs`](src/snapshot.rs) | broker material + API-key snapshot | — |
| [`spend.rs`](src/spend.rs) · [`wave.rs`](src/wave.rs) · [`windows.rs`](src/windows.rs) | spend + waves + windows | — |

## Public API

`fetch_claude_oauth_usage` accepts only the access token supplied to the
broker-owned refresh. It never invokes `claude -p /usage` or probes
`claude --version`. The broker holds `unattended_keychain_guard()` before
background discovery. `read_claude_keychain_item(service)` always prohibits UI.
`prepare_claude_keychain_auth(service)` is the explicit operator preparation API;
it checks that stdin, stdout and stderr are terminals before any Keychain access.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-claude
cargo clippy -p jackin-usage-provider-claude --all-targets -- -D warnings
```
