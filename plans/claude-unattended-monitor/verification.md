# Verification evidence

## V2 status — verification in progress

The historical V1 results below do not establish V2 completion or real-account readiness. V2 uses schema 2 / broker protocol 6, separate observers and explicit operator-approved policies. Final direct offline Cargo and installed-pair results will be recorded here after passing. The old pair and Claude settings remain untouched.

A tooling incident activated a global `codebook-lsp` installation with crates.io traffic; that installer was stopped. Exact tooling request count is unknown. The overall session is not wholly offline. Provider/credential fixture counters are reported separately; no live provider or Keychain verification is authorized.

## Historical V1 evidence

## Isolation and safety

The targeted CLI subprocess fixture uses a temporary root under `/tmp`, private
`HOME`, `JACKIN_HOME_DIR`, `JACKIN_CONFIG_DIR`, an explicit `--data-dir`, and a
cleared child environment. Its private `PATH` contains executable tripwires for
`op`, `claude`, and `security`; HTTP proxy variables point at a local request
counter. The service is started only through the explicit local-only broker
path and is stopped through `usage service stop`. Those subprocess assertions
recorded zero credential-command invocations and zero HTTP requests.

No actual interactive authentication, native Keychain access, real Claude
process, or real provider request was run. The negative auth-preparation test
uses piped standard streams and verifies rejection before the helper starts.

## Coverage map

| Acceptance area | Evidence |
| --- | --- |
| Passive doctor, service status, and bare cached usage do not start a broker or call credential/provider paths | Offline subprocess test repeats reads before and after explicit service start; checks run-directory state and tripwires |
| Non-TTY auth preparation cannot open a dialog or start the helper | Asserts `interaction_required`, empty stderr, no broker run directory, and zero credential/HTTP trips |
| Unknown optional OAuth does not degrade broker and ingress readiness | Doctor subprocess asserts exit 0, `auth_state=unknown`, and `auth_status_unknown` |
| Official statusline payload normalizes session ID, model object, and rate limits; identical callback is idempotent | Offline subprocess fixture fills exactly 16 KiB, checks normalized fields and unchanged evidence sequence/receipt; fake-clock monitor tests cover freshness |
| Blocked status has exit 2 and JSON issue codes | Offline subprocess checks blocked status, `quota_unknown`, spend issues, and exit class |
| Malformed and oversized statusline inputs fail before broker access | Malformed JSON returns `statusline_invalid`; 16 KiB + 1 byte returns `statusline_too_large`, both exit 3 |
| A due reset keeps polling until a bounded wait timeout | Isolated account starts with a reset 120 seconds in the past and a pre-recorded verified spend receipt; wait returns `reset_due_unverified` plus `wait_timeout` after about one second |
| Reattached watch starts at current state after restart | A formerly runnable goal hits the account-wide 96% guard, broker is orderly stopped/restarted, and the first cursor-zero JSONL event remains blocked with `limit_guard_reached` |
| Spend baseline is explicit and goal-scoped | Receipt after goal creation does not silently repair its baseline; a distinct new goal captures the verified receipt; an account without a receipt stays `budget_unverifiable` |
| Broker lifecycle is orderly | Test fixture stops each monitor, sends service stop, and waits for leader/socket files to disappear |

## Recorded commands and results

The current offline subprocess, CLI, and race regression gates passed:

