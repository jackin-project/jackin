# jackin-usage-broker

Host-only usage broker lifecycle
and bounded Unix-socket transport:
leader lease, serve loop, dispatch,
capability authorization, and
binding rediscovery.

## What this crate owns

- Lifecycle (`ensure`, `leader`,
  `startup`, `service`, `handle`,
  `config`): activation, leader
  lease, and service entry points.
- Transport (`serve`, `dispatch`,
  `client`, `view`, `waits`):
  bounded Unix-socket serve loop
  and client.
- Auth (`authorize`,
  `capabilities`, `forwarding`,
  `errors`): capability allowlists
  and credential-scope proofs.
- Refresh (`executor`,
  `rediscover`, `projection`):
  binding rediscovery and
  projection loading.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`service.rs`](src/service.rs) | service entry | `tests/` |
| [`serve.rs`](src/serve.rs) | serve loop | `tests/` |
| [`ensure.rs`](src/ensure.rs) | activation | `tests/` |
| others | transport/auth/refresh | `tests/` |

## Public API

`ensure_usage_broker`,
`run_usage_broker_service`,
`UsageBrokerClient`,
`UsageBrokerHandle`, and the
capability-allowlist functions.

## How to verify

```sh
cargo nextest run -p jackin-usage-broker
cargo clippy -p jackin-usage-broker --all-targets -- -D warnings
```
