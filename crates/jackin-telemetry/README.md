# jackin-telemetry

The schema authority and governed instrumentation facade keep jackin❯ telemetry consistent across processes.

## What this crate owns

- The closed OpenTelemetry extension registry and generated Rust constants.
- Bounded semantic values and correlation-id minting.

## Architecture tier and allowed dependencies

T0 — must match the TIERS table in `crates/jackin-xtask/src/arch.rs`.
Allowed workspace dependencies: none. Ecosystem dependencies are limited to OpenTelemetry semantic conventions and UUID generation. It must not depend on another `jackin-*` crate.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | Crate root and re-exports | — |
| [`schema.rs`](src/schema.rs) · [`schema/`](src/schema) | Generated attributes, events, spans, and bounded values | [`tests.rs`](src/schema/tests.rs) |
| [`event.rs`](src/event.rs) · [`operation.rs`](src/operation.rs) · [`metric.rs`](src/metric.rs) | Bounded governed signal facade | [`disabled_alloc.rs`](tests/disabled_alloc.rs) |
| [`spawn.rs`](src/spawn.rs) · [`propagation.rs`](src/propagation.rs) | Async ownership and W3C propagation | module tests |
| [`benches/`](benches) | Disabled-path benchmark and reviewed 5% cross-workload baseline | `cargo xtask telemetry-bench` |

## Public API

Import schema names and bounded values through `jackin_telemetry::schema`.

## How to verify

`cargo nextest run -p jackin-telemetry --locked`

The scheduled performance lane runs the launch, console-frame, pane-body, and
PTY byte-pump Criterion targets, then executes
`cargo xtask telemetry-bench --capture`. Refresh
[`benches/baseline.json`](benches/baseline.json) only from a reviewed
performance-affecting change: run those four targets with `--quick`, inspect the
Criterion medians, replace the baseline values, and include the reference
machine/toolchain in `reference`. A current median more than 5% above its
reviewed baseline fails the comparator.
