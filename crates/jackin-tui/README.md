# jackin-tui

Shared terminal UI primitives and components for jackin❯ surfaces.

The host console, launch cockpit, and `jackin-capsule` multiplexer all consume this crate for terminal UI pieces that must not drift: the phosphor color palette, Ratatui color adapters, tab-cell layout math, agent display names, reusable text-field state, scroll metrics, and canonical Ratatui components.

New repeated TUI patterns belong here before a second surface uses them. Surface crates keep domain state and compose these components; they do not reimplement visual primitives.

The component inventory lives in [COMPONENTS.md](COMPONENTS.md).
