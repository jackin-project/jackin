# Coverage Ledger — unified-agent-usage

Item: `roadmap/unified-agent-usage/README.md` at commit `92d21efb`, ingested
2026-08-21. Override: none. Invocation context: planning and every implementation
slice stay on `chore/roadmap-unified-agent-usage` and PR #898; no new branch or
PR may be created.

## Screens

| ID | Screen/state | Item anchor | Spec | Plans | Status |
|---|---|---|---|---|---|
| S1 | Console Overview | §Screens/Console usage Overview | covered | 005 | DONE |
| S2 | Console Account Detail | §Screens/Console usage Account detail | covered | 005 | DONE |
| S3 | Console loading and refreshing-with-last-good | §Screens/Console usage Loading and refreshing | pending | pending | pending |
| S4 | Console successful empty inventory | §Screens/Console usage Empty inventory | pending | pending | pending |
| S5 | Console stale and partial-provider failure | §Screens/Console usage Stale and partial failure | pending | pending | pending |
| S6 | Console global failure | §Screens/Console usage Global failure | pending | pending | pending |
| S7 | Desktop Usage window and runtime/accessibility matrix | §Screens/Desktop usage | covered | 007 | DONE |
| S8 | Desktop provider-focused status popover | §Screens/Desktop status popover | covered | 007 | DONE |
| S9 | Bare CLI compact human output | §Screens/CLI usage output schematic | covered | 005 | DONE |
| S10 | CLI stale, partial, empty, total failure, JSON | §Screens/CLI usage output states | covered | 005 | DONE |
| S11 | Capsule Usage Overview | §Screens/Capsule usage Overview | covered | 006 | DONE |
| S12 | Capsule multi-account agent detail | §Screens/Capsule usage multi-account agent | covered | 006 | DONE |
| S13 | Capsule zero-agent empty and lifecycle/failure states | §Screens/Capsule usage states | covered | 006 | DONE |

## Capabilities

| ID | Capability | Item anchor | Spec | Plans | Status |
|---|---|---|---|---|---|
| F1 | One Rust-owned canonical account graph and immutable versioned projection | §Intent; §Data & integrations | pending | pending | pending |
| F2 | Current-config host membership across global/role/workspace/workspace-role scopes | D6; §Data & integrations | pending | pending | pending |
| F3 | Eight host providers and seven-provider desktop filter/order | D3; D15; D29 | pending | pending | pending |
| F4 | Stable canonical identity, deduplication, merge, and deterministic account ordering | D5; D33; Planning-owned closure | pending | pending | pending |
| F5 | Durable process-independent broker with shared in-flight generations | D5; W6 | covered | 003 | DONE |
| F6 | Broker-owned cache, retry, cancellation, crash recovery, and atomic last-good publication | D5; §Planning-owned closure | covered | 003 | DONE |
| F7 | Adaptive 2/5/15/30-minute refresh policy and due-on-open behavior | D22 | covered | 003 | DONE |
| F8 | Rust-owned quota semantics, formatting, category/order, summary selection, severity | D14; D30; §Data & integrations | covered | 002, 004 | DONE |
| F9 | Provider quota parity including Codex money cap, supported extra windows, OpenCode | D21; Q1-Q3; §Research | covered | 004 | DONE |
| F10 | Provider-backed Not started and confidence-gated rich-surface run-out estimate | D23; D24 | covered | 004 | DONE |
| F11 | Bare host CLI human/JSON projection and settled exit behavior | D1; D4; D9; D16; D27 | covered | 005 | DONE |
| F12 | Instance accounts/verify retained without cache/refresh bypass authority | D10; W3 | covered | 005 | DONE |
| F13 | Top-level Console Usage route using confirmed native Console grammar | D7; D8; D26 | covered | 005 | DONE |
| F14 | Capsule launch-config membership, lifecycle, previews, account tabs, and launch neutrality | D2; D11-D13; D28; D31-D32 | covered | 006 | DONE |
| F15 | Desktop filtered projection through sanitized boltffi DTOs; Swift display-only | D14-D15; §Data & integrations | covered | 007 | DONE |
| F16 | Desktop status modes, popover handoff, native Usage window, Settings retention | D17-D20; S7-S8 | covered | 007 | DONE |
| F17 | Stable selection; removal returns to Overview with persistent inline notice | D20; D34 | covered | 007 | DONE |
| F18 | Developer ID, notarization, immutable public artifact, Homebrew cask proof | D19; §Quality bar | blocked by explicit release authorization | 008 | REJECTED (external release authorization required) |
| F19 | Cross-surface fixture parity and no-direct-fetch architecture proof | §Quality bar | covered | 008 | DONE |
| F20 | All implementation executes on current branch and PR | Invocation context | covered | all | DONE |