```text
cargo test --offline --locked -p jackin --test usage_monitor_offline -- --nocapture
# 2 passed; subprocess assertions: 0 credential trips, 0 HTTP requests
cargo test --offline --locked -p jackin --test broker_service_lifecycle
# 2 passed; local-only process start/stop and recovery
cargo test --offline --locked -p jackin --features e2e --test usage_broker_e2e usage_broker_twenty_host_processes_make_one_provider_call -- --exact
# 1 test passed; 20 child clients observed exactly 1 fake provider call
cargo test --offline --locked -p jackin --features e2e --test usage_broker_e2e docker::usage_broker_failure_and_rate_deadline_are_identical_for_all_waiters -- --exact --nocapture
# 1 test passed; one fake provider call, identical waiter deadline, forced pre-deadline retry suppressed
cargo test --offline --locked -p jackin --features e2e --test usage_broker_e2e recovery::usage_broker_killed_owner_recovers_once_without_a_herd -- --exact --nocapture
# 1 test passed; 8 fake clients recovered one abandoned generation without a provider herd
cargo test --offline --locked -p jackin --lib parses_usage
# 10 passed
cargo test --offline --locked -p jackin --lib cli::usage::tests
# 14 passed; includes bounded spend input, doctor, watch, wait, and non-TTY auth guards
cargo test --offline --locked -p jackin --lib cli::usage::statusline
# 11 passed
cargo test --offline --locked -p jackin --lib cli::usage
# 30 passed, repeated five times with default parallelism
cargo test --offline --locked -p jackin --lib cli::usage::store::tests
# 5 passed, repeated five times with default parallelism
cargo test --offline --locked -p jackin --lib cli::usage -- --test-threads=1
# 30 passed
cargo test --offline --locked -p jackin --lib monitor_cli_errors
# 1 passed
cargo test --offline --locked -p jackin --lib doctor_treats_unknown_auth
# 1 passed
cargo test --offline --locked -p jackin --lib watch_fresh_attach
# 1 passed
cargo test --offline --locked -p jackin --lib wait_timeout_is_appended
# 1 passed
```

The database telemetry assertion initially flaked under the default parallel
test runner: another thread's no-subscriber callsite interest could leave the
`DB_CLIENT` callsite cached as disabled, yielding zero of seven expected spans.
The test used `tracing-core 0.1.36`; with only its thread-local test dispatcher
registered, tracing-core's single-dispatch fast path did not reliably rebuild
interest for the parallel test dispatchers. The test now keeps a second live
`tracing::Dispatch::new(tracing_subscriber::registry())` while using the test
subscriber. That makes tracing-core rebuild interest through its registered
dispatchers. This is test-fixture isolation only; no production behavior was
changed. The final fixture passed the store filter five times and the full
usage filter five times with default parallelism, while retaining the 7-span
and privacy assertions.

Offline provider, broker, protocol, and consumer gates:

```text
cargo test --offline --locked -p jackin-usage-provider-core -p jackin-usage-provider-claude -p jackin-usage-discovery -p jackin-usage-credential-resolver -p jackin-usage-credential-snapshots
# 178 passed, 83 filtered out (16 suites)
cargo clippy --offline --locked -p jackin-usage-provider-core -p jackin-usage-provider-claude -p jackin-usage-discovery -p jackin-usage-credential-resolver -p jackin-usage-credential-snapshots --all-targets -- -D warnings
# no issues found
cargo test --offline --locked -p jackin-usage-broker
# 89 passed (3 suites)
cargo clippy --offline --locked -p jackin-usage-broker --all-targets -- -D warnings
# no issues found
cargo test --offline --locked -p jackin-usage
# 68 passed; provider-call inventory now includes callback function items
cargo test --offline --locked -p jackin-usage-broker-publish
# 15 passed
cargo clippy --offline --locked -p jackin-usage-broker-publish --all-targets -- -D warnings
# no issues found
cargo test --offline --locked -p jackin-protocol -p jackin-usage-coordinator
# 173 passed, 1 ignored (6 suites)
cargo test --offline --locked -p jackin-usage-host-runtime
# 6 passed
cargo test --offline --locked -p jackin-usage-ffi
# 6 passed
cargo test --offline --locked -p jackin-runtime-usage-relay
# 19 passed
cargo test --offline --locked -p jackin-capsule --bin jackin-capsule usage_relay
# 13 passed
cargo test --offline --locked -p jackin --lib console::adapter::run
# 7 passed
cargo test --offline --locked -p jackin --bin jackin-usage-broker
# 7 passed; helper tests only, no auth preparation executed
cargo clippy --offline --locked -p jackin-usage-host-runtime --all-targets -- -D warnings
# no issues found
cargo clippy --offline --locked -p jackin-usage-ffi --all-targets -- -D warnings
# no issues found
cargo clippy --offline --locked -p jackin-runtime-usage-relay --all-targets -- -D warnings
# no issues found
cargo clippy --offline --locked -p jackin --all-targets -- -D warnings
# exited successfully (0 errors; RTK reported one non-fatal warning)
cargo fmt --all -- --check
# passed
git diff --check
# passed
```

