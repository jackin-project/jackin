# jackin-usage-broker-wire

Broker socket and lease wire
primitives: constants, lease
ownership, run-directory hardening,
bounded probe budgets, and wire
error constructors.

## What this crate owns

- Wire (`consts`, `errors`):
  socket/lease constants and
  coordination error constructors.
- Lease (`lease`, `paths`):
  descriptor-bound ownership and
  run-directory hardening.
- Probe (`probe`): bounded budgets
  for synchronous provider probes.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`consts.rs`](src/consts.rs) | wire consts | (via `jackin-usage`) |
| [`errors.rs`](src/errors.rs) | error ctors | (via `jackin-usage`) |
| [`lease.rs`](src/lease.rs) | ownership | (via `jackin-usage`) |
| [`paths.rs`](src/paths.rs) | hardening | (via `jackin-usage`) |
| [`probe.rs`](src/probe.rs) | budgets | (via `jackin-usage`) |

## Public API

`BROKER_*`/`CONNECT_*`/`PUBLISH_TICK`
consts, `BrokerLease`,
`BrokerLeaseOwner`, `ServePolicy`,
`secure_run_directory`,
`validate_owned_*`,
`run_probe_with_budget`, and the
wire error constructors.

## How to verify

```sh
cargo nextest run -p jackin-usage-broker-wire
cargo clippy -p jackin-usage-broker-wire --all-targets -- -D warnings
```
