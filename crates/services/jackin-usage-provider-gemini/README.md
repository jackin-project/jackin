# jackin-usage-provider-gemini

`Gemini CLI` eligibility + project-quota snapshot: consumer-OAuth
retirement (2026-06-18), entitlement parsing, credential presence,
and project-scoped quota buckets. Consumed by `jackin-usage`
(credential snapshots, host discovery).

## What this crate owns

- Retirement (`GEMINI_CONSUMER_OAUTH_END`): consumer OAuth ended
  2026-06-18; Standard/Enterprise remain supported.
- Credentials (`gemini_oauth_creds_path`,
  `gemini_credential_presence`): presence only, secrets never read.
- Entitlement (`parse_gemini_entitlement`): tier/plan/project
  identity plus explicit consumer-unsupported flags.
- Migration (`gemini_migration_action`): targeted reconnect notice
  only on an explicit server signal.
- Project quotas (`parse_gemini_project_quotas`,
  `gemini_quota_buckets`): RPM/TPM/daily/model limits as count
  buckets, never invented denominators.
- Snapshot (`gemini_snapshot`, `gemini_snapshot_with_presence`):
  typed reporting gaps, never zero balances.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`adapter.rs`](src/adapter.rs) | eligibility + quota surface | [`tests.rs`](src/tests.rs) |

## Public API

`gemini_snapshot`, `gemini_snapshot_with_presence`, entitlement and
quota parsing, consumed by `jackin-usage`.

## How to verify

```sh
cargo nextest run -p jackin-usage-provider-gemini
cargo clippy -p jackin-usage-provider-gemini --all-targets -- -D warnings
```
