# Donor performance baseline

Recorded 2026-07-15 on Linux aarch64 (`7.0.11-orbstack`) before any ownership
or rendering change. These are coarse wall-clock baselines, not release
benchmarks; Stage 3 remeasures them and Stage 5 assigns budgets after parity.

## Compilation

Each measurement ran `cargo clean -p jackin-tui -p jackin-tui-lookbook` first.

| Command | Wall | User | System |
|---|---:|---:|---:|
| `cargo build -p jackin-tui -p jackin-tui-lookbook` | 2.276 s | 2.041 s | 0.959 s |
| `cargo build -p jackin-tui -p jackin-tui-lookbook --all-features` | 2.311 s | 2.224 s | 0.881 s |

## Render catalog

`cargo run -p jackin-tui-lookbook -- target/shared-tui-stage0/lookbook` took
0.266 s wall / 0.155 s user / 0.117 s system. The 29 generated SVG files used
485,885 bytes including directory metadata (`du -sb`). The fixture files are
byte-identical to `docs/public/tui-lookbook`.

## Component projections

The donor has no allocation-counting benchmark target. Stage 0 therefore
records its reproducible render-conformance corpus as the component baseline:
tabs; select lists at empty, normal, filtered-empty, and scrolled states; long
labels and Unicode clipping; and single/side-by-side diffs all pass within the
265-test suite and the 29-story catalog. Stage 3 must add an allocation-counting
benchmark for 10/1,000/100,000-row visible-window projections before making a
performance claim; absence of that harness is recorded rather than inventing
allocation results.

## First frame and restoration

Method: run the built terminal browser under a PTY with
`script -q -c "timeout --signal=INT 0.5s env TERM=xterm-256color target/debug/tui-lookbook --terminal" /dev/null`.
It entered the alternate screen and painted within the 0.5-second observation
window (total wall time 0.517 s). The forced-timeout transcript did not contain
alternate-screen, cursor, or mouse restoration sequences; this is a baseline
failure-path observation for the Stage 3 PTY restoration tests, not an accepted
restoration guarantee.
