# Integration docker-e2e

Host macOS 27.0.1 build 26A434, arm64, OrbStack. This is not the pinned macOS 26 gate.

Capsule binary: `target/aarch64-unknown-linux-gnu/release/jackin-capsule`, rebuilt at the integration head before the rerun. `JACKIN_HOME_DIR` must stay unset. Setting it makes `jackin load` use the shared cache and stop on "Remove the cached repo and re-clone".

First full run with a shared home was stopped. The isolated rerun (`--no-fail-fast`) finished exit 100: 24 tests, 17 passed, 7 failed.

Rerun of those 7 after `python:3.14-alpine` was pulled and `jackin-usage-broker` was built, with `JACKIN_USAGE_BROKER_BIN` set: 5 passed, 2 failed.

Still failing:

- `jackin_load_agent_smith_can_reach_its_dind_daemon_with_proxy_env`: Capsule transport ACK reset, then the Java Testcontainers smoke did not pass.
- `usage_broker_killed_owner_recovers_once_without_a_herd`: child processes time out in `wait_until`. Raising the recovery waits from 10s to 30s still failed and was not kept.

Passed on the rerun: `load_options_launch`, the four docker usage-broker capsule tests that had been missing `python:3.14-alpine`.
