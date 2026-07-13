# jackin-tui

Shared terminal UI primitives and components for jackin❯ surfaces.

The host console, launch cockpit, and `jackin-capsule` multiplexer all consume this crate for terminal UI pieces that must not drift: the phosphor color palette, Ratatui color adapters, tab-cell layout math, agent display names, reusable text-field state, scroll metrics, and canonical Ratatui components.

New repeated TUI patterns belong here before a second surface uses them. Surface crates keep domain state and compose these components; they do not reimplement visual primitives.

The `runtime` module owns the shared update-loop contracts (`Subscription`, `UpdateResult`, `Component`, `View`) and the shared frame driver `runtime::drive_frame`, which runs a `View<Model>` render plus a frame-scoped overlay closure inside one `Terminal::draw` call. Surface loops dispatch their per-tick rendering through it (host console today; capsule and launch are the tracked follow-up).

The component inventory lives in [COMPONENTS.md](COMPONENTS.md).
