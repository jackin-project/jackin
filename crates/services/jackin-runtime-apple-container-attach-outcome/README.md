# jackin-runtime-apple-container-attach-outcome

Apple Container post-attach outcomes for launch and reconnect.

`record_attach_outcome` writes only for exact `Running`/`Stopped`; absence, errors, `Unknown`, and `Stopping` abstain. Recorder errors stay best-effort. See the [inventory contract](../../../docs/content/research/platform/isolation/apple-container-backend.mdx).

| Source | Tests |
|---|---|
| [`lib.rs`](src/lib.rs) | — |
| [`attach_outcome.rs`](src/attach_outcome.rs) | — |

Public API: `attach_outcome::record_attach_outcome`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-apple-container-attach-outcome
cargo clippy -p jackin-runtime-apple-container-attach-outcome --all-targets -- -D warnings
```
