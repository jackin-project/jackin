# Capsule daemon presentation adapters

Physical home for `Multiplexer` presentation impls that the daemon module
loads via `#[path]` from `daemon.rs`:

- compositor
- dialog_mgmt
- input_dispatch
- mouse_input
- pane_layout

They stay logical children of `daemon` (for `impl Multiplexer` / `pub(super)`)
while living under `src/tui/` so TUI code is not mixed into non-presentation
daemon files.