Broker `tests/case_08.rs::numeric_and_http_date_retry_after_survive_restart_without_early_requests`
asserts one local fake HTTP call before each `Retry-After` deadline and two total
calls after each deadline. The separate fake process retry/deadline E2E asserts
one total call and suppression of a forced pre-deadline retry. The fake 20-process test asserts one call
across all clients. No real provider network request or credential lookup is
part of either test.

The provider, broker, host-runtime, FFI, runtime relay, and publisher all-target
Clippy gates passed. Whole-app Clippy exited successfully; the RTK summary
reported one non-fatal warning in addition to zero errors.

## Accidental Docker suite invocation

The following broad command was run accidentally and is not counted as offline
verification:

```text
cargo test --offline --locked -p jackin --features e2e --test usage_broker_e2e
# 7 passed, 5 failed, 11 filtered
```

The failures were:

- `docker::usage_broker_failure_and_rate_deadline_are_identical_for_all_waiters` — expected retry epoch `1791538279`, actual `1791538280` (one second later).
- `docker::usage_broker_capsule_refresh_is_same_updating_generation_in_desktop` — Docker container-name conflict.
- `docker::usage_broker_docker_capsule_cannot_access_another_account_or_global_tree` — Docker container-name conflict.
- `docker::usage_broker_desktop_and_twenty_docker_capsules_make_one_provider_call` — Docker container-name conflict.
- `recovery::usage_broker_killed_owner_recovers_once_without_a_herd` — a child exited unsuccessfully at `recovery.rs:34`.

The exact fake-only retry/deadline case passed after its provider deadline was
placed 600 seconds ahead, beyond the independently enforced 300-second attempt
floor; the assertion still requires the exact provider deadline. The recovery fixture was updated to seed the private schema-2
projection catalog before starting its fake owner; the exact recovery filter
then passed with eight clients and two total fake provider calls (one abandoned
generation and one recovered generation). Neither exact filtered rerun called
Docker.

Static inspection shows these tests use fake `FileCountingProvider`,
`GateProvider`, or `FailureProvider` implementations and do not invoke normal
provider discovery, Claude auth, or Keychain paths. Docker cases call `docker
info`, `docker run`, container inspection, `docker exec`, and a drop-time
`docker rm --force`. The in-container scripts use the mounted Unix socket to
reach the fake broker; they do not make provider HTTP calls. The `docker run`
requests image `python:3.14-alpine`; three test-output lines reported that the
image was not local and caused registry pull attempts. Docker-daemon HTTP
request count was not instrumented, so the exact number of registry requests is
unknown.

Read-only Docker inspection afterward found no container at attempted ID
`40b5fc88f5affe07a8a36f0306d09e79321b4274a58ee364ce371445db843e3d` and no
container matching `jackin-usage-e2e-88635-0`. No container cleanup or process
termination was performed. Because Docker attempted registry access, the
verification session as a whole was not fully offline.

## Initial implementation binaries and installed CLI proof (historical)

A fresh locked offline build was completed from implementation commit
`29db1854cf694d3a3f9cdae23659ffe5ee5da3cc`:

```text
cargo build --offline --locked -p jackin --bin jackin --bin jackin-usage-broker
# succeeded; one future-incompatibility notice for proc-macro-error2 v2.0.1
```

Cargo resolved these artifacts, and each installed file compares byte-for-byte
with its build output:

