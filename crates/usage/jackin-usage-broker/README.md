# jackin-usage-broker

Host-only broker lifecycle and bounded Unix-socket transport: leader lease,
service, dispatch, capability checks, and binding rediscovery.

## Modules

| Module | Role |
|---|---|
| `service.rs` | service lifecycle |
| `serve.rs`, `client.rs` | socket transport |
| `ensure.rs`, `leader.rs` | activation and lease |
| other modules | auth and refresh |

`lib.rs` re-exports `ensure_usage_broker`, `run_usage_broker_service`,
`UsageBrokerClient`, `UsageBrokerHandle`, and capability-allowlist functions.

## Long socket paths

If the canonical data-directory socket exceeds the Unix-socket limit, a
stable alias uses `/private/tmp` on macOS or `/tmp` elsewhere; `TMPDIR` is
ignored. Set `UsageBrokerConfig::socket_alias_root` (or internal
`--socket-alias-root PATH`) to use another existing absolute root. The root
must be root-owned mode `01777` or effective-user-owned mode `0700`, with
trusted ancestors. Only its effective-user `0700` child is created; insecure
existing roots or children are rejected, never repaired. Restart the broker
after changing roots.
The socket file stays `0600`; only its name changes on alias.

## Verify

```sh
cargo nextest run -p jackin-usage-broker --no-tests warn
cargo clippy -p jackin-usage-broker --all-targets -- -D warnings
```
