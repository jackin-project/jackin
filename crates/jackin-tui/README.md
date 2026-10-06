# jackin-tui

Cross-surface jackin❯ product presentation and transitional Ratatui adapters used by the console, launch, and capsule surfaces.

## Ownership boundary

- Owns jackin❯-specific compositions shared by at least two surfaces, including:
  - `operator_info` — Debug-info / container-info row policy, `ContainerInfoState`, paint via TermRock `DetailTable`/`Panel`, copy/hyperlink hit geometry, and OSC 8 overlay bytes.
  - `tokens` — Ratatui adapters for product-owned brand/domain color tokens (brand pill, menu chips, status accents).
  - `runtime` — `View`, `drive_frame`, and `drive_render`; `SurfaceFocus` / `SurfaceFocusTarget`; `UpdateResult` / `Dirty` / `NoEffect`; plus phase-frozen `Component` / `Subscription` / `SubscriptionPoll`.
- Does not own `ModalOutcome`; `jackin-oppicker` defines the canonical type and publicly re-exports it as `jackin_oppicker::ModalOutcome`.
- The `Component<Vec<u8>, Vec<InputEvent>>` implementation for Capsule `InputParser` has no invocation. `jackin_tui::runtime::Subscription` and `jackin_tui::runtime::SubscriptionPoll` have no current workspace consumer after OpPicker moved to its own `LoadPoll` / `LoadSubscription`. Console defines a separate `tui::runtime::SubscriptionPoll` and `BlockingSubscription`. The facade declarations remain phase-frozen pending their owning phase disposition.
- Does not own neutral widgets, geometry, focus mechanics, hover, scroll mechanics, Theme/Role tables, or terminal lifecycle; those belong to TermRock. `runtime::SurfaceFocus` adapts Capsule focus identities to TermRock's focus graph. Surfaces resolve neutral roles via `termrock::style::DesignSystem::default()` / `Role` directly.
- Does not own a surface application model, input adapter, external subscription, or run loop; those remain under each surface crate's `src/tui/`.
- Does not live in `jackin-core` (L0 domain vocabulary only).

## Architecture tier and allowed dependencies

**L1 presentation.** Workspace deps: `jackin-brand`, `jackin-core`. External: `termrock`, `ratatui`, `crossterm`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root | — |
| [`operator_info.rs`](src/operator_info.rs) · [`operator_info/`](src/operator_info) | Cross-surface Debug-info composition and paint | [`tests.rs`](src/operator_info/tests.rs) |
| [`runtime.rs`](src/runtime.rs) · [`runtime/`](src/runtime) · [`focus.rs`](src/runtime/focus.rs) | Active Capsule/Launch adapters plus phase-frozen facade contracts | [`tests.rs`](src/runtime/tests.rs) |
| [`tokens.rs`](src/tokens.rs) · [`tokens/`](src/tokens) | Product-owned brand/domain Ratatui colors | [`tests.rs`](src/tokens/tests.rs) |

## How to verify

```sh
cargo nextest run -p jackin-tui
cargo clippy -p jackin-tui --all-targets -- -D warnings
```