| Binary | Build artifact | Installed path | Mode / size | SHA-256 |
| --- | --- | --- | --- | --- |
| `jackin` | `/private/tmp/jackin-claude-monitor/target/debug/jackin` | `/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin` | `0755`, 213,671,080 bytes | `5664344dad6b42f970f925a94dc3543002c98751007cbcc452c9f1427cc7c4a4` |
| `jackin-usage-broker` | `/private/tmp/jackin-claude-monitor/target/debug/jackin-usage-broker` | `/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin-usage-broker` | `0755`, 51,832,760 bytes | `e9d65e4db60b930851923d0630c15921777ddc0b4ee808ddab7ea259e2a6113e` |

The `target` directory resolves into the existing build cache. Canonical built
paths verified with `realpath` are:

- `/Users/donbeave/Library/Caches/mbx/targets/v1/519eebab1636eb6d9b57bb77aa6e6d394b5e4bf8c0d778e9bcdd1148ee1fffff/debug/jackin`
- `/Users/donbeave/Library/Caches/mbx/targets/v1/519eebab1636eb6d9b57bb77aa6e6d394b5e4bf8c0d778e9bcdd1148ee1fffff/debug/jackin-usage-broker`

Both installed binaries report version `0.6.4`. The installed `usage --help`
output lists doctor, service, monitor, status, refresh, watch, wait, statusline,
spend, and auth. `monitor` exposes start/stop; `service` exposes
start/stop/status; watch supports `--timeout-secs`; wait requires
`--until runnable` and supports `--timeout-secs`.

The installed CLI was exercised with cleared child environments, a private
`HOME`, config and data directory, fake `op`/`claude`/`security` executables,
and a local HTTP proxy request counter. The fixture root was
`/tmp/jkin-install-ju5ppyf1` and was removed only after orderly stop. `TMPDIR`
was `/tmp`; `JACKIN_USAGE_BROKER_BIN` was unset, so startup resolved the
installed sibling executable. Observed command results:

| Command | Exit | Evidence |
| --- | ---: | --- |
| `usage service start` | 0 | JSON reports `running: true` |
| `usage doctor --provider claude --unattended` | 0 | JSON reports broker and statusline ingress available, auth remains `unknown` |
| `usage monitor start --provider claude --account installed-proof-account --goal installed-proof-goal` | 2 | JSON status is paused with `quota_unknown`, `missing_reset`, `spend_unavailable`, and `budget_unverifiable` |
| `usage status --monitor monitor-00000001` | 2 | Same blocked monitor status |
| `usage watch --monitor monitor-00000001 --timeout-secs 1` | 0 | One valid JSONL event with the current blocked status |
| `usage wait --monitor monitor-00000001 --until runnable --timeout-secs 1` | 2 | JSON includes `wait_timeout`; broker state is not changed |
| `usage monitor stop --monitor monitor-00000001` | 0 | JSON reports lifecycle `stopped` |
| `usage service stop` | 0 | JSON reports `service_stopped`; run files disappear |

The fake credential-command log and HTTP proxy recorded zero requests. The
monitor JSON is local-only evidence and does not claim provider authentication
or live quota.

A separate fallback check used the default macOS temp root
`/var/folders/4s/k27jn48x1cggdq0ws6vsly4c0000gn/T`, with no broker override.
The full socket path was 113 bytes (the macOS limit is 104), while the derived
`jk-ub-501` alias path was 83 bytes. Alias paths at or above 104 bytes are
rejected fail-closed; that condition was not exercised. Installed service start
and stop both returned 0; the alias and run files disappeared after a
0.2-second cleanup poll. This did not reproduce the earlier `broker_unavailable` startup result.
That first fixture root and its child logs had already been removed, so its
specific cause remains unconfirmed. The later short-path installed smoke and
default-temp alias fallback both passed with the same installed binaries.

## Limits

