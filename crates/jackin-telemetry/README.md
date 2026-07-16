# jackin-telemetry

The schema authority and governed instrumentation facade keep jackin❯ telemetry consistent across processes.

## What this crate owns

- The closed OpenTelemetry extension registry and generated Rust constants.
- Bounded semantic values and correlation-id minting.
- Privacy-safe typed error capture for arbitrary `Result` error types.

## Architecture tier and allowed dependencies

T0; no workspace dependencies. Ecosystem dependencies are limited to OpenTelemetry semantic conventions and UUID generation.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | Crate root and re-exports | — |
| [`schema.rs`](src/schema.rs) · [`schema/`](src/schema) | Generated attributes, events, spans, and bounded values | [`tests.rs`](src/schema/tests.rs) |
| [`event.rs`](src/event.rs) · [`error.rs`](src/error.rs) · [`operation.rs`](src/operation.rs) · [`metric.rs`](src/metric.rs) | Bounded governed signal facade | [`disabled_alloc.rs`](tests/disabled_alloc.rs) |
| [`spawn.rs`](src/spawn.rs) · [`propagation.rs`](src/propagation.rs) | Async ownership and W3C propagation | module tests |
| [`benches/`](benches) | Disabled-path benchmark and reviewed 5% cross-workload baseline | `cargo xtask telemetry-bench` |

## Public API

Import schema names and bounded values through `jackin_telemetry::schema`.

At a fallible operation's semantic owner, `ResultTelemetryExt` records any
`Err` as one typed event without formatting it and returns the result unchanged.
The operation guard separately completes failed span status with the same type.

## How to verify

`cargo nextest run -p jackin-telemetry --locked`

The scheduled performance lane runs the four product Criterion targets then
`cargo xtask telemetry-bench --capture`. Refresh the reviewed
[`baseline`](benches/baseline.json) only with inspected `--quick` medians and
reference machine/toolchain data; a regression above 5% fails.
