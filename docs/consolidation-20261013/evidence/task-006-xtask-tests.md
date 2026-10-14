# TASK-006 xtask tests

Host: arm64, macOS ProductVersion 27.0.1. Commands used `env -u RUSTUP_TOOLCHAIN mise exec -- mbx +1.99.0 nextest run -p jackin-xtask ...` in `/Users/donbeave/Projects/jackin-project/wt-integration-20261013`. This file does not approve TASK-006. Warm CI timing was not measured.

| Filter | Result |
| --- | --- |
| `test(ci_evidence)` | 57 passed, 426 skipped, exit 0, at `274665c0` |
| `test(affected)` first run | 42 passed, 1 failed, exit 100. `scanner_records_cross_boundary_literals` expected `docker/x.sh` and observed `crates/docker/x.sh` because the fixture used three parent segments from `crates/adapters/jackin-image/src`. |
| `test(affected)` after `181ea7c4` | 43 passed, 440 skipped, exit 0 |
| `test(wrapper)` | 0 tests selected. The filter does not name `mise_native_rust_option_routes_cargo_through_mbx`. |
| that MBX routing test, before the assertion edit | FAIL at `assert!(!mise.contains("[tasks."))` because `[tasks.renovate-upstream-sources]` is a real root task. The same assertion exists on policy tip `fba69d60`. |
| that MBX routing test, after the alias-only assertion | 1 passed, exit 0. `mise exec -v -- cargo --version` from the repo root and from `native/` both reported `shim[cargo] WRAPPER command: mbx`. |

`test(wrapper)` remains a zero selection, so it is not a pass of the wrapper gate. The routing test above is the nonzero check that ran.