The offline subprocess suite and installed smoke prove the headless
auth-preparation guard and local-only monitor behavior. They do not exercise
macOS Keychain ACL prompts or authenticate against a real provider. The
accidental Docker image pulls mean this verification session as a whole was not
fully offline. The earlier installed bootstrap error remains unexplained, but
it was not reproduced with either short `/tmp` paths or the default macOS temp
root and its long-socket alias fallback.

## Deterministic acceptance fixtures

All paths below are relative to the repository. The corresponding packages
passed the scoped gates above; simulation is distinct from live verification.

| Required cases | Fixture location and representative test |
| --- | --- |
| Missing/locked/consent-required Keychain, no UI, guard errors | `crates/services/jackin-usage-provider-claude/src/keychain.rs`: `fake_keychain_read_maps_search_outcomes_only_while_ui_is_disabled`, `fake_keychain_query_and_disable_errors_never_run_search`, nested guard tests |
| 401 unchanged/changed caller token, 403, 429, timeout/transport, no retry or CLI fallback | `crates/services/jackin-usage-provider-claude/src/tests/case_04.rs`: `claude_401_does_not_reread_credentials_and_uses_only_later_caller_token`, `claude_http_auth_scope_and_rate_failures_keep_typed_status`, `claude_http_timeout_and_transport_failures_stay_typed_and_do_not_retry` |
| Both Retry-After forms across restart; no hot retries/reset bypass | Broker `src/tests/case_08.rs` numeric/date restart test; `case_10.rs::reset_barrier_does_not_bypass_shared_provider_retry_after`; coordinator attempt-floor/backoff/catalog-reset fixtures |
| Repeated identical payload, missing/stale windows, independent field age | Broker `src/monitor/tests.rs`: `identical_statusline_does_not_refresh_evidence_age_or_decision_sequence`, `missing_and_stale_quota_fields_remain_unknown_independently`; no-op notification tests in `policy_regression_tests.rs` |
| Malformed/oversized input and preservation of old statusline command | App `tests/usage_monitor_offline.rs`; `src/cli/usage/statusline.rs` bounded capture, quote, early-exit, drain and composition fixtures |
| Reordered/overlapping sessions; capacity and replay bounds | Broker `src/monitor/tests.rs::older_overlapping_session_cannot_replace_the_account_reset_watermark`; `session_capacity_tests.rs` |
| >10-minute idle, restart, clock rollback, sleep/wake deadlines, reset waits | Broker `src/monitor/tests.rs` sticky pause/reopen and clock-rollback tests; `watch_restart_tests.rs`; broker `src/tests/case_09.rs` wall-clock wake with no provider work |
| 89→96, 94→100; action sequence idempotency | Broker `src/monitor/tests.rs::quota_threshold_direct_jumps_emit_every_crossed_action` and sequential threshold/idempotency test |
| Five-hour reset with weekly 100%; missing reset; partial-input reset rejection | Broker `src/monitor/policy_regression_tests.rs::weekly_exhaustion_survives_a_confirmed_five_hour_reset`; account barrier test rejects reset-only/model-only/spend-only recovery; independent window tests |
| Missing/USD/SGD/unknown-currency spend; rollover; baseline restart persistence | Broker `src/monitor/spend.rs` receipt/threshold/closing-period tests; `src/monitor/tests.rs` same-goal stop/reopen, missing-baseline, currency and rollover tests |
| Passive consumers and concurrent refresh deduplication | CLI credential/network tripwires; host runtime/FFI/relay/Capsule regressions; fake twenty-process single-flight and eight-client recovery E2Es |

The suspend/wake fixtures test the coordinator wall-clock wake helper, monitor
deadline calculation and local ticking separately; they do not drive the actual
background ticker thread with a simulated suspended operating system. Native
Swift DTO/API consumers were audited statically; no full Desktop UI build or
live provider/Keychain check was performed. The locked tracing-core fast-path
defect remains in the dependency; the test fixture removes its enabling
single-dispatch condition rather than modifying production instrumentation.


