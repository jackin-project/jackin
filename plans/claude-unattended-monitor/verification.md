# Verification evidence

## Bounded auth diagnostic follow-up on main

Branch `fix/auth-malformed-diagnostic-dco` is based on remote main
`3f3ddd7c48284b648b083305f969ee3e709b74b7`. DCO-signed source checkpoints
`eb8f6b58c5fda32f437f7e9434edabaa1d47f0f8` and
`31dc0574a8d0a052e115fa4031740eef31664af5` are on the delivery branch. The
latest branch has source tree `ea7246252602ed897f0027ff448050219f191533`,
identical to the tree used for the installed v4 pair. The bounded diagnostic
is attached to `auth_malformed` only after the existing all-stream TTY gate;
it uses the payload from the existing single bootstrap read and stops JSON
classification above 65,536 bytes. It reports fixed JSON kinds, recognized
alias presence/duplicates, access-token string/nonempty facts, and total
payload/limit bytes. It includes no values, snippets, token length, identifiers,
or unknown keys. Existing typed parsing remains the acceptance authority.

MBX source check and scoped formatting passed. Focused offline tests passed:

| Check | Result |
| --- | --- |
| Diagnostic fixtures | 9 passed, 0 failed |
| Oversize bootstrap validation | 1 passed, 0 failed |
| Malformed foreground bootstrap | 1 passed; one read, zero guard/ready calls |
| Broker malformed JSON | 1 passed, 0 failed |
| Broker foreground TTY/error mapping | 3 passed, 0 failed |
| Claude credential lease regressions | 15 passed, 0 failed |

The installed v4 debug pair was built with MBX 1.22.0/Rust 1.97.1 and reports
version 0.6.4. The installed non-TTY command exited 2 with
`interaction_required`, without a diagnostic and without calling the broker.
Independent read-only review confirmed the parser and install checks; installed
v2/v3 hashes were unchanged. Exact command, artifact hashes, and command output
are in [v4-installation.json](v4-installation.json). This verifies only
synthetic fixture behavior and the non-TTY interaction gate. No Keychain item,
credential, account identity/status, live bootstrap, or provider endpoint was
inspected; actual account status remains unknown.

## Current main-port verification snapshot

