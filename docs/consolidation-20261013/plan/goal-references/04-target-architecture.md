# 04 — Target architecture and integration design

> **Planning status: PARTIAL EVIDENCE / IMPLEMENTATION BLOCKED.** All 79 discovered server branches have dossiers and all 50 distinct tips received some inspection. Complete unique-history/semantic closure, the operator's local inventory, Jackin test execution and independent review are not complete. These are hard execution gates, not silently waived work.

## Preferred architecture — conditional approval

Use the reviewed P layered workspace as the architecture candidate, while retaining M's protected behavior and the strongest approved U/T/recovery changes. Do **not** blindly choose P because it is newer or ahead. The implementation source-unit manifest is deliberately gated on full history and independent review in TASK-002/004. No 366-commit blanket merge has been approved.

## Verified package ownership at P

| Responsibility | Observed P package roots | Required invariant |
| --- | --- | --- |
| Apps | crates/apps/jackin; crates/apps/jackin-capsule | Thin entrypoints with real launch/restore behavior and packaged sidecar |
| Foundational types/config | crates/core/jackin-core; jackin-config; jackin-protocol; jackin-process | One authoritative type/protocol/config owner; no legacy duplication |
| Usage authority | crates/services/jackin-usage-broker; jackin-usage-coordinator; jackin-usage-discovery; jackin-usage-credential-resolver | Exact identity/capability/lease authority, single-flight and fail-closed unavailable state |
| Provider evidence | crates/services/jackin-usage-provider-core; provider-claude; provider-codex; usage-output | Typed errors and truthful per-window observation provenance |
| State and projection | crates/services/jackin-usage-store-backend; snapshot-store; token-monitor; broker-wire; projection | Explicit schema/wire transition; observation is not dispatch permission |
| Runtime durability | crates/services/jackin-runtime-coordination; repo-cache; cleanup-absent-guards; snapshot | Stable owned inodes, lock semantics, identity-aware cache and safe recovery |
| Launch seams | crates/services/jackin-runtime-launch-load-options; launch-dry-run; launch-programmatic-selection; launch-restore | Accepted CLI selections survive complete launch/restore path |
| Adapters | crates/adapters/jackin-docker; jackin-image; jackin-omp-store; jackin-usage-ffi | Preserve container/storage/native behavior and tests at boundaries |
| Diagnostics/telemetry | crates/services/jackin-diagnostics; jackin-telemetry; crates/testing/jackin-otlp-testbed | No sensitive payload leaks; retry distribution verified |
| Tooling/native | crates/tools/jackin-xtask; native/mise.toml; native/Package.swift; native/project.yml | Pinned/reproducible CI and complete Rust-to-Swift producer/consumer contract |

Source: [P Cargo.toml](https://github.com/jackin-project/jackin/blob/fba69d60c2373156dc0dcecee8e085c83776966f/Cargo.toml). This proves package-root membership, not the exact child-file destination of every old symbol. TASK-004 must map old files before editing and verify all names with Cargo metadata. Intermediate `jackin-runtime-apple-container-ensure-absent` is not a P member; selected cleanup ownership differs.

## Compatibility and migration decisions

Preserve required behavior, tests, data integrity and clear user-facing semantics. Repository AGENTS rejects permanent compatibility shims and permits explicit breaking changes; therefore implement an intentional, tested migration rather than keeping duplicate flat/layered paths indefinitely. A breaking change needs a documented rationale, selected client/wire handling, data predecessor tests and operator guidance. It never permits silently dropping an important capability.

U's schema-6/wire-v9 values are source requirements to reconcile, not automatic final version assignments. Inspect actual selected versions and supported predecessors. Test only disposable synthetic/cloned fixtures during normal execution. No live database, Keychain item, provider call or existing foreground broker may be mutated without separate explicit authorization.

## Security and behavior invariants

Credential authority is exact account/surface/source/material identity, not a provider alias guess, UI state or error string. Monitor presence cannot create task authorization. Foreground lease/monitor keepalive is distinct from passive idle expiry. Provider status and retry authority remain structured; freshness is per window and reset timestamps are never invented. Coordination inodes survive lock release and reject unsafe path/ownership shapes. Sidecar fallback is not chosen until ownership and capability tests establish its safety.

## CI evidence and native design

Use explicit push-head/artifact-bound obligations, not an observed-run denominator or first-parent history alone. Preserve missing/failed/cancelled/flaky/partial evidence and bind successful reuse to exact semantic input keys. Enforce the selected documented warm CI contract through measurements, without fabricating achieved timings. Regenerate outputs from current inputs and preserve handwritten files.

One native producer-consumer graph must connect the FFI build to both SwiftPM and XcodeGen consumers. Preserve selected profile/floor, test harnesses, UI per-test counts, release stamping, dSYM and format/lint/dead-code gates. Prefer the selected native-local Mise entry points, not stale generated unit names.

## Integration technique

Create the integration branch from M (or a revalidated newer target approved in TASK-001). Import complete reviewed commits only when all their hunks/dependencies are approved. Otherwise use targeted extraction or semantic reimplementation with source-attributed tests. Apply structural moves first, shared toolchain/CI next, independent feature slices after interfaces are stable, then state/native composition and whole-tree audit. The final remote path is an authorized squash PR, not direct main commits.