## Renewed acceptance audit

The renewed goal reopened proof against the complete objective. The previous
installation is historical evidence. The audit found that a suspended owner's
expired lease could reject renewal, and a failed due tick was remembered as
successful. Independent security review also found transient lease locks could
allow a successor while startup, queued requests or in-flight writes still
belonged to the old broker.

The replacement holds the owner descriptor lock for its lifetime, through
startup, connection workers, ticker and admitted WaitPool operations. Teardown
releases authority last. Both lease open paths use `O_CLOEXEC`, and the fixture
checks `F_GETFD` on new and recovered claims. A live hung owner requires operator
lifecycle intervention; expiry cannot transfer its authority. Process exit
releases the kernel lock. The ticker retries failed due ticks after one second
and does not latch failed persistence as success. Fake clock tests exercise the
actual spawned ticker across an eleven-minute sleep jump, notify Watch, retain
active monitors and assert zero provider calls.

Auth hardening checkpoint: `e5863815`; lifecycle/policy checkpoint: `f669ec7`;
CLI help checkpoint and current source: `6c1709e`. The provider exposes an
unattended-only read and a separate operator preparation API whose internal
TTY check precedes all Keychain work. Fake headless preparation records zero
query, disable, search and restore operations. Independent security review is
Ready. This is source and deterministic fixture evidence, not live Keychain
verification.

The new help regression initially assumed Clap's argument long-about appeared
in variant help; a second expectation also used the wrong description. Both
were corrected to assert the actual generated `-h` and `--help`. Cargo caught
an Arc moved before teardown and test import/qualification errors; all were
fixed before the passing reruns. Clippy's four test duration lints were fixed.

Fresh current gates: broker 101 tests and all-target Clippy; Claude provider 37
and discovery 43 and all-target Clippy; broker auth helper 7; CLI help 1; usage
command tests 14; offline subprocess 2 with zero credential trips and HTTP
requests; lifecycle 2; exact fake killed-owner recovery 1. The recovery filter
uses only local fake processes, never the broad Docker suite. Final app gates,
build and installed proof are pending.

Independent freshness review found no supported provider timestamp for an
unchanged statusline field. A changed five-hour value cannot reattest an
unchanged weekly/reset/model value. This conservative limitation can block
active sessions when those fields age out; the public docs and handoff now
state it explicitly. Existing per-field fixtures assert that behavior.

Rate-limit review is Ready: persisted retry/rate-limit deadlines and the
300-second floor block forced requests. Positive exponential backoff applies
to retryable failures; the latest provider Retry-After/backoff/floor wins.
There is no separate circuit-breaker state beyond these persisted timed
admission guards. Standard independent Claude OAuth remains disabled.


## Replacement build and installed proof

Fresh `cargo build --offline --locked -p jackin --bin jackin --bin
jackin-usage-broker` passed from source commit
`6c1709e4bea1db9ee05d56352f11440db165e4e7`. Its sole warning is the dependency
future-incompatibility notice for `proc-macro-error2 v2.0.1`; app all-target
Clippy also exits zero with this notice. Workspace formatting and diff checks
pass. The installed files were replaced atomically in the separate prefix and
compare byte-for-byte with their build artifacts.

| Binary | Installed path | Mode / bytes | SHA-256 |
| --- | --- | --- | --- |
| jackin | `/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin` | 0755 / 213671080 | `5bee8b128d872fcfcea9476d15b7185c2eb0cbc5dce177bbe723bda9f5067d6f` |
| jackin-usage-broker | `/Users/donbeave/.local/share/jackin-claude-monitor/bin/jackin-usage-broker` | 0755 / 51862296 | `6321536a6908ef0e0a594e7c0460be9527ac257ac0015940de71165d723412b0` |

