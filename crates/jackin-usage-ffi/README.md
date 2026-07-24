# jackin-usage-ffi

Synchronous UniFFI facade over `jackin-usage` host runtime for the native macOS
agent-usage menu bar. Mirrors TableRock’s `tablerock-ffi` split: Rust owns all
truth; Swift is display-only.

**Limits only** for Desktop DTOs — no token unit prices or historical usage
trends.

## Build

```sh
cargo build -p jackin-usage-ffi --release
cargo nextest run -p jackin-usage-ffi
cargo clippy -p jackin-usage-ffi --all-targets -- -D warnings
```

## UniFFI surface (additive desktop v1)

| Method / type | Role |
|---|---|
| `set_format_prefs` | Presentation prefs (`left`/`used`, `countdown`/`exact_clock`) |
| `compact_status_bar_label_for` | Pinned surface compact label |
| `compact_status_bar_strip` | Worst-first multi-surface strip |
| `overview_rows` → `OverviewRowDto` | Popover + Usage-window overview |
| `next_refresh_label` | Next refresh countdown / due |
| `UsageViewDto.estimate_caption` | Honesty caption when estimated |

Existing methods (`snapshot`, `compact_status_bar_label`, …) are unchanged.

`QuotaBucketDto.status_slot` projects the protocol `StatusSlot` as an exact
lowercase string — `"session"`, `"daily"`, `"weekly"`, `"spend"`. `"daily"`
carries Amp Free's daily-allowance glance; Swift renders it and never re-derives it.

## Swift bindings

```sh
cargo xtask desktop bindings
# or: mise run desktop-bindings
```

## XCFramework

```sh
cargo xtask desktop xcframework
# or: mise run desktop-xcframework
```
