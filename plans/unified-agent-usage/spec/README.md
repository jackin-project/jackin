# Unified agent usage specification

These contracts turn the PLANNED roadmap into implementation requirements. Rust owns
identity, freshness, quota semantics, ordering, and display strings. Consumers own
layout only. Every implementation change executes on
`chore/roadmap-unified-agent-usage` in PR #898.

## Capabilities

| Contract | Coverage |
|---|---|
| [Canonical projection](canonical-projection.md) | F1-F4, F8, F17, W1-W6 |
| [Broker and refresh](broker-refresh.md) | F5-F7, W6 |
| [Provider quotas](provider-quotas.md) | F3, F8-F10 |
| [CLI usage](cli-usage.md) | F11-F12, S9-S10, W2-W3 |
| [Console usage](console-usage.md) | F13, S1-S6, W1 |
| [Capsule usage](capsule-usage.md) | F14, S11-S13, W4 |
| [Desktop usage](desktop-usage.md) | F15-F17, S7-S8, W5 |
| [Parity and release](parity-release.md) | F18-F20, B1-B8 |

## Sole must-not registry

| ID | Prohibition | Enforced in plans |
|---|---|---|
| N1 | Duplicate canonical account rows | pending |
| N2 | Token pricing, cost estimates, spend history/trends, charts, or cost rankings | pending |
| N3 | Consumer direct fetch, queued duplicate refresh, or secondary freshness authority | pending |
| N4 | Source ordinal used as durable identity | pending |
| N5 | Quota state authorizes or blocks launch/session work | pending |
| N6 | `agent_uninitialized` is downgraded or conflated with provider failure | pending |
| N7 | Capsule membership comes from anything except resolved launch configuration | pending |
| N8 | Agent/runtime names appear in visible provider labels | pending |
| N9 | Unlike windows aggregate or severity/freshness changes navigation order | pending |
| N10 | Removed selection silently changes to another account | pending |
| N11 | Browser-cookie import/decryption, authenticated WebView, or scraping | pending |
| N12 | `Not started` is inferred, or weak/stale/simple-CLI run-out is shown | pending |
| N13 | Human CLI uses bars, cards, animation, pace, raw mode, or interactive chrome | pending |
| N14 | Prototype fixture/store/scenario/harness code is copied into production | pending |

## Deferred

- X1 provider catalog expansion waits for supported API and trusted credentials.
- X2 provider incident badges belong to operational health.
- X3 credit-expiry timelines/notifications wait for reliable expiry data and policy.
- X4 Codex code-review quota waits for a supported non-browser source.
