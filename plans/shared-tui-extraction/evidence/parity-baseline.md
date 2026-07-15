# Stage 0 parity baseline

- Frozen donor revision: `33896a504e19ef13adb8692550c1845cb86a9504`.
- Command: `cargo nextest run -p jackin-tui -p jackin-tui-lookbook`.
- Result: 265 tests passed, 0 skipped, in 0.180 seconds (2026-07-15).
- Command: `cargo run -p jackin-tui-lookbook -- target/shared-tui-stage0/freeze-render` followed by `diff -r target/shared-tui-stage0/freeze-render docs/public/tui-lookbook`.
- Result: no differences across all 29 SVG fixtures.
- The exact baseline hashes and byte/pixel sizes are recorded in `render-manifest.json`.
