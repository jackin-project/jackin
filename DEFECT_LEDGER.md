# Defect → gate ledger

One row per escaped defect — a bug that reached an operator or the installed
panic hooks (capsule `crates/jackin-usage/src/logging.rs` panic hook / host
`crates/jackin-diagnostics/src/run.rs` `run.error_typed("panic", …)`).

Append-only. Reviewed when choosing the next lint family adoption.

| Date | Symptom | Root cause | Characterization test | Gate/lint/budget adopted (or reason none) |
|------|---------|------------|----------------------|-------------------------------------------|
| 2026-07-09 | Resize coalesce dropped the frame queued behind a coalesced resize | Frame path discarded pending content on coalesce | plan 004 suite | Phase 1 silent-failure / render path discipline (plan 004 landed) |
| 2026-07-09 | OSC 8 hyperlink maps grew without bound | Maps not cleared on terminal reset | plan 007 suite | Plan 007 bound + clear-on-reset |
| 2026-07-09 | DinD left running when post-success finalization failed | Missing cleanup guard after success path | plan 008 suite | Plan 008 finalization cleanup guard |
| 2026-07-14 | Hover/click on earlier OSC 8 cells navigated to a later URI when `id=` (or empty id) was reused | Hyperlink tokens interned by id only; `hyperlink_targets` overwritten on reuse | `osc8_id_reuse_with_new_uri_keeps_earlier_cells`, `osc8_empty_id_updates_do_not_repoint`, `osc8_same_id_same_uri_shares_token` | Tests only — no practical lint beyond the three regressions (plan 014) |

Related: panic hooks already export escaped defects through OTLP; this
ledger turns those escapes into permanent gates rather than one-off fixes.