## Flows

| ID | Flow | Screens touched | Spec | Plans | Status |
|---|---|---|---|---|---|
| W1 | Host Overview through Console | S1-S6 | covered | 005 | DONE |
| W2 | Host read through bare CLI | S9-S10 | covered | 005 | DONE |
| W3 | Instance inspection through CLI | S10, S11-S13 | covered | 005 | DONE |
| W4 | Capsule pre-session quota preview and initialization transition | S11-S13 | covered | 006 | DONE |
| W5 | Desktop status glance to Usage detail | S7-S8 | covered | 007 | DONE |
| W6 | Shared refresh, degradation, cancellation, owner exit, and recovery | S1-S13 | covered | 003, 005-008 | DONE |

## Must-not anchors

| ID | Statement | Reason | Registry |
|---|---|---|---|
| N1 | No duplicate canonical account rows | Identity truth and operator clarity | pending |
| N2 | No token pricing, session cost, spend history/trends, charts, or rankings | Limits-only product boundary | pending |
| N3 | No consumer/diagnostic direct provider calls, queued duplicate refresh, or secondary authority | One broker owns freshness | pending |
| N4 | No unstable source ordinal as durable account identity | Prevent fragmentation/aliasing | pending |
| N5 | No quota state authorizes or blocks Capsule launch/session actions | Observation is not policy | covered |
| N6 | No `agent_uninitialized` downgrade or conflation with provider failure | Lifecycle and freshness are independent | covered |
| N7 | No Capsule rows from global catalog/discovery, unresolved config, or capability alone | Resolved launch config owns membership | covered |
| N8 | No agent/runtime names in visible provider labels | Provider-only naming decision | pending |
| N9 | No unlike-window aggregation or severity/freshness/discovery-driven account reorder | Stable traceable summaries/navigation | pending |
| N10 | No silent account substitution after removal | Preserve operator intent | pending |
| N11 | No browser-cookie import/decryption, authenticated WebView, or scraping | Credential trust boundary | pending |
| N12 | No inferred Not started or weak/stale/CLI run-out estimate | Semantic truth and simple CLI | pending |
| N13 | No bars/cards/animation/pace/raw mode/interactive chrome in human CLI | CLI is a simple readout | covered |
| N14 | No prototype fixture/store/scenario/harness code copied into production | Reference behavior, not production architecture | pending |

## Quality bar

| ID | Statement anchor | Spec scenario(s) | Status |
|---|---|---|---|
| B1 | Console TUI repository rules and major-state render conformance | covered | 005 | DONE |
| B2 | Desktop native/Liquid Glass rubric and accessibility/runtime matrix | covered | DONE |
| B3 | Concurrent reads join one generation; every bypass removed | 003 process/coordinator proof; later consumer audit | DONE |
| B4 | Cross-surface Rust-owned value/label parity | covered | DONE |
| B5 | Console golden matrix including 80×24/focus/scroll/removal | covered | 005 | DONE |
| B6 | CLI golden matrix including JSON, TTY/non-TTY, plaintext | covered | 005 | DONE |
| B7 | Capsule golden matrix including lifecycle/account tabs/narrow/focus | covered | 006 | DONE |
| B8 | Signing/notarization/public artifact/Homebrew proof | blocked by explicit release authorization | REJECTED (external release authorization required) |

## Decisions (constraints)

