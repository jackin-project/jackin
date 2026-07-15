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

## Public API

Import schema names and bounded values through `jackin_telemetry::schema`.

## How to verify

`cargo nextest run -p jackin-telemetry --locked`
