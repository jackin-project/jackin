<!-- SPDX-FileCopyrightText: 2026 Alexey Zhokhov -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# jackin-otlp-testbed

Test-only OTLP/gRPC receiver used by jackin❯ conformance suites. It records
typed trace, log, and metric export requests and supports deterministic gRPC
failure responses. It is never linked into product binaries or shipped as a
Collector.
