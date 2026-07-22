# jackin-usage

Usage probes, host runtime, snapshot store, and Capsule/Desktop view shaping.

## Hard rules

- Capsule and usage telemetry emits through the shared `jackin-telemetry` governed facade and direct providers in `jackin-diagnostics`; do not introduce generic macros, raw OpenTelemetry construction, local telemetry files, or another sink.
- Borrow, don't clone, usage views: account materialization serializes from borrowed views/iterators, not full clones.
- **Limits only — never token price or historical usage trend.** This crate feeds Capsule and jackin❯ Desktop. Ship **quota / limit windows only**: remaining or used percent, reset times, pace/status honesty, plan labels, multi-account identity, and provider-supplied **limit** money windows when they are a hard cap (e.g. monthly budget remaining). **Do not** add or expose for product UI:
  - token unit pricing ($/token, $/MTok, model price tables used as product surfaces)
  - session/period **cost** totals framed as “how much you spent on tokens”
  - historical usage **trends** (sparklines, time-series charts, Today / Yesterday / 30 Days spend or token graphs)
  - aggregate-spend donuts, cost legends, ranked spend-by-model charts for the operator UI
  - “Buy credits” or other commercial write actions on usage surfaces
- Internal probe/token-monitor math may still need provider pricing tables for **limit arithmetic** when a provider only reports money against a cap — that is not a product “price for tokens” surface. Never surface price tables, trend series, or cost dashboards to Desktop/Capsule as features.
- Host Desktop / UniFFI path: same ban applies to every field exported for display. Prefer dropping a field over inventing a trend or unit price for the UI.
