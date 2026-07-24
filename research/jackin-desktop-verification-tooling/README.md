# jackin❯ Desktop verification tooling

Vetted: 2026-07-24 · Informs: jackin-desktop. Single-chapter topic:
[01-commands.md](01-commands.md) — the proven build/test/verify commands for
the Desktop stack (all cited `file:line` from this repo; every command is the
exact one CI runs green today, method: read of `.github/workflows/ci.yml` job
"Native usage menu bar" + `mise.toml` + `TESTING.md`).

Conclusions: crates gate = `cargo nextest run -p jackin-usage -p
jackin-usage-ffi --locked`; bindings drift gate = `cargo xtask desktop
bindings` + `git diff --exit-code -- native/Generated
native/Sources/JackinUsageBridge/jackin_usage_ffi.swift`; app gate =
`cargo xtask desktop build/verify/test` (mise `desktop-*` wrappers); Swift
suite = `cargo xtask desktop test` (full Xcode: `cd native && swift test -c
release`); native system-image goldens pin current stable `macos-26` instead
of the moving `macos-latest` alias and freeze render inputs; fmt/clippy per
workspace baseline; release path notarizes via `cargo xtask
desktop sign-notarize`.
