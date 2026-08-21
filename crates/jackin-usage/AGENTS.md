# jackin-usage

Usage probes, host runtime, snapshot store, and Capsule/Desktop view shaping.

## Hard rules

- Capsule and usage telemetry emits through the shared `jackin-telemetry` governed facade and direct providers in `jackin-diagnostics`; do not introduce generic macros, raw OpenTelemetry construction, local telemetry files, or another sink.
- Borrow, don't clone, usage views: account materialization serializes from borrowed views/iterators, not full clones.
- **Limits only — never token price or historical usage trend.** This crate feeds Capsule and jackin❯ desktop. Ship **quota / limit windows only**: remaining or used percent, reset times, pace/status honesty, plan labels, multi-account identity, and provider-supplied **limit** money windows when they are a hard cap (e.g. monthly budget remaining). **Do not** add or expose for product UI:
  - token unit pricing ($/token, $/MTok, model price tables used as product surfaces)
  - session/period **cost** totals framed as “how much you spent on tokens”
  - historical usage **trends** (sparklines, time-series charts, Today / Yesterday / 30 Days spend or token graphs)
  - aggregate-spend donuts, cost legends, ranked spend-by-model charts for the operator UI
  - “Buy credits” or other commercial write actions on usage surfaces
- Internal probe/token-monitor math may still need provider pricing tables for **limit arithmetic** when a provider only reports money against a cap — that is not a product “price for tokens” surface. Never surface price tables, trend series, or cost dashboards to Desktop/Capsule as features.
- Host Desktop / boltffi path: same ban applies to every field exported for display. Prefer dropping a field over inventing a trend or unit price for the UI.
- Rust owns all seven usage-domain responsibilities: **account discovery, config/auth resolution, canonical account identity, deduplication, scheduling, shared cache, and single-flight coordination**. Probe-routing slugs and display labels never decide account ownership. Swift receives immutable, non-secret projections and renders them.
- One host-only Rust broker is the sole refresh-generation, provider-call, cache, retry-deadline, and failure-count authority. All active callers join the same canonical-account generation; `force` never queues a second generation behind active work. Coordination/state failure is fail-closed and performs zero provider calls.
- A wait timeout never releases refresh ownership. Ownership remains active until bounded provider work terminates. Terminal state is one atomic, crash-recoverable host-only envelope; provider `Retry-After` is preserved exactly and local backoff is shared when no provider deadline exists.
- Capsules receive only launch-forwarded account capabilities through their scoped `/jackin/run/usage.sock` relay. Never expose the global account catalog, broker socket, or broker state tree in a container. Credentials created only inside a Capsule are out of scope until a separate secure-enrollment design exists.
- The Desktop catalog is exactly Codex/OpenAI, Claude/Anthropic, Amp, Grok/xAI, Z.AI, Kimi, and MiniMax in Rust `DESKTOP_PROVIDER_ORDER`. OpenCode remains a wider host surface, not a Desktop provider.