| ID | Decision | Dated | Constrains |
|---|---|---|---|
| D1 | Bare CLI is host Overview; instance remains explicit | 2026-08-20 | F11-F12, W2-W3 |
| D2 | Capsule rows are resolved launch-config agents; uninitialized is typed | 2026-08-20 | F14, W4 |
| D3 | Eight host providers; desktop excludes OpenCode | 2026-08-20 | F3, F9, F15 |
| D4 | CLI and Console share canonical Rust projection | 2026-08-20 | F1, F11, F13 |
| D5 | One host broker owns provider work/freshness | 2026-08-20 | F5-F7, W6 |
| D6 | Current read-only discovery owns host membership | 2026-08-20 | F2, F4 |
| D7 | Console Usage is top-level split route | 2026-08-20 | F13, S1-S6 |
| D8 | Console ordering/states/keys/focus contract | 2026-08-20 | F13, S1-S6 |
| D9 | Human CLI group/account/window hierarchy and stable JSON | 2026-08-20 | F11, S9-S10 |
| D10 | Retain instance diagnostics; eliminate/redefine bypass forms | 2026-08-20 | F12, W3 |
| D11 | Capsule multi-account previews expose every canonical account | 2026-08-20 | F14, S11-S12 |
| D12 | Capsule ordering/states/refresh/selection contract | 2026-08-20 | F14, S11-S13 |
| D13 | Quota never blocks Capsule launch/session | 2026-08-20 | F14, N5 |
| D14 | Rust owns all visible quota semantics and strings | 2026-08-20 | F8, F11-F16 |
| D15 | Desktop is canonical graph filter with fixed order | 2026-08-20 | F3, F15-F16 |
| D16 | CLI partial/empty/total-failure exit contract | 2026-08-20 | F11, S10, W2 |
| D17 | Retain provider-focused status-item modes | 2026-08-20 | F16, S8 |
| D18 | Retain native popover/two-pane Usage/Settings IA | 2026-08-20 | F16, S7-S8 |
| D19 | Desktop completion includes signed public Homebrew delivery | 2026-08-20 | F18, B8 |
| D20 | Desktop alternative A without H is blessed | 2026-08-20 | F16, S7-S8 |
| D21 | Codex monthly individual money cap is in scope | 2026-08-21 | F9 |
| D22 | Broker adaptive 2/5/15/30 policy | 2026-08-21 | F7 |
| D23 | Not started requires explicit provider evidence | 2026-08-21 | F10, N12 |
| D24 | Confidence-gated run-out appears only on rich current detail | 2026-08-21 | F10, N12-N13 |
| D25 | Browser-cookie/WebView/scraping lane excluded | 2026-08-21 | N11, F9 |
| D26 | Console uses shipped frame and Capsule quota composition | 2026-08-21 | F13, S1-S6 |
| D27 | Human CLI stays deliberately simple | 2026-08-21 | F11, N13 |
| D28 | Capsule retains modal/agent tabs plus conditional account tabs | 2026-08-21 | F14, S11-S12 |
| D29 | Provider-only labels and fixed order | 2026-08-21 | F3, N8 |
| D30 | Overview summary uses first ranked real limit; no aggregate | 2026-08-21 | F8, N9 |
| D31 | Capsule Overview rows are agent/account pairs | 2026-08-21 | F14, S11 |
| D32 | Capsule entry preserves focused agent/account else Overview | 2026-08-21 | F14, W4 |
| D33 | Accounts sort locale-aware label then stable ID | 2026-08-21 | F4, F8 |
| D34 | Removed selection notice is persistent inline | 2026-08-21 | F17, S1, S7, S11 |

## External references & integrations

| ID | Reference | Kind | Research topics |
|---|---|---|---|
| R1 | `crates/jackin-usage/` | canonical usage/provider/broker code | agent-usage-platform 01, 04, 06, 07 |
| R2 | `crates/jackin-protocol/` | wire/control contracts | agent-usage-platform 01, 04, 06 |
| R3 | `crates/jackin/` CLI and console adapter | host CLI integration | agent-usage-platform 01, 06, 08 |
| R4 | `crates/jackin-console/` | Console TUI architecture | agent-usage-platform 01, 08 |
| R5 | `crates/jackin-capsule/` | Capsule modal/reference experience | agent-usage-platform 01, 08 |
| R6 | `crates/jackin-runtime/` relay | Capsule usage capability boundary | agent-usage-platform 01, 06, 08 |
| R7 | `crates/jackin-usage-ffi/` + generated bindings | Rust/native boundary | agent-usage-platform 01, 08 |
| R8 | `native/` | production macOS app | agent-usage-platform 02, 08 |
| R9 | `native/Design/Prototypes/UnifiedAgentUsage/` | blessed visual/interaction reference | item §Native design preparation; topic 08 |
| R10 | CodexBar repository | provider/reference behavior | agent-usage-platform 03, 07 |
| R11 | OpenUsage repository | broker/provider/reference behavior | agent-usage-platform 03, 07 |
| R12 | Apple signing/notarization/Gatekeeper APIs | distribution | agent-usage-platform 02, 08 |
| R13 | Homebrew cask commands/docs | install distribution | agent-usage-platform 02, 08 |
| R14 | `mise.toml`, CI, repo testing docs | verification commands | agent-usage-platform 05, 08 |

