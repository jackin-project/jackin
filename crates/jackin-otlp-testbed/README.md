<!-- SPDX-FileCopyrightText: 2026 Alexey Zhokhov -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# jackin-otlp-testbed

Test-only OTLP/gRPC receiver used by jackin❯ conformance suites. It records typed trace, log, and metric requests and provides deterministic success, partial-success, delay, and gRPC-error behavior. It is never linked into product binaries or shipped as a Collector.

## What this crate owns

- Loopback implementations of the three OTLP collector services.
- Typed decoded-request accessors and namespace/privacy detector support.
- Deterministic exporter failure fixtures.

## Architecture tier and allowed dependencies

T3 test infrastructure. Product crates must never depend on it outside dev/test scopes.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | receiver, scripted behavior, captured-request accessors | module loopback and detector self-tests |

## Public API

`Testbed::start`, endpoint and signal accessors, `Behavior`, wait helpers, and detector results for acceptance tests.

## How to verify

`cargo nextest run -p jackin-otlp-testbed --locked`