Canonical build directory remains
`/Users/donbeave/Library/Caches/mbx/targets/v1/519eebab1636eb6d9b57bb77aa6e6d394b5e4bf8c0d778e9bcdd1148ee1fffff/debug`.
Both binaries report 0.6.4. Fresh current gates additionally pass the complete
30-test usage consumer group, the two exact fake single-flight/rate-deadline
E2Es and app all-target Clippy. Twenty clients assert one fake provider call;
forced early waiters assert one call; eight recovery clients assert one
replacement (two total including the killed fixture owner).

The reusable `installed-smoke.py` is ready to exercise this installed pair
without a broker override, including JSONL watch, headless auth rejection and
repeated passive reads. Its result is recorded below after execution.

Installed smoke PASS against the replacement pair, fixture
`/tmp/jkin-installed-smoke-h_rnm8yy` (removed only after orderly stop). No
`JACKIN_USAGE_BROKER_BIN` override was used. Versions/help exit 0; piped auth
prepare returns exit 2 with `interaction_required`; service start/status/stop
exit 0; doctor and two repeats exit 0; monitor start/status and repeated status
reads exit 2 with paused/missing-evidence JSON; explicit JSONL watch exits 0
with one current event; bounded wait exits 2 with `wait_timeout`; monitor stop
exits 0. Credential command tripwires: 0. HTTP proxy requests: 0. The test does
not invoke native Keychain; fake native coverage plus the internal terminal
gate supplies the no-dialog proof. No live provider/auth checks ran in this
renewed audit. The prior broad Docker pull incident remains recorded above.

The original checkout is clean. All replacement implementation lives in the
isolated branch. Push remains blocked by the previously observed SSH-agent
signing refusal; no interactive unlock or auth was attempted. Operator setup,
statusline composition and attested fresh SGD receipts remain prerequisites
for useful runnable evidence; neither fixture proof nor doctor asserts live
quota or credential availability.


## Remote publication update — 2026-10-10

The operator-requested push succeeded without interactive authentication.
All implementation and evidence commits through `3aeb8631` are published on
`origin/claude-unattended-monitor`. The earlier SSH-agent signing refusal is
historical; it no longer blocks publication. Both the isolated worktree and
original checkout were clean at verification.


## Legacy CLI incident diagnosis — 2026-10-10

The reported command used
`/Users/donbeave/Projects/tailrocks/jackin-project/jackin/target/debug/jackin
usage host projection --format json`, not the installed monitor binary. Both
CLI builds print 0.6.4, so version equality is not capability or provenance
proof. Read-only help confirms the repository artifact exposes old host
snapshot/projection commands; current monitor help does not. The original
source's projection handler requests a refresh, while current bare usage only
reads the materialized broker projection. We did not run the old projection
command during this investigation.

| Artifact | SHA-256 |
| --- | --- |
| Original checkout CLI | `d95ccf05a94f0bdf7e6cfaa23ca1041a032994f07960e5230a2c54d5b9dd2f3f` |
| Original checkout broker | `6fe2fce548d3173f58d03076cbe151d7f89580feb066188e330bdb5b03cc531e` |
| Installed CLI | `5bee8b128d872fcfcea9476d15b7185c2eb0cbc5dce177bbe723bda9f5067d6f` |
| Installed broker | `6321536a6908ef0e0a594e7c0460be9527ac257ac0015940de71165d723412b0` |

Original artifacts resolve to build cache key `7931e261...`; current artifacts
resolve to `519eebab...`. The original artifact's exact source commit is not
attested by its hash alone. The original checkout is at `ff9eb01f`; the current
monitor branch is separate. Neither checkout nor Claude settings were changed
as part of diagnosis.

A separate passive installed doctor check on
`/Users/donbeave/.local/share/jackin-claude-monitor/state` returned exit 3 JSON
`broker_unavailable`. That proves the expected service prerequisite is not
reachable at that state; it does not identify why the old broker was unavailable
at the time of the reported command. No production service was started, no
credentials were read and no provider check was requested. The durable fix is
to select the installed capability-checked CLI and matching state, plus explicit
operator local-only setup. Passive reads continue to fail closed when absent.