The canonical branch is `feat/claude-usage-monitor-main`, based on main
`868ce535`. Latest signed/pushed source checkpoint:
`8abfa235cce150d5382d99a5679afab49e098525`. The final seven-gate MBX rerun
and scoped regression matrix passed. Its offline MBX pair build passed in
1.29 seconds. CLI SHA-256 is
`b57e60f2f13b026ae2ec47034b61ecddc69644b230157e621c5ddb5cf447007d`; broker
SHA-256 is
`30b6f3b1f0777dbe9181851f83fbc2efb3f0356d290e1896bf2ccaf434fbf550`. The
installed fixture smoke passed with exit 0, selected the sibling broker,
recorded one JSONL watch event, recorded zero proxy requests and
credential-command trips, and left the fixture state empty with no open
binaries. Manifest SHA-256 is
`64a0a101224ad5c38f4292a59c5191a44071fc54eab3cc95e8a57a13bc454601`. Source-head
CI run [38044592706](https://github.com/jackin-project/jackin/actions/runs/38044592706)
passed for this exact commit: Required, all Rust matrix jobs, and Actionlint
succeeded; the baseline publication job was skipped by workflow rules. The
documentation/evidence PR head `56181d4d` passed run
[38045612792](https://github.com/jackin-project/jackin/actions/runs/38045612792):
Required, all Rust matrix jobs, Actionlint, and DCO passed; baseline publication
was skipped by workflow rules. The documentation corrections in this commit
create a new PR head and require their own exact-head checks. CI run
`38042237548` and the 45a installed pair are historical for this source.

P1's private one-way-migration rejection has fresh proof; follow-up reply
`4237303703` is posted and the thread resolved. P2 was accepted: the FFI rank
fix resets raw fields before capping, and its focused suite passed 11 tests.
The fix is included in 8abfa; reply `4237302394` links the fixing commit and
the thread is resolved. General comment `6096529404` has a linked disposition.
A V1 end-to-end migration regression passed 1 test. The final seven-gate MBX
rerun and its focused regression checks passed:

| Current frozen-patch focused scope | Result | Evidence |
| --- | --- | --- |
| Seven-package scoped formatting | Passed | `/private/tmp/jackin-mbx-v4-final-freeze11-fmt.log` |
| Seven-package source check | Passed | `/private/tmp/jackin-mbx-v4-final-freeze11-check.log` |
| Seven-package strict Clippy, all targets | Passed | `/private/tmp/jackin-mbx-v4-final-freeze10-clippy.log` |
| S6 rotation | 1 passed | `/private/tmp/jackin-mbx-v4-final-freeze4-s6-rotation.log` |
| S6/full-scope verification | 7 passed | `/private/tmp/jackin-mbx-v4-final-freeze4-scope-verification.log` |
| FFI rank fix | 11 passed | `/private/tmp/jackin-mbx-v4-final-freeze5-ffi.log` |
| Docs Commands | 5 passed | `/private/tmp/jackin-mbx-v4-final-freeze5-docs-commands.log` |
| V1 storage/migration | 1 passed | `/private/tmp/jackin-mbx-v4-final-freeze5-v1-migration.log` |
| Host broker | 184 passed | `/private/tmp/jackin-mbx-v4-final-freeze5-host-broker.log` |
| Capsule diagnostic dedup | 1 passed | `/private/tmp/jackin-mbx-v4-final-freeze10-capsule-dedup1.log` |
| Capsule proxy scope | 11 passed, 1 filtered | `/private/tmp/jackin-mbx-v4-final-freeze10-capsule-proxy.log` |
| MBX offline pair build | Passed, exit 0, 1.29 seconds; CLI/broker hashes unchanged | `/private/tmp/jackin-mbx-build-8abfa235.log` |
| Installed fixture smoke | Passed, exit 0; exact sibling selected, one JSONL watch event, zero proxy/credential-command trips, fixture cleaned, no open binaries | [v3-installed-smoke.log](v3-installed-smoke.log) |
| Installed manifest | SHA-256 `64a0a101224ad5c38f4292a59c5191a44071fc54eab3cc95e8a57a13bc454601` | [v3-installation.json](v3-installation.json), [v3-checks.json](v3-checks.json) |
| Installed handoff | Matches repository handoff; SHA-256 `3ca3855cb508ccc6785d5d28aee382f878f490e91021a03afbff02c4df935c43` | [claude-code-handoff.md](claude-code-handoff.md) |

The capsule fake socket test used a canonical parent directory with mode 0700,
a socket with mode 0600, and a bounded accept deadline. Checkpoint `8abfa235`
is signed and pushed, its fixture install smoke passed, and source-head CI run
[38044592706](https://github.com/jackin-project/jackin/actions/runs/38044592706)
completed successfully. Native Security
Framework calls were not instrumented; this smoke does not establish live
account readiness or successful foreground authentication. The 56181d4 PR
head's exact checks passed; final checks for the documentation corrections
remain required before landing. The 45a installed pair is detailed below; it is
historical and does not validate this source.

Current Cargo verification must use MBX 1.22.0 with offline/locked flags.
Earlier checks used the checked-in Mise-to-MBX Cargo wrapper. Current isolated
checks use `MISE_AUTO_INSTALL=false mise exec -- mbx ...`; commands that can
launch nested Cargo also inject wrapper-first PATH after Mise, as documented in
the current task queue. The direct-Cargo blocks in the historical sections
below preserve old transcripts and are not current run instructions. The local
wrapper and prior build output verify MBX 1.22.0; PR #1120 is not required to
enable it and remains draft with failing required checks.

| Checkpoint 8288 selected source scope | Result | Evidence |
| --- | --- | --- |
| Production five-package compile | Passed | `/private/tmp/jackin-mbx-v4-five-package-check-final.log` |
| Final seven-package scoped check | Passed after test-scanner fix | `/private/tmp/jackin-mbx-v4-scoped-check-final-retry.log` |
| Seven-package scoped formatting | Passed | `/private/tmp/jackin-mbx-v4-fmt-final4.log` |
| Protocol | 135 passed | `/private/tmp/jackin-mbx-v4-protocol-full.log` |
| Projection | 10 passed | `/private/tmp/jackin-mbx-v4-usage-projection-tests-retry.log` |
| Discovery | 41 passed | `/private/tmp/jackin-mbx-v4-usage-discovery-tests.log` |
| Claude provider | 25 passed | `/private/tmp/jackin-mbx-v4-usage-claude.log` |
| Coordinator | 50 passed | `/private/tmp/jackin-mbx-v4-usage-coordinator-tests.log` |
| Coordinator state | 11 passed | `/private/tmp/jackin-mbx-v4-usage-coordinator-state-tests.log` |
| Host broker | 183 passed, 0 failed, 511 filtered | `/private/tmp/jackin-mbx-v4-usage-host-broker-retry2.log` |
| Host inventory | 13 passed | `/private/tmp/jackin-mbx-v4-usage-host-tests-final.log` |
| Runtime usage relay | 21 passed | `/private/tmp/jackin-mbx-v4-runtime-usage-relay-final.log` |
| CLI usage | 43 passed | `/private/tmp/jackin-mbx-v4-jackin-cli-usage-lib.log` |
| App | 29 passed | `/private/tmp/jackin-mbx-v4-jackin-app-tests.log` |
| Console | 61 passed | `/private/tmp/jackin-mbx-v4-console-usage-tests.log` |
| FFI bridge | 9 passed | `/private/tmp/jackin-mbx-v4-ffi-bridge.log` |
| Capsule | 11 passed | `/private/tmp/jackin-mbx-v4-capsule-usage-relay-proxy.log` |
| Offline bootstrap integration | 2 passed | `/private/tmp/jackin-mbx-v4-integration-usage-bootstrap-offline.log` |
| Offline monitor integration | 5 passed | `/private/tmp/jackin-mbx-v4-integration-usage-monitor-offline.log` |
| Broker installation integration tests | 5 passed | `/private/tmp/jackin-mbx-v4-integration-broker-installation.log` |
| Broker service lifecycle integration tests | 2 passed | `/private/tmp/jackin-mbx-v4-integration-broker-service-lifecycle.log` |
| Contract baseline | 10 passed | `/private/tmp/jackin-mbx-v4-usage-contract-baseline-final.log` |
| Docs typecheck, tests, and static build | 18 tests, 30 assertions; 1,293 routes | `/private/tmp/jackin-docs-wire8-final-build-rerun.log` |
| Docs repository links | Passed | `/private/tmp/jackin-mbx-v4-docs-repo-links-final.log` |
| Roadmap metadata scan | 18 pages passed | `/private/tmp/jackin-mbx-v4-roadmap-audit.log` |
| Research metadata scan | 63 pages passed | `/private/tmp/jackin-mbx-v4-research-check.log` |

These are independent scopes and overlap; do not sum their counts. They record
passing selected source checks at checkpoint 8288, not final acceptance. The
broker run covers the lifetime leader lock remaining held on its exact inode
across sleep, `O_CLOEXEC` descriptors, dead-owner takeover, wake/renewal, and
owned-path cleanup. Its passing cases include
`expired_lease_is_reclaimed_after_dead_owner_releases_lifetime_lock`,
`live_lease_owner_blocks_expired_takeover_and_can_renew_after_waking`,
`stale_lease_descriptor_cannot_renew_or_clean_successor_files`, and
`spawned_ticker_reconciles_one_sleep_jump_and_wakes_monitor_watch`. The same
183-test run passes
`interaction_diagnostics_survive_new_catalog_incremental_publish_and_revocation`,
`clean_catalog_scan_clears_catalog_diagnostics_and_keeps_unrelated_provider_issues`,
and `cleared_diagnostic_removes_empty_provider_row`.

The unused public `HostUsageRuntime` and its references were removed; the
active `HostUsageProjectionRuntime` remains. This was checked against current
consumers and the seven-package scoped check. The following records the
historical 8288/14c gate state: retry/cooldown and security lifecycle work was
still open at 8288. Linux CI run
[38035268298](https://github.com/jackin-project/jackin/actions/runs/38035268298)
for that exact head failed five Rust jobs plus Required; failures included
excessive-nesting Clippy and E0308/E0599 compile errors. Later CI run
`38042237548` on head 14c failed three
Rust jobs (`jackin`, `jackin-runtime`, and `jackin-capsule`), and its Required
aggregate failed. The cfg fix and subsequent changes are included in source
checkpoint 8abfa; its final MBX gates and affected suites passed, and source-head
CI run [38044592706](https://github.com/jackin-project/jackin/actions/runs/38044592706)
passed. These historical failures do not describe 8abfa or the current PR head.

## Historical bounded source verification: checkpoint 22ac3581

These scoped results are newer than the checkpoint 8288 matrix above. They
verify signed, pushed source checkpoint
`22ac3581a70310b9792f03d71168ee8bdd799591`; they are historical evidence and
do not cover the cfg fix committed in 45a33093. The installed pair for 22ac3581
passed its fixture smoke below.
CI run [38040399067](https://github.com/jackin-project/jackin/actions/runs/38040399067)
for this 22ac head had five failed Rust jobs, all reporting the same three
E0004 match-site diagnostics; its Required aggregate check failed as well.
Each suite is an independent scope and overlaps other suites; do not sum test
counts.

| Checkpoint 22ac bounded scope | Result | Evidence |
| --- | --- | --- |
| Seven-package scoped formatting | Passed | `/private/tmp/jackin-mbx-v4-final-fmt-seven.log` |
| Seven-package source check | Passed | `/private/tmp/jackin-mbx-v4-final-source-check.log` |
| Seven-package strict Clippy, all targets | Passed, exit 0 | `/private/tmp/jackin-mbx-v4-final-clippy-seven.log` |
| Coordinator | 57 passed | `/private/tmp/jackin-mbx-v4-final-coordinator-tests2.log` |
| Coordinator state | 11 passed | `/private/tmp/jackin-mbx-v4-final-coordinator-state-tests.log` |
| Host broker | 183 passed, 0 failed, 522 filtered | `/private/tmp/jackin-mbx-v4-final-host-broker.log` |
| Host inventory | 13 passed | `/private/tmp/jackin-mbx-v4-final-host-tests.log` |
| Projection | 10 passed | `/private/tmp/jackin-mbx-v4-usage-projection-tests-retry.log` |
| Discovery | 41 passed | `/private/tmp/jackin-mbx-v4-usage-discovery-tests.log` |
| Runtime usage relay | 21 passed | `/private/tmp/jackin-mbx-v4-final-runtime-tests.log` |
| CLI usage | 43 passed | `/private/tmp/jackin-mbx-v4-final-cli-usage-tests.log` |
| FFI bridge | 9 passed | `/private/tmp/jackin-mbx-v4-final-ffi-tests.log` |
| Console | 61 passed | `/private/tmp/jackin-mbx-v4-final-console-tests.log` |
| Capsule | 11 passed, 1 filtered | `/private/tmp/jackin-mbx-v4-final-capsule-tests.log` |
| Offline bootstrap integration | 2 passed | `/private/tmp/jackin-mbx-v4-final-integration-bootstrap.log` |
| Offline monitor integration | 5 passed | `/private/tmp/jackin-mbx-v4-final-integration-monitor.log` |
| App | 29 passed | `/private/tmp/jackin-mbx-v4-final-app-tests.log` |
| Broker installation integration | 5 passed | `/private/tmp/jackin-mbx-v4-final-integration-broker-installation.log` |
| Broker service lifecycle integration | 2 passed | `/private/tmp/jackin-mbx-v4-final-integration-broker-lifecycle.log` |
| Claude provider and lease lifecycle | 28 passed | `/private/tmp/jackin-mbx-v4-final-claude-tests.log` |
| Contract baseline | 10 passed | `/private/tmp/jackin-mbx-v4-final-contract-baseline2.log` |

## Historical source checks for checkpoint 45a33093

These results cover signed, pushed source checkpoint
`45a33093df524c03440ed524e71375953ee6834b`. They are historical for the
later signed/pushed source checkpoint `8abfa235` and its current verification.
Local macOS MBX results; each affected test suite is an independent scope.

| Scope | Result | Evidence |
| --- | --- | --- |
| Seven-package formatting | Passed | `/private/tmp/jackin-mbx-v4-linux-payload-fmt.log` |
| Seven-package source check | Passed | `/private/tmp/jackin-mbx-v4-linux-payload-check.log` |
| Seven-package strict Clippy, all targets | Passed | `/private/tmp/jackin-mbx-v4-linux-payload-clippy.log` |
| Claude provider and lease lifecycle | 28 passed | `/private/tmp/jackin-mbx-v4-linux-payload-claude.log` |
| Host broker | 183 passed, 0 failed, 522 filtered | `/private/tmp/jackin-mbx-v4-linux-payload-broker.log` |
| Discovery | 41 passed | `/private/tmp/jackin-mbx-v4-linux-payload-discovery.log` |
| MBX offline build | Passed, exit 0, 15.93 seconds | `/private/tmp/jackin-mbx-build-45a33093.log` |
| Installed fixture smoke | Passed | [v3-installed-smoke-checkpoint-45a.log](v3-installed-smoke-checkpoint-45a.log) |
| Intermediate Linux CI poll for 45a | 22 passed, 6 pending, 0 failed | Later run `38042237548` failed three Rust jobs on head 14c; source-head run `38044592706` then passed for 8abfa; see current snapshot above |

The installed 22ac and 45a pairs below are historical relative to current
source checkpoint 8abfa.

The retry/cooldown suites cover restart floor recovery, projection cadence,
and active cooldown tombstones across loaded, lazy-loaded, and
pending-before-purge paths. Independent review confirmed the corrected
tombstone assertion: a non-pending-reset tombstone clears
`started_at_epoch` while retaining `provider_invoked=1000` and
`RetryAfter=5000`; restart at 1101 with a fresh 300-second floor chooses
`max(1300, 1401)`.

The current Claude lifecycle tests and independent review confirm that each
initial HTTP request, credential reread, and retry needs a generation permit;
deactivation prevents new permits, previously admitted I/O may finish, and
cache updates are serialized against the exact generation. An earlier
pre-permit race concern was withdrawn after review of the current liveness
mutex. Static review and fake fixtures do not prove native Keychain runtime
behavior.

The corrected contract baseline passed 10/10 with the exact 33-row
provider-call inventory. The earlier 9/10 run failed because its inventory
expected the pre-rename wrapper route; it is superseded by the passing result
in the table. Host inventory, runtime, CLI, app, FFI, console, capsule,
bootstrap/monitor, broker-lifecycle, and broker-installation results complete
the consumer/integration matrix for historical checkpoint 22ac3581. Scoped
formatting and strict Clippy across all targets passed for that checkpoint,
and its MBX installed pair passed the isolated fixture smoke. The 45a source
checks, build, and installed smoke are recorded as historical above. The
post-14c production changes were finalized in checkpoint 8abfa. Its final
seven-gate MBX verification and installed fixture smoke are recorded in the
current snapshot at the top of this document, and source-head CI run
`38044592706` passed. CI run `38042237548` is the historical failed run on
head 14c; it does not describe checkpoint 8abfa or the current PR head.

## Historical installed fixture checkpoint 45a33093

The installed pair for signed, pushed checkpoint
`45a33093df524c03440ed524e71375953ee6834b` was built through MBX 1.22.0 with
offline, locked flags. Build time was 15.93 seconds; build log:
`/private/tmp/jackin-mbx-build-45a33093.log`.

The isolated fixture smoke passed; its checkpoint-specific transcript is
[v3-installed-smoke-checkpoint-45a.log](v3-installed-smoke-checkpoint-45a.log).
It verified the installed
CLI SHA-256
`b57e60f2f13b026ae2ec47034b61ecddc69644b230157e621c5ddb5cf447007d` and
broker SHA-256
`30b6f3b1f0777dbe9181851f83fbc2efb3f0356d290e1896bf2ccaf434fbf550`.
The provenance manifest SHA-256 is
`8c3a3ba5b4d80ed6a11e1c0c6dc1d8b611a3e7a54f926c8886b5d7ee02d20c02`,
recorded with the binary hashes in that checkpoint-specific transcript. The
smoke selected the installed sibling
broker with `JACKIN_USAGE_BROKER_BIN` unset, recorded zero HTTP-proxy requests
and zero credential-command trips, and left the fixture state directory
empty. After orderly stop, no broker process or open files remained.

Native Security Framework calls were not instrumented; successful foreground
authentication, live account/provider readiness, and dispatch approval were
not exercised. The installed fixture proves only isolated smoke behavior.

## Installed fixture checkpoint 22ac3581

The installed pair for signed, pushed checkpoint
`22ac3581a70310b9792f03d71168ee8bdd799591` through MBX 1.22.0 with offline,
locked flags. Build time was 37.02 seconds and reported network transfer was
0 bytes. Build log: `/private/tmp/jackin-mbx-build-22ac3581.log`.

The installed fixture smoke passed:
`/private/tmp/jackin-v3-installed-smoke-22ac3581.log`. It verified the exact
installed CLI hash `7f7d99960351b1da8fe3da9cd6ac35d577261bcdc42325a40d5e2d146d2d8225`
and broker hash `b74c9cae283f454fcffe6b5c13e6fd498017fec3d808c97a8aeba72ea7f5bcc0`.
The exact binary hashes and provenance manifest are recorded in the
[checkpoint-specific smoke transcript](v3-installed-smoke-checkpoint-22ac.log);
the current JSON artifacts describe 8abfa. It selected the installed sibling broker
with `JACKIN_USAGE_BROKER_BIN` unset, and recorded zero HTTP-proxy requests and
zero credential-command executions. The fixture state was empty afterward;
the service process and its open files were absent after orderly stop. The
provenance manifest SHA-256 is
`3982ae357883ec5e67ab586790dc1b195024290e4731e9f963dbbb8555381738`.
The frozen transcript for this historical pair is
[v3-installed-smoke-checkpoint-22ac.log](v3-installed-smoke-checkpoint-22ac.log).

Native Security Framework calls were not instrumented; foreground auth
success, live account/provider readiness, and dispatch approval were not
tested. Real-account setup remains operator-controlled and unverified.
Checkpoint 8288's earlier fixture transcript is preserved in
`v3-installed-smoke-checkpoint-8288.log` and does not describe the 22ac pair.

Passive usage `status`, `watch`, `wait`, `doctor`, and `current` reads remain
credential- and network-free in the selected offline scopes. Standalone
interactive console startup is an active consumer: it can request broker
refresh for known accounts, so the passive guarantee does not apply to every
CLI command. Claude provider requests remain foreground-only and require the
capability, approval, and persisted 300-second rate floor. No live account or
native Keychain runtime verification is claimed.

## Checkpoint 8288 installed fixture evidence

The MBX 1.22.0 / Rust 1.97.1 offline, locked binary build and isolated installed
smoke passed for source commit
`8288ef4a4e174624e353f8748304766ce98e5822`. The directory retains its historical
`jackin-claude-monitor-v3` name, but this installed pair uses broker wire v8,
monitor schema v4, projection schema v3 in source, and statusline input v2.
The exact build command, installed paths, and binary hashes are in
`/private/tmp/jackin-v3-provenance-8288ef4a.json`; the installed transcript is
`/private/tmp/jackin-v3-installed-smoke-8288ef4a.log`.
The smoke verified the installed sibling broker with
`JACKIN_USAGE_BROKER_BIN` unset, wire v8 and monitor schema v4, zero
HTTP-proxy requests, zero credential-command executions, and orderly fixture
cleanup. Native Security Framework calls were not instrumented; foreground
auth success and dispatch approval were not exercised. The smoke does not
assert persistence of a projection envelope. This section is historical
evidence for source checkpoint 8288. The separate installed pair for checkpoint
22ac3581 is recorded above as historical evidence. The 45a33093 build and
installed fixture smoke, and CI run `38042237548` on head 14c, are historical.
The final 8abfa build, installed smoke, seven-gate MBX verification, and passing
source-head CI run `38044592706` are recorded in the current snapshot at the top
of this document. PR documentation/evidence head `56181d4d` passed exact-head
run `38045612792`; this documentation correction creates a new head whose
checks are still required before landing.

Previously recorded pre-v8 scopes: protocol 133; broker 173; Claude
fake-auth/provider 25; CLI usage 43; app 29; coordinator 54. These historical
counts and results do not validate the current wire-v8 candidate. The
Docker/e2e-feature executable ran zero tests and is not e2e proof. Prior logs
and source provenance are recorded in `v3-checks.json` and
`v3-installation.json`.

Before checkpoint 8288, the pair built through MBX from
`1a45196dbe24d439e596c14e22fbda59799e7b0d` used wire v7/store v4. Its earlier
isolated fixture smoke is historical and was superseded in the same install
directory by the checkpoint 8288 wire-v8 pair above. The v7 smoke does not
verify the current pair. Nothing here approves a dispatch policy or modifies
the running Claude session. The old v2 installation remains untouched.

The previously described pre-v8 cleanup removed dormant CLI fallback helpers
and repaired the provider-call inventory, including callback routes. Its checks,
consumer regressions, rebuild, updated hashes/smoke and GitHub landing were
pending at that checkpoint. The prior installed v7 fixture pass does not prove
that cleanup or the current v8 candidate. See the durable task queue for current
gates. An operator-controlled attended bootstrap and real usage observation
remain necessary before claiming real-account readiness.

## Historical predecessor status — source and installed fixture verified

V2 uses monitor/store schema 2 and broker wire protocol 6. The installed pair
was built from clean source `4492d3cb91d9e45fafec2aa8acb8158268618cd9` and
verified twice with isolated callbacks and state. This is **not real-account
readiness**. Historical V1 results below are not V2 completion proof.
The old pair/state, original checkout, and Claude settings remain untouched.

A tooling incident activated a global `codebook-lsp` installation with crates.io traffic; that installer was stopped. Exact tooling request count is unknown. The overall session is not wholly offline. Provider/credential fixture counters are reported separately; no live provider or Keychain verification is authorized.

## Historical predecessor source gates

Binary source commit: `4492d3cb91d9e45fafec2aa8acb8158268618cd9`.
All commands used the absolute Rust 1.97.1 Cargo binary, `--offline --locked`,
and the isolated `/private/tmp/jackin-claude-monitor` checkout.

| Gate | Result |
| --- | --- |
| Protocol, relay, usage facade, broker, coordinator, host runtime, output, Claude provider | 439 passed; 1 pre-existing ignored |
| CLI unit regressions (`-p jackin --lib cli::`) | 216 passed |
| Installation/lifecycle/observation subprocess tests | 12 passed |
| Credential resolver, discovery, provider core | 139 passed; 83 inner filtered |
| Final broker rerun after fake HTTP fixture cleanup | 128 passed |
| Retry-After/reset fixture stability | 5 consecutive runs, 2 tests each, passed |
| Clippy: app, broker, protocol, relay, Claude provider; all targets, `-D warnings` | Passed |
| Scoped rustfmt: 29 changed Rust files; `git diff --check` | Passed |
| Direct offline pair build from clean source | Passed |

Counts overlap between reruns; do not sum them as unique tests. Fake-clock tests
cover idle/restart/suspend and reset deadlines without sleeping for ten minutes.
Fake Keychain tests cover disabled UI, missing/locked/consent outcomes and failure
to disable UI. They do not touch the native Keychain. Provider fixtures preserve typed 401 with zero implicit credential rereads or
retries, including an unchanged token and a later explicitly caller-changed token;
they also preserve 403, numeric/date 429, timeout and transport;
independent Claude provider collection remains disabled without a supported
contract, so no normal monitoring path invokes credential resolution or HTTP.

The direct telemetry registry check and generation succeeded using the vendored
semantic conventions and raw Weaver executable. The complete xtask gate then
failed on pre-existing unrelated legacy namespace literals, unchanged from the
base commit; it is **not** reported as passing. Generated usage telemetry consumers
are covered by the 216 CLI tests. This is targeted verification, not a claim that
all repository gates pass.

Independent Luna max reviews covered CLI/security, provider deadlines, state
integrity/migration, consumers, contract and handoff. Accepted findings were
repaired and verified: zero-budget approval, migrated-policy authorization,
strict account spend relevance for session-filtered guards, durable integrity,
and installed-proof schema assertions. Final bounded integrity/security/consumer
reviews have no outstanding actionable finding; static reviews are not live
account verification.

## Historical installed workflow proof from predecessor pair

- CLI: `/Users/donbeave/.local/share/jackin-claude-monitor-v2/bin/jackin`
- Broker: `/Users/donbeave/.local/share/jackin-claude-monitor-v2/bin/jackin-usage-broker`
- Built pair: `/private/tmp/jackin-claude-monitor/target/debug/` (isolated checkout).
- Intended real state: `/Users/donbeave/.local/share/jackin-claude-monitor-v2/state`;
  it was **not** populated with fixture approvals, goals, receipts or callbacks.

[v2-installation.json](v2-installation.json) records source tree, clean-build
assertion, exact hashes/sizes, toolchain, commands, script pre/post SHA and run times.
[v2-checks.json](v2-checks.json) records passing source gates and local log hashes.
[v2-coverage.txt](v2-coverage.txt) lists selected passing deterministic cases.
[v2-installed-smoke.log](v2-installed-smoke.log) is the final installed transcript;
[v2-installed-schema-examples.json](v2-installed-schema-examples.json) provides
actual nested JSON response examples. All identity/value examples are synthetic.
Independent Luna max installed review recomputed both installed binary hashes,
checked log/schema/exit evidence, and reported PASS within that fixture scope.

```text
python3 plans/claude-unattended-monitor/installed-smoke.py
# exit 0; two consecutive installed runs passed
# broker_process_selection=verified exact installed sibling invocation
# credential_trips=0
# http_proxy_requests=0
# installed_smoke=PASS
# fixture_removed_after_orderly_stop=true
```

The fixture reads only its own broker PID lease and argv, requiring the actual
installed sibling executable with the exact private data directory/build marker
and `--local-only`. `JACKIN_USAGE_BROKER_BIN` was unset in cleared fixture env;
this does not establish its value in Claude's environment. Versions alone were
not used as provenance. Private HOME/config/data, bounded callbacks, fake model
and rate-limit evidence, command tripwires and a loopback HTTP proxy were used.
In both runs, the bound composed adapter execution preserves existing output;
both composition proposals leave fixture settings files unchanged; no real settings proposal was applied.

Observation starts without a receipt, goal, policy or spend baseline and never
authorizes dispatch. A separately approved **synthetic** quota-only guard with
fresh fixture quota evidence starts runnable, reports `readiness.budget=disabled`,
spend unknown, and null budget/baseline/cumulative amount. Duplicate starts reuse
IDs. Headless auth/binding/policy commands return `interaction_required` exit 2
before helper/broker work. Passive reads leave counters unchanged. The installed
observer proves reset validity unknown then future independently, plus expected
model unknown then match. Strict baseline admission, rollback/history integrity,
thresholds and reset barriers are verified by source fixtures, not a real SGD bill.

Counters cover PATH `op`/`claude`/`security` commands and requests routed through
the fixture proxy. They are **not OS-enforced native Keychain/egress tracing**.
Static attach-only route reviews and disabled provider execution, along with fake
Keychain no-UI tests, are the additional no-dialog/no-provider evidence. No live
provider/Keychain/interactive-auth verification was performed. Provider fake HTTP
request-count assertions include one shared flight, zero requests before persisted
Retry-After, no reset-time bypass, and a next attempt only after the deadline.

## Historical handoff reverification

The installed smoke was rerun before handing instructions to Claude. Both binary
hashes still match; no product source changed between the predecessor build at
`4492d3cb` and this installed smoke. Independent
Luna max review found no blocking CLI/schema mismatch. Its one illustrative-path
finding was fixed: printed settings examples use the absolute path rather than
a quoted `~`. The corrected run passed with zero credential-command/proxy counts
and orderly cleanup; manifest/log/schema examples record this latest run. No
live account, native Keychain or provider readiness claim was added.

## Remaining operator steps and limits

1. Verify the exact v2 paths and any broker override in the operator's environment.
   Do not change Claude's environment or reuse an unknown service.
2. Choose a genuine session for session-only observation, or confirm a stable
   local account label in an attended terminal. A label is not authenticated
   provider identity and cannot detect account switches automatically.
3. Review the composed proposal and have the operator install it outside this
   task's protected running-session scope; receive a **genuine supported Claude
   callback**. No receipt is required to collect/report observations.
4. Separately approve dispatch policy. Existing strict goals cannot downgrade;
   strict first activation needs fresh compatible same-account SGD evidence.
   No real receipt/policy selection has been supplied or inferred here.

Statusline rate-limit fields were verified against the current official
[statusline documentation](https://code.claude.com/docs/en/statusline) and
[v2.1.80 changelog](https://raw.githubusercontent.com/anthropics/claude-code/v2.1.80/CHANGELOG.md).
The source has no provider observation timestamp/account identity, model-specific
limit, or authoritative extra-usage permission. Callback receipt time is reported
separately; absent evidence time remains null. Unchanged values, timer replay and
sibling changes do not refresh evidence age. A 300-second freshness bound can
therefore block unchanged reset/model descriptors even during activity; report-only
validity does not relax it. No guaranteed current data during inactivity.

SGD receipts remain manual operator attestations with 300-second freshness, not
an automatic authoritative billing updater. Quota-only provides no SGD enforcement;
strict mode is a local conservative dispatch guard, not a hard per-goal billed cap.
Session USD/list-price cost and unknown credits never become billed SGD. Hard billing
protection requires the account-side Anthropic spending limit. Unsupported
independent Claude provider observations remain disabled; local refresh does not
fetch new evidence. The 300-second minimum is not a guarantee against bans.
Reset time plus grace is only a wake hint, and weekly/model/spend guards remain.
No automatic Claude reinvocation or guaranteed resumption is provided. External
setup blockers require checkpoint/report/exit, not repeated doctor/completion loops.

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

## Historical V1 direct-Cargo transcripts and results

These verbatim commands and results record predecessor V1 verification. They
are historical evidence, not current main-port test results or run
instructions:

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

Historical V1 provider, broker, protocol, and consumer gate transcripts:

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

## Historical accidental Docker suite invocation

During predecessor V1 verification, this broad command was run accidentally
and was not counted as offline verification:

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


## Historical predecessor renewed acceptance audit

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

That predecessor snapshot's fresh gates: broker 101 tests and all-target
Clippy; Claude provider 37 and discovery 43 and all-target Clippy; broker auth helper 7; CLI help 1; usage
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


## Historical predecessor replacement build and installed proof

The historical predecessor run's direct-Cargo build
`cargo build --offline --locked -p jackin --bin jackin --bin
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

That predecessor snapshot also recorded passing current-at-the-time gates: the
complete 30-test usage consumer group, two exact fake single-flight/rate-deadline
E2Es and app all-target Clippy. Twenty clients assert one fake provider call;
forced early waiters assert one call; eight recovery clients assert one
replacement (two total including the killed fixture owner).

Canonical build directory remains
`/Users/donbeave/Library/Caches/mbx/targets/v1/519eebab1636eb6d9b57bb77aa6e6d394b5e4bf8c0d778e9bcdd1148ee1fffff/debug`.
Both binaries report 0.6.4.

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


## Historical final composed-hook verification — 2026-10-10

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
