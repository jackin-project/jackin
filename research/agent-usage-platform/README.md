# Agent usage platform research

Vetted: 2026-08-20

## Scope

This topic establishes the architectural, native macOS, reference-implementation, and delivery evidence needed to unify subscription and quota-limit usage across `jackin usage`, `jackin console`, `jackin-capsule`, and jackin❯ desktop.

## Chapters

- [Codebase architecture and failure modes](01-codebase-architecture.md)
- [Apple-native design and distribution evidence](02-apple-native-design.md)
- [Reference implementations and delivery directions](03-reference-implementations.md)
- [Contract and proof matrix](04-contract-and-proof-matrix.md)
- [Verification ledger](05-verification-ledger.md)
- [Broker and canonical projection planning freeze](06-broker-projection-planning.md)
- [Provider contracts for planning](07-provider-contracts-planning.md)
- [Surface and release tooling map](08-surfaces-release-tooling.md)
- [Shipped surface review](09-shipped-surface-review.md)

## Conclusions

1. **The settled one-broker rule requires a process-independent per-user service boundary.** The current leader dies with its first caller, PID-only liveness can accept a reused process ID, and an already-live leader ignores a new caller's catalog and resolver. Whether the service is always resident or demand activated, it must survive the activating client and own provider calls, catalog revision, deadlines, retry state, last-good persistence, and protocol negotiation. — [Codebase architecture](01-codebase-architecture.md), [reference directions](03-reference-implementations.md), [broker proof matrix](04-contract-and-proof-matrix.md#broker-direction-proof)
2. **Account identity needs an explicit evidence ladder, not a source ordinal.** Provider account IDs and provider-stable non-secret handles are stronger than configuration provenance. When neither is available before authentication, the projection needs a typed provisional state and a later evidence-based alias or merge; persisting `source-NNNN` as canonical identity is ruled out. Exact provider behavior remains an eight-provider implementation matrix rather than a claim the available evidence can prove. — [Codebase identity findings](01-codebase-architecture.md), [reference identity findings](03-reference-implementations.md)
3. **One versioned Rust-owned projection is the only direction consistent with all settled surfaces.** It must carry stable identity, settled ordering, grouped limit windows, lifecycle and freshness state, last-good timestamps, and structured provider failures. Rust owns labels, rounding, countdowns, units, collision and merge precedence, and partial-failure semantics; CLI, TUI, Capsule, FFI, and Swift adapt layout only. — [Codebase projection findings](01-codebase-architecture.md), [shared-projection reference direction](03-reference-implementations.md)
4. **Cache semantics must be broker operations rather than caller conventions.** Current bypasses and process-local scheduling show that single-flight provider calls alone are insufficient. Normal, forced, and no-refresh reads, cancellation, deadlines, locking, persistence, crash recovery, and retry restoration need one executable broker contract and adversarial tests. — [Codebase broker findings](01-codebase-architecture.md)
5. **Current read-only discovery remains the membership authority.** Global, role, workspace, and workspace-role discovery determine current accounts; durable history may enrich but not create members. The host graph contains eight settled surfaces, while jackin❯ desktop derives its seven-provider view by excluding OpenCode without creating another discovery or identity path. — [Codebase discovery findings](01-codebase-architecture.md)
6. **The existing native architecture is the evidence-backed baseline.** System status items, the native popover, retained split Usage window, and Settings already provide the correct macOS structure. Standard AppKit and SwiftUI components should own Liquid Glass, focus, keyboard, accessibility, contrast, transparency, and localization behavior; custom-painted glass or structural replacement requires a demonstrated defect. — [Apple-native evidence](02-apple-native-design.md), [native reference findings](03-reference-implementations.md)
7. **Repository dependencies impose a verifiable implementation order.** Identity and protocol records precede broker and projection work; consumer migrations then proceed through CLI/diagnostics, console/Capsule, FFI/Swift, native QA, and signed distribution. Each migrated consumer needs a regression gate proving it cannot fetch providers independently. — [Codebase dependency findings](01-codebase-architecture.md), [Apple distribution evidence](02-apple-native-design.md)
8. **Capsule membership and initialization are separate from host discovery and quota freshness.** The current fully resolved instance launch configuration exclusively owns agent rows. Before the first session, an eligible agent carries typed `agent_uninitialized`; a resolved usage capability may add quota preview rows without clearing that lifecycle error. Fixed/global tabs and capability-only rows are excluded, and no usage state gates launch. — [Contract and proof matrix](04-contract-and-proof-matrix.md#capsule-membership-and-lifecycle-contract), [current Capsule findings](01-codebase-architecture.md)

## Shipped surface review

[09 — Shipped surface review](09-shipped-surface-review.md) is the reviewed
reading of what the implemented surfaces do when executed at revision
`8400e14d`, with severity and a recommended order. Its raw evidence lives in the
verification ledger; the desktop composition gap is captured visually in
[`native/Design/UnifiedAgentUsage/ShippedComparison.html`](../../native/Design/UnifiedAgentUsage/ShippedComparison.html).

## Reproducible verification

[Verification ledger](05-verification-ledger.md) records the source revision,
literal bypass-audit commands, focused test commands/results, expected
assertions, zero-test/unavailable lanes, and exact later proof targets. It is the
authority for what was executed; prescriptive matrices elsewhere are not passes.

## Candidate broker directions

### Resident per-user service

An always-running per-user broker gives retry deadlines, refresh generations, protocol ownership, and last-good state one stable lifetime independent of CLI, Capsule relay, and desktop clients. It adds service installation, login/startup policy, idle resource use, upgrade negotiation, permissions, shutdown, and crash-recovery obligations. Terminal OpenUsage demonstrates the transport and polling shape, not jackin❯-specific guarantees. — [Reference direction](03-reference-implementations.md#resident-per-user-broker-with-a-socket-read-model)

### Demand-activated per-user service

A client starts the same independent broker service when its endpoint is absent, after which every caller becomes a thin client and the service survives the activator. It avoids unconditional startup but adds concurrent cold-start election, activation failure, endpoint readiness, version mismatch, idle-lifetime, and exact-generation join cases. The references demonstrate activation and in-process coalescing separately, not the required cross-process combination. — [Reference direction](03-reference-implementations.md#demand-activated-broker-with-durable-cache-and-broker-local-single-flight)

No citation distinguishes these directions for jackin❯ under concurrent cold start and owner exit. A bounded implementation spike must choose by proving the settled one-authority invariant; this is not further desk research.

## Ruled out

- Separate CLI, desktop, Capsule, or diagnostic refresh owners.
- Direct storage reads that bypass broker freshness and retry state.
- Source ordinals as durable account identity.
- A seven-provider desktop projection reused as the eight-provider host inventory.
- Custom-painted glass without a demonstrated native-component limitation.
- Spend, token-price, historical trend, sparkline, quota-pace, or provider cost-ranking features.

## Remaining proof work

These are implementation proofs, not unresolved product choices:

- Build the eight-provider identity matrix and prove every provisional-to-canonical transition or explicit unresolved state.
- Spike broker activation, concurrent cold starts, owner exit, PID reuse, protocol mismatch, crash recovery, and exact-generation joining before migrating consumers.
- Freeze every row in the [projection, identity, cache, and proof matrix](04-contract-and-proof-matrix.md), then freeze cross-surface fixtures and JSON compatibility rules before exposing the new bare command.
- Prove console loading, refresh, empty, stale, partial-error, global-failure, navigation, focus, and footer behavior with render-conformance fixtures.
- Complete running-app visual and accessibility evidence across appearance, contrast, transparency, keyboard, VoiceOver, localization, display, and release-artifact matrices.
- Verify one exact public-artifact digest through Developer ID signing, notarization, stapling, quarantine-aware Gatekeeper launch, and Homebrew cask install and uninstall.
