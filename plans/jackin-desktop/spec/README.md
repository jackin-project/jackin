# Spec — jackin-desktop

Contract between `roadmap/jackin-desktop/README.md` (READY 2026-07-24) and
the plans. Plans implement requirements, never raw item prose. Vocabulary
from the item binds naming here (provider, account, enabled provider, Agent
Usage preview, Usage window, glance %: weekly except Amp Free daily).

Linked research (vet-checked current 2026-07-24 — every citation opened this
session): `research/agent-usage-provider-apis/` (ch. 01–11),
`research/jackin-desktop-verification-tooling/`.

## Capability index

| File | Owns |
|------|------|
| [providers.md](providers.md) | F3, F5, F6, F7, F11, F12, W5 · provider data core (Rust) |
| [status-bar.md](status-bar.md) | S1–S4, F1 · menu bar items + context menu |
| [popover.md](popover.md) | S5–S10, F4, F8, F10, W1, W3, W4 · Agent Usage preview |
| [usage-window.md](usage-window.md) | S11–S12, F13, W2 · Capsule-parity detail window |
| [architecture.md](architecture.md) | F2, B1, B2, B4 · display-only Swift, native-material fallbacks, limits-only presentation |
| [distribution.md](distribution.md) | F9, B6 · notarized release + cask (headless) |

Quality-bar mapping: B1/B2/B4 → architecture.md; B3 → usage-window.md;
B5 → status-bar.md + popover.md; B6 → distribution.md.

## Must-not registry

| ID | Statement | Reason | Enforced in plans |
|----|-----------|--------|-------------------|
| N1 | Swift MUST NOT contain logic beyond displaying Rust-provided usage information — no computing, rewording, reordering, or deriving of any usage-data label, number, or projection in Swift; static navigation, action, and empty-state copy fixed verbatim by the spec is allowed | item §Must not (Rust owns implementation) | 001, 002, 003, 004, 005, 006, 007, 008, 009 |
| N2 | The popover MUST NOT contain action buttons or link-out rows — sole exceptions: the Refresh footer row (⌘R) and provider-header/account-chip/tab clicks, which are navigation/selection, not actions | item §Must not, D2/D3 | 006, 007, 009 |
| N3 | No surface MUST ever show token unit prices, cost-of-session estimates, spend-over-time charts, trend sparklines, token/spend histories, aggregate-spend donuts, or cost-legend rankings — provider-supplied quota bounds (money caps, credit balances) are the only money allowed | repo hard rule (AGENTS.md usage-surfaces) | 001, 002, 003, 004, 005, 006, 008, 009, 010, 011 |

## Deferrals

| Ledger ID | Reason | Revisit trigger |
|-----------|--------|-----------------|
| F14 (Amp paid-plan/monthly `displayText`) | Amp Free daily is now covered by F12/plan 001, but no public Megawatt/Gigawatt/linked-subscription capture exists (research ch. 11) | Operator-authenticated paid-account `amp usage` capture lands in `research/agent-usage-provider-apis/`; then re-run tailrocks-plan (D15 governs) |

Q2 probe-list items (Spark live `limit_name`, z.ai header two-form probe,
Kimi schema/UA, Grok web protobuf, MiniMax plan-title fields, Claude routines
key) are NOT deferrals: no covered requirement depends on them; the plans
that graze them carry explicit fallbacks/STOP conditions (A1–A3).