## Assumptions

| ID | Assumption | Why safe | Falsified by | Status |
|---|---|---|---|---|
| A1 | All implementation remains on `chore/roadmap-unified-agent-usage` and PR #898 | Explicit invocation context; compatible with active-branch repo law | Operator changes branch/PR instruction | holds |
| A2 | External signing/publication credentials may be absent during implementation | Roadmap identifies them as external inputs; local proof can be built without secret values | Required public release gate cannot be exercised and no placeholder path exists | holds |

## Research questions

| ID | Question | Research topic | Status |
|---|---|---|---|
| Q1 | Codex/Z.AI semantic window classifier | agent-usage-platform/07 | direction closed; captured fixtures Plan 004 |
| Q2 | Supported Grok quota source | agent-usage-platform/07 | direction closed; authenticated/failure proof Plan 004 |
| Q3 | OpenCode authentication and usage contract | agent-usage-platform/07 | closed; unresolved identity is explicit |

## Deferred

| ID | Deferral | Reason and revisit trigger |
|---|---|---|
| X1 | Provider catalog expansion | After settled parity and supported API/trusted credential lane |
| X2 | Provider incident badges | Separate post-usage operational-health roadmap |
| X3 | Per-credit expiry timelines/notifications | After reliable individual expiries and notification-policy roadmap |
| X4 | Codex code-review quota | When supported API or trusted non-browser credential lane exists |

## Planned traceability resolution

The intake tables above preserve the pre-plan `pending` cells for audit history. The
authoritative resolved mapping is:

| Coverage | Specification | Implementation plans | Planned status |
|---|---|---|---|
| S1-S6, F13, W1, B1, B5 | `spec/console-usage.md` | 001, 005, 008 | covered |
| S7-S8, F15-F17, W5, B2 | `spec/desktop-usage.md` | 001, 002-004, 007-008 | covered |
| S9-S10, F11-F12, W2-W3, B6 | `spec/cli-usage.md` | 001-005, 008 | covered |
| S11-S13, F14, W4, B7 | `spec/capsule-usage.md` | 001-004, 006, 008 | covered |
| F1-F4, F8, F17 | `spec/canonical-projection.md` | 001-002, 005-008 | covered |
| F5-F7, W6, B3 | `spec/broker-refresh.md` | 001-003, 005-008 | covered |
| F3, F8-F10, Q1-Q3 | `spec/provider-quotas.md` | 001-004, 008 | covered |
| F18-F20, B4, B8, N1-N14 | `spec/parity-release.md` and sole registry | 001-008 | covered |

All plans execute on `chore/roadmap-unified-agent-usage` in PR #898. No coverage
item authorizes another branch or PR.

## Execution evidence

| Plan | Delivered coverage | Proof |
|---|---|---|
| 001 | Contract fixtures, baseline ownership, N1-N14 enforcement inventory | `contract_baseline` 5; repository format/lint/test |
| 002 | F1-F4 canonical V1 foundation; F8 typed category/order seam; F17 destination normalization; N1, N4, N8-N10, N14 | canonical 5; protocol 8; crate nextest 290; full workspace and E2E gates |
| 003 | F5-F7 broker authority, policy, projection operations, atomic envelope, N3 process activation seam | `broker_service_lifecycle` 1; coordinator 14; broker 8; policy 2; state 6; protocol 9; usage clippy; dependent checks |
| 004 | F3, F8-F10, F9 provider parity, Q1-Q3 adapter contracts and typed failures | 4 focused fixtures; usage 146; host 59; usage clippy |
| 005 | S1-S2, S9-S10, F11-F13, W1-W3, B1, B5-B6, N13 | jackin-console 1286; jackin CLI tests; console/jackin clippy; fmt; mise lint |
| 006 | S11-S13, F14, W4, B7, N5-N7 | relay inventory 1; capsule projection 1; capsule/runtime clippy; fmt |
