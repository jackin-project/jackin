# jackin-tui

Shared terminal UI primitives for jackin' surfaces.

The host console and `jackin-capsule` multiplexer render through different backends: ratatui widgets on the host side and raw ANSI bytes inside the container. This crate keeps the small pieces that must not drift between those renderers: the phosphor color palette, tab-cell layout math, agent display names, and reusable text-field state.

Renderer-specific code does not belong here. Add only backend-neutral primitives that both TUI surfaces can consume.