The original checkout source uses broker protocol 3, while the monitor source
uses protocol 5 despite the shared 0.6.4 display version. A live incompatible
broker is not silently replaced; the explicit separate data directory avoids
that ownership collision. Existing installation tests still expected bare usage
to auto-start a sibling, contradicting the migrated passive contract. Those
fixture tests are now included in the incident verification scope. The first
removed-command test run passed three cases and failed only the assumed parser
message; the observed `unexpected argument` error is now asserted with exit 2.

Incident scoped integration gates pass: `usage_monitor_offline` 4/4 and
`broker_installation` 4/4. The first new parse assertion was corrected against
actual output; app Clippy found two `expect` calls in the new helper, which
are being replaced with typed test errors before the final rerun. Independent
provenance review confirmed the installed path/capability preflight and broker
override rule. The read-only override check distinguishes unset from present,
even empty, without changing Claude's environment.

Installed smoke rerun PASS on the unchanged verified pair, with default sibling
lookup and explicit JSONL watch. Fixture `/tmp/jkin-installed-smoke-_q0csgjv`
was removed after orderly monitor/service stop. Credential trips 0; HTTP proxy
requests 0. Headless preparation returned interaction_required. The fixture
does not start the real documented state or install a statusline into the
running Claude session. No production source behavior or installed bytes were
changed in this incident fix; only regression coverage and handoff were updated.

Final incident rerun is green: offline CLI 4/4; installation 4/4; app
all-target Clippy with `-D warnings`; workspace formatting; diff checks. The
helper's typed-error change passed the rerun. The only Cargo warning is the
previously recorded `proc-macro-error2 v2.0.1` future-incompatibility notice.
These fixes deliberately leave the old checkout/build and live session alone;
they do not guarantee availability without operator setup or prevent an
uninstructed caller from choosing a different binary.


## Final composed-hook verification — 2026-10-10

On source HEAD `37fd349c`, the offline rerun passed 362 tests: broker 101;
Claude provider/discovery 80; protocol 123; coordinator 50 plus one pre-existing
ignored unit test; CLI offline and installation 4 each. Focused attempt-floor,
Retry-After/catalog restart and shared backoff filters also passed. Workspace
formatting and diff checks passed; prior Clippy evidence is reused because
production source remains unchanged.

`verification-installed-hook.json` preserves the actual installed-pair proof.
The proposed statusline command was executed through `/bin/sh -c` over raw
fixture JSON, retaining existing stdout byte-for-byte and leaving settings
byte-identical. The broker received the expected account/session and quota
fields. Fresh synthetic SGD receipts preceded first goal creation, then
monitor start/status were runnable. Identical callbacks retained evidence
sequence/receipt time. At 90%, 91% and 95%, decision sequences 2/3/4 emitted
checkpoint, dispatch reduction and blocking pause respectively. Weekly 100%
remained exhausted with five-hour usage reduced to 5% and an advanced reset.
One-second wait returned wait_timeout. Public stops succeeded; run files,
credential-helper tripwire lines and HTTP proxy requests were all empty.

The first monitor baseline only counts later increases; it cannot reconstruct
spend from the already-started task. The live account, existing real statusline
command and real SGD billing evidence remain unverified. These synthetic
receipts do not establish a real baseline or justify a hard per-goal cap.

This proof also confirmed a prompt error: Wait actions are emitted for future
reset hints even when runnable=true. Only blocked status/Pause stops dispatch;
a routine Wait must not turn a healthy monitor into a pause. The final handoff
is corrected against this source and installed behavior. No product
code or live session change is needed for that documentation correction.

Independent final review of the corrected handoff and public usage docs is
Ready. The synthetic proof is retained for audit, not for live ingestion or
billing. Current production source and installed binaries are unchanged; the
original checkout remains untouched. Real operator binding, reviewed adapter
installation and genuine spend evidence are still needed before live readiness
can be claimed.
