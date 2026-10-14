# TASK-003 DinD proxy e2e

Tree: `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` at `7a05f3aebda4a73e183b665702f00bb7a38ce36f` on `integration/branch-consolidation-20261013`. Read-only. No cargo, mbx, nextest, or docker.

Verdict: environment limit. The test does not mis-assert. No smallest product edit is supported. Edit list is empty. Result: BLOCKED. Not approved.

## What failed

`jackin_load_agent_smith_can_reach_its_dind_daemon_with_proxy_env` (`crates/apps/jackin/tests/dind_e2e.rs:108`) failed at the Java smoke check (`dind_e2e.rs:226-229`). That line runs only after the report file exists and already contains `REPORT_BEGIN`, `REPORT_END`, the DinD Docker env, both `NO_PROXY` casings with the DinD hostname, a child container id, and `DIND_DOCKER_RUN_STATE=running` (`dind_e2e.rs:182-225`). Those earlier checks passed. The proxy merge this test guards (`dind_e2e.rs:1-4`, role env at `tests/dind_e2e/fixtures.rs:99-102`, merge at `crates/services/jackin-runtime/src/runtime/launch/launch_runtime.rs:628-681`) is not the failing predicate.

`TESTCONTAINERS_SMOKE=ok` is appended only after Maven (`fixtures.rs:427-490`). `REPORT_END` and the docker-run lines are written first (`fixtures.rs:419-432`). The PTY runner treats a zero exit before that sentinel as a return, not a harness panic (`tests/dind_e2e/pty_runner.rs:195-214`). A non-zero exit would have panicked earlier with `script exited before sentinel file appeared` (`pty_runner.rs:208-212`). Reaching the smoke assertion means `jackin` exited 0 without the smoke line.

The outro `You were in the Construct for 15 seconds` is the normal last-container caption (`crates/ui/jackin-launch/src/animation.rs:324-328`). Under 60 seconds it prints as `{n} seconds` (`animation.rs:507-526`). Elapsed time comes from the exit claim (`crates/services/jackin-runtime-launch-exit-outro/src/exit_outro.rs:63-79`). Fifteen seconds inside the Construct is the agent session ending, not a 15-second product timeout. A cold Maven plus Testcontainers run does not finish in that window, so the post-`REPORT_END` smoke line was never written. The assertion reports that correctly.

## Capsule ACK reset

The stderr chain is produced only by the in-container client:

- `jackin-capsule protocol-check` adds `Capsule transport protocol check failed` (`crates/apps/jackin-capsule/src/client/attach.rs:52-74`).
- That wraps `client_handshake_async`, which fails at `reading Capsule transport ACK` (`crates/core/jackin-protocol/src/capsule_transport.rs:175-193`).
- `Connection reset by peer (os error 104)` is Linux `ECONNRESET`. A macOS host syscall would report 54. The failing process is the Linux `jackin-capsule` the test required (`tests/dind_e2e/common.rs:39-78`), dialing `/jackin/run/jackin.sock` (`crates/core/jackin-core/src/container_paths.rs:75`, `crates/apps/jackin-capsule/src/socket.rs:26`).

Connect and the preface write succeeded. A major mismatch returns a NAK or wrong-ACK error (`capsule_transport.rs:72-90`). A clean close returns `Capsule transport ACK ended before all bytes arrived` (`capsule_transport.rs:149-157`). `ECONNRESET` means the peer aborted the socket while the ACK was being read (`capsule_transport.rs:190-193`).

That check is the Docker-exec preflight, not the proxy env:

- Readiness retries `docker exec … protocol-check` for up to 30s when the host socket does not negotiate (`crates/services/jackin-runtime-attach-capsule-ready/src/capsule_ready.rs:65-89` and `:92-113`).
- Snapshot and attach-proxy require the same preflight (`crates/services/jackin-runtime-snapshot/src/snapshot.rs:267-278` and `:363-367`; `crates/services/jackin-runtime-host-attach/src/host_attach.rs:150-158`).
- macOS selects that proxy when the bind-mounted socket is missing, too long for `sun_path`, or fails negotiation (`crates/services/jackin-runtime-attach-transport/src/transport.rs:58-95`). The code already says a same-kernel host can use the bind mount and Docker Desktop for macOS cannot (`snapshot.rs:83-86`).

The socket file is the host directory bind-mounted onto `/jackin/run` (`launch_runtime.rs:853-858` and `:983-984`; `crates/services/jackin-runtime-usage-relay/src/usage_relay.rs:33-43`). On this machine the engine is OrbStack’s Linux VM (`crates/services/jackin-runtime-docker-profile/src/docker_profile/probe.rs:38` and `:50-54`), so that mount is not a same-kernel socket. The in-container client still dials that mounted inode. An ACK read reset is that socket aborting, not `NO_PROXY` routing.

## Host inputs that are not the defect

- Host is macOS 27.0.1 build 26A434 with OrbStack. The pinned OrbStack lane is Apple Silicon macOS 26 (`TESTING.md:51-55`). This host is outside that gate.
- `JACKIN_CAPSULE_BIN` at `target/aarch64-unknown-linux-gnu/release/jackin-capsule` is the ELF the harness demands (`common.rs:71-78`).
- `JACKIN_HOME_DIR` unset is the harness default. The PTY sets `HOME` to the temp home (`pty_runner.rs:95`) and does not set `JACKIN_HOME_DIR`. Paths then use `$HOME/.jackin` (`crates/core/jackin-core/src/paths.rs:62-84`).

## Edit list

Empty. Result: BLOCKED.

No one-line product or test change fixes an ACK reset on the OrbStack bind-mounted socket, and weakening the smoke assertion would hide a session that exited before Maven finished. This run does not show the proxy-env merge failing.
