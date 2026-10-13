# TASK-002 src3 / sanitized tip evidence

Read-only. Repo `/Users/donbeave/Projects/jackin-project/jackin`. No commit, push, build, or source edit.

| Pin | SHA | Subject |
| --- | --- | --- |
| M | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` | fix(usage): retain foreground broker for auth lease (#1124) |
| P | `fba69d60c2373156dc0dcecee8e085c83776966f` | docs: record Velnor d35 lock audit |
| U | `2e4a62da4bc4406764162a267163389a5fb513a7` | docs(usage): update v11 live handoff gates |

Unique commits are `git rev-list M..tip` (reachable from the tip, not from M). Cherry marks use `git log --cherry-mark --right-only`. Patch identity uses `git patch-id --stable`. None of these tips is an ancestor of M, P, or U. Cherry equality is an aid; the verdicts below use the destination function and test bodies.

Text that contains 401, 403, or 429 is not credential or retry authority. A first-parent range inside one recorded push is not a first-parent-history obligation denominator. Installer-wide `RUSTC_WRAPPER` / `SCCACHE_GHA_ENABLED` is not restored.

## Verdicts

| Tip | Verdict | Proof |
| --- | --- | --- |
| B026 `2d97bba390864fbdc79aa6566e3394df938408af` | **ALREADY_IN_P_OR_U** | Tip tree equals parent. Ancestral product behavior is in P and U. The in-process fallback ancestor is the B034 patch and is rejected there. |
| B027 `502e0627d3940e2cd68ae0078910f21452d9ccc1` | **ALREADY_IN_P_OR_U** | Per-request 0/1/2 assertion blob matches M, P, and U. |
| B028 `6ce20ce13cb87c5ab89b44d72698fa0d68aacf39` | **ALREADY_IN_P_OR_U** | Typed status and header retry metadata are in P and U. Message text is not authority. |
| B030 `12dea6c5fd0f9d2c594d5ad3159e6493d03352f4` | **REJECT_WITH_EVIDENCE** | Tip denominator is first-parent history. P and U use a push-head ledger. Those are not the same proof. |
| B032 `b1dc105acffb68ba71fff86f9c18dc5ff77de924` | **REJECT_WITH_EVIDENCE** | Tip exports installer-wide sccache. P and U assert that wrapper and backend are absent. |
| B034 `33d88b1cf764f614d3935c11c48decd53386083e` | **REJECT_WITH_EVIDENCE** | Bundled `jackin-usage-broker` is already in P and U. Missing-sidecar in-process fallback is absent; P and U fail closed. |

---

## B026 — sanitized handoff tip

**Verdict: ALREADY_IN_P_OR_U**

Merge-base with M: `5639cdc30e93249eac6a76c6e5df451000c82f76`. Ahead/behind `M...tip`: 44 / 11. No cherry-equivalent commit in M, P, or U.

### Tip patch

`git diff --raw 2d97bba390864fbdc79aa6566e3394df938408af^ 2d97bba390864fbdc79aa6566e3394df938408af` is empty. Both trees are `3c01a03b8cb2aae87c284438aab1a66284485959`. `git show --numstat` has no file lines. `git patch-id --stable` returns no id. Planning's null API patch is true for this commit versus its parent. It is not true of the unique ancestry.

### Unique commits versus M

| SHA | Subject | Tree delta | Same behavior in P and U |
| --- | --- | --- | --- |
| `0268e45490b0d3dcb5519e6218b2bd81dcb5107a` | fall back to in-process broker, ship sidecar | 11 files, +528/−194 | Same stable patch-id as B034 `33d88b1c` (`a94d207c42163b5e7e560fd8ee47ac6d9361d568`). Bundled binary already present. Fallback rejected under B034. |
| `d75c0f112261703ae16db19eeb3729e5f09db1db` | silent NotFound cleanup, channel-aware capsule errors | 40 files, +276/−144 | `cleanup_warning_ignores_not_found` and preview download-failure tests are in P `crates/adapters/jackin-image/src/capsule_binary/tests/case_01.rs` and U `crates/jackin-image/src/capsule_binary/tests.rs`. `local_version_override` returns the cargo version unchanged for local builds in P `crates/core/jackin-build-meta/src/lib.rs` (lines 65–76). The extra `EXPECTED_PRERELEASE_MARKER = Some("-dev")` gate is not in P or U. Workspace `version = "0.6.4"` at M `Cargo.toml:43`, P `Cargo.toml:161`, and U. Restoring that gate would reject the current bare version. Verbatim embedding is already the function. |
| `039ce1b1245d728ec9718ace42a425c40035592b` | container stderr in pre-attach diagnosis | 11 files, +361/−50 | `diagnose_premature_exit_surfaces_stderr_only_docker_logs` is in P `crates/services/jackin-runtime/src/runtime/launch/tests/case_03.rs` and U `crates/jackin-runtime/src/runtime/launch/tests.rs`. `run_preserving_evidence` and `disarm` are in P `crates/services/jackin-runtime-launch-load-cleanup/src/load_cleanup.rs` and U `crates/jackin-runtime/src/runtime/launch/load_cleanup.rs`. |
| `b99275f8812b3b2fe71b0d2c6fef21e79091ceba` | Antigravity discovery and refresh | 9 files, +495/−43 | `read_antigravity_keychain`, `disc_antigravity_grant_mints_cli_refresh_material`, and `refresh_antigravity_binding_dispatches_to_cli` are in P `crates/services/jackin-usage-discovery/` and U `crates/jackin-usage/src/host/discovery/`. |
| `26249aad300dc912f56ab101c6d56297166dd835` | Landlock symlink grant | 1 file, +41/−0 | `claude_installer_share_dir_is_granted_read_only` is in P `crates/apps/jackin-capsule/src/process_isolation/tests/linux_rules.rs` and U `crates/jackin-capsule/src/process_isolation.rs`. |
| `879988abe127593b92e7e0267e624b2bb35d6aaf` | rustfmt | 3 files, +19/−20 | Formatting only. |
| `2754bd217a93de841df0b80c6f4ce3b3c1b75168` | re-render velnor state | 2 files, +3/−3 | Generated `.github/ci` state only. |
| `ec3dbcf662f1f62eb20a53e7a9e4ae6004c05498` | Keychain fail-fast consent diagnostic | 6 files, +249/−29 | `KeychainConsentRequired` and `ClaudeKeychainRead::ConsentRequired` are in P `crates/services/jackin-usage-discovery/` and `crates/services/jackin-usage-provider-claude/src/keychain.rs`, and in U `crates/jackin-usage/src/host/discovery.rs` plus `crates/jackin-usage/src/usage/claude/keychain.rs`. P test `disc_claude_keychain_consent_is_not_reported_missing` is `crates/services/jackin-usage-discovery/src/tests/case_01.rs`. |
| `bda29aa72cb6b51bb8e751dcb50661eec2535c99` | pause checkpoint docs | empty | No tree delta. |
| `a43df8f41168a432982a8d363c3a1a6b8299306e` | finalize metadata docs | empty | No tree delta. |
| `2d97bba390864fbdc79aa6566e3394df938408af` | audit repair docs | empty | Tip. Identical tree to parent. |

---

## B027 — per-request diagnostic retry

**Verdict: ALREADY_IN_P_OR_U**

Merge-base with M: `fce94cea8a15de0c2db3bb4ff880d741baf5c00a`. Ahead/behind `M...tip`: 69 / 6. No cherry-equivalent commit in M, P, or U. The assertion still matches byte for byte.

### Behavioral delta

Tip `502e0627` changes only `crates/jackin-diagnostics/tests/wire_failure_support/mod.rs` (+14/−3). `assert_wire_requests` requires every captured log request to carry exactly one record whose event name matches, in addition to trace/log/metric totals. A 0/1/2 split whose flattened total is 3 fails.

### Unique commits versus M

| SHA | Subject | Delta |
| --- | --- | --- |
| `7d6de46e5a3c526d3368d7b79f64394654ecfd51` | actionable wire-count mismatches | helper + `plans/cicd-reliability/EXECUTION.md` |
| `c65be28fe996bd76296d318d67000de1f1be0336` | PR 1014 docs | one line in `EXECUTION.md` |
| `dde7ff514101308f8fd2b38067dab324c296ce44` | clippy::panic in wire-count helper | helper plus ledger/json notes |
| `3dc9f60f1262e928ca69d252d40a449a93f325ea` | Policy generated-tree docs | plan notes |
| `dfd61f67cf75d8ec7b37dc45780de4d4c466fc2a` | regenerate ownership state | `.github/ci/.github-actions-generator-state` |
| `502e0627d3940e2cd68ae0078910f21452d9ccc1` | per-request log-record distribution | the assertion above |

### P and U

Blob compare of the helper is empty against:

- M and U: `crates/jackin-diagnostics/tests/wire_failure_support/mod.rs` (comment and `records.len() == 1` check at lines 69–82)
- P: `crates/services/jackin-diagnostics/tests/wire_failure_support/mod.rs` (same lines)

The 0/1/2 distribution fails in all three trees even when totals match. Plan notes exist at P and U under `plans/cicd-reliability/`. They are not the retry contract.

---

## B028 — typed provider status

**Verdict: ALREADY_IN_P_OR_U**

Merge-base with M: `fd14ac6a7e0642842e66eeeb31d4b589d07148e1`. Ahead/behind `M...tip`: 25 / 2.

### Unique commits versus M

| SHA | Subject | Cherry vs M, P, and U |
| --- | --- | --- |
| `67f89e95c416dc6571155e63649624a3335addba` | inject fixed clock into parity renders | `=` equivalent patch in M, P, and U |
| `6ce20ce13cb87c5ab89b44d72698fa0d68aacf39` | preserve typed provider error statuses | not cherry-equivalent; behavior is present in rewritten modules |

Stable patch-id of `6ce20ce1` equals B030 ancestor `b6b3744ddcb75e110eb4ff5a68540446f10ef228` (`89308caa45d43ce07cead831f933609cc6a0a2af`).

### Behavioral delta

The tip stops classifying auth, scope, and rate limit by searching error strings.

- `claude_error_is_scope_restriction` is true only for typed HTTP 403. Transport, decode, and CLI strings that mention 401, 403, or 429 do not qualify. A typed 403 whose message mentions 401 still qualifies. Typed 401 and 429 do not.
- `usage_error_is_unauthorized` is typed 401 or 403. `usage_error_is_rate_limited` is typed 429.
- Tip `parse_retry_after_seconds` always returns `None` so a rendered Retry-After phrase cannot create a deadline.

### P and U

P:

- `crates/services/jackin-usage-provider-core/src/refresh.rs` lines 199–204: unauthorized is `Some(401 | 403)`; rate limit is `Some(429)`.
- `crates/services/jackin-usage-provider-claude/src/wave.rs` lines 120–121: scope restriction is `error.status() == Some(403)`.
- `crates/services/jackin-usage-provider-claude/src/tests/case_01.rs`: transport/decode/plain strings that mention status codes are not scope restriction; only typed 403 is.
- `crates/services/jackin-usage-provider-codex/src/tests/case_01.rs`: `auth_and_rate_limit_classification_requires_typed_http_status`.
- `crates/services/jackin-usage-provider-core/src/tests/case_04.rs` `typed_rate_limit_preserves_retry_after_but_rendered_429_text_does_not`: a typed 429 keeps header `retry_after_seconds`; transport/decode text that mentions 429 and `Retry-After: 37` yields no status, no retry seconds, and no rate limit.
- `crates/services/jackin-usage-provider-core/src/http.rs` lines 142–148: retry seconds come from `retry_after_header_seconds(response.headers(), ...)`, not from the error string.
- `crates/services/jackin-usage-broker/src/tests/case_01.rs` `discovery_provider_error_text_cannot_set_rate_limit_or_retry_deadline`: view text including `port 429` leaves `retry_at_epoch` as `None`.

U has the same rules in `crates/jackin-usage/src/usage/refresh.rs` lines 190–195, `crates/jackin-usage/src/usage/claude.rs` line 467, `crates/jackin-usage/src/usage.rs` lines 1658–1664, and `crates/jackin-usage/src/host/broker/tests.rs` lines 652–674.

P and U keep structured header retry metadata. That is stronger than the tip's always-`None` parser. Message digits remain non-authoritative. Do not restore the always-`None` parser over the header field.

---

## B030 — first-parent CI denominator

**Verdict: REJECT_WITH_EVIDENCE**

Merge-base with M: `fd14ac6a7e0642842e66eeeb31d4b589d07148e1`. Ahead/behind `M...tip`: 25 / 8.

### Unique commits versus M

| SHA | Subject | Disposition of that commit |
| --- | --- | --- |
| `67f89e95c416dc6571155e63649624a3335addba` | fixed clock | cherry-equivalent in M, P, and U |
| `7c178a8a3caecf616b238251f8f87d852943eb89` | isolate parallel OTLP harness | test isolation, not the denominator |
| `900d7b6f06e986fcfc3f8c53e2843e5f6d291b77` | OTLP testbed lifecycle | testbed ownership, not the denominator |
| `7c57de6c6d052ec040359081ae868e16e0491cae` | reconstruct missing event heads | ancestor collector; P/U replaced the denominator with the push-head ledger |
| `b6b3744ddcb75e110eb4ff5a68540446f10ef228` | typed provider statuses | same patch-id as B028; already in P and U |
| `762ea61e7b632c017b5f792018418764d2d2db74` | converge evidence and Apple workflow generation | generated workflow/mise tree; not the denominator decision |
| `63c8e8b1338936b7881926f7afcd35ebb3ed8c4c` | wire sccache for Apple cargo jobs | different patch-id from B032 (`7760816ec88bb99e170cadf11f597ae2e5a61d64` vs `4f65fc936fd1b58ee285714ec0c6d6dcd634abc0`) but the same installer-wide wrapper. Rejected with B032. |
| `12dea6c5fd0f9d2c594d5ad3159e6493d03352f4` | harden first-parent collection | the rejected denominator |

### Behavioral delta of the tip

`crates/jackin-xtask/src/ci_evidence.rs`:

- Line 364: no fixture means `expected_from_first_parent`.
- Lines 613–685: `git log --first-parent` of `refs/remotes/origin/{branch}` becomes `DenominatorSource::FirstParentHistory`, and every history commit becomes an obligation.
- Lines 1628–1642: `require_qualified` fails unless the source is first-parent history and `green_claim_qualified` is set.
- `.github-gen/velnor-workflow.toml` lines 126–127: first-parent main history is described as the complete denominator proof.
- Test `first_parent_history_derives_both_contract_obligations` and `fixture_denominator_cannot_qualify_green` in `crates/jackin-xtask/src/ci_evidence/tests.rs`.

That model treats each first-parent commit in the window as a workflow obligation. A multi-commit push can have one workflow head. Those are not the same set.

### P and U

P `crates/tools/jackin-xtask/src/ci_evidence.rs` lines 8–11 and 512–520: the live denominator is `expected_from_push_head_ledger`. The rollup is an advisory observer of a durable push-head artifact. It does not create that producer workflow.

P `crates/tools/jackin-xtask/src/ci_evidence/ledger.rs`:

- Line 42: `expected_from_push_head_ledger`.
- Lines 613–628: `--first-parent` is used only to list commits in one artifact's `before_sha..head_sha` and to reject an artifact whose `pushed_commits` do not equal that range.

P `crates/tools/jackin-xtask/src/ci_evidence/producer.rs` lines 197–207: `--first-parent` lists commits inside one live push event. It is not `git log --first-parent` of the branch used as the obligation set.

U has the same functions at `crates/jackin-xtask/src/ci_evidence/ledger.rs` and `crates/jackin-xtask/src/ci_evidence/producer.rs`.

P already has the negative cases under the push-head source, not under first-parent history:

- `fixture_denominator_remains_advisory` in `crates/tools/jackin-xtask/src/ci_evidence/tests/case_04.rs`
- `conflict_marker_requires_raw_conflicting_observations` in `crates/tools/jackin-xtask/src/ci_evidence/tests/case_02.rs`

U has those tests in `crates/jackin-xtask/src/ci_evidence/tests.rs` (`fixture_denominator_remains_advisory`, `conflict_marker_requires_raw_conflicting_observations`).

Do not call P/U's in-range `--first-parent` check equal to B030's first-parent-history denominator.

---

## B032 — installer-wide sccache

**Verdict: REJECT_WITH_EVIDENCE**

Merge-base with M: `fd14ac6a7e0642842e66eeeb31d4b589d07148e1`. Ahead/behind `M...tip`: 25 / 2. Neither unique commit is cherry-equivalent to M, P, or U.

### Unique commits versus M

| SHA | Subject | Delta |
| --- | --- | --- |
| `432e0108ce90b64b428c69c27d4a2ac23e559379` | converge native workflow generation | 14 generated workflow/state files, +718/−253. No Rust product source. Not the sccache decision. |
| `b1dc105acffb68ba71fff86f9c18dc5ff77de924` | wire sccache for Apple cargo jobs | the rejected wrapper |

### Behavioral delta

`.github-gen/velnor-workflow.toml` sets `RUSTC_WRAPPER = "sccache"` and `SCCACHE_GHA_ENABLED = "true"` on shared profiles and on caller env, including profiles whose tools include `cargo:sccache` before other tools. Generated `.github/workflows/desktop-merge.yml` and `desktop-scheduled.yml` put those variables on the job env while `install_args` also installs `cargo:sccache`. `ci-unit-swift.yml` sets the same env. Tests in `crates/jackin-xtask/src/ci/tests.rs` require the wrapper on every provisioned profile (`sccache_env_is_bound_only_to_provisioned_cargo_profiles`).

That is installer-wide activation, including before tool setup, and it wraps a non-Cargo Swift job.

### P and U

P `crates/tools/jackin-xtask/src/ci/tests.rs`:

- lines 87–94: `assert_no_sccache_env` rejects `RUSTC_WRAPPER: sccache`, `SCCACHE_GHA_ENABLED: "true"`, and the `=` forms
- line 107: `sccache_is_absent_from_generated_workflow_environment`
- line 115: `sccache_is_absent_from_installer_environment`

U has the same tests at `crates/jackin-xtask/src/ci/tests.rs` lines 86–123. U `mise.toml` pins `"cargo:sccache"` and `crates/jackin-xtask/src/release_archive.rs` can write optional stats. That pin and stats writer do not export `RUSTC_WRAPPER` into workflow or installer env. P's sccache grep hits are only the absence tests.

Do not restore the global wrapper or the GHA backend.

---

## B034 — bundled broker executable versus missing-sidecar fallback

**Verdict: REJECT_WITH_EVIDENCE**

This is the usage-broker executable, not the native Swift sidecar. One unique commit. Merge-base with M is the parent `68edddabb1a0f3b191f488cdf4d7206f3df5a381`. Ahead/behind `M...tip`: 46 / 1. Not cherry-equivalent to M, P, or U. Same stable patch-id as B026 ancestor `0268e454`.

### Unique commit versus M

`33d88b1cf764f614d3935c11c48decd53386083e` — 11 files, +528/−194:

- `crates/jackin/Cargo.toml` adds `[[bin]] name = "jackin-usage-broker"`
- `crates/jackin/src/bin/usage_broker.rs` and `crates/jackin-runtime/src/usage_broker.rs` `run_service_process`
- `crates/jackin-usage/src/host/broker.rs` `ensure_usage_broker` (about lines 1503–1538): if `sidecar_service_usable` is false, start an in-process executor instead of failing closed
- test `ensure_usage_broker_falls_back_to_in_process_when_sidecar_missing` in `crates/jackin-usage/src/host/broker/tests.rs`

### Bundled executable — already in P and U

- P `crates/apps/jackin/Cargo.toml` line 28: `name = "jackin-usage-broker"`
- U and M `crates/jackin/Cargo.toml`: same bin name
- P `crates/apps/jackin/tests/broker_installation.rs` `packaged_broker_version_needs_no_service_configuration` runs `CARGO_BIN_EXE_jackin-usage-broker --version`
- U `crates/jackin-xtask/src/release_archive.rs` packages `jackin-usage-broker` with jackin
- U `crates/jackin-usage/src/host/broker.rs` `default_service_executable` resolves the sibling `jackin-usage-broker`

### Missing-sidecar in-process fallback — rejected

`ensure_usage_broker_falls_back_to_in_process_when_sidecar_missing`, `sidecar_service_usable`, and `run_service_process` occur only at the B034 tip. Counts in M, P, and U are zero.

P `crates/services/jackin-usage-broker/src/ensure.rs` lines 24–61: a missing executable returns `UsageCoordinationErrorKind::Unavailable` and does not start an in-process broker. Spawn failure does the same.

P `crates/apps/jackin/tests/broker_installation.rs`:

- `passive_usage_reads_do_not_activate_missing_broker_override`
- `explicit_service_start_preserves_missing_broker_override_path`

Both point `JACKIN_USAGE_BROKER_BIN` at a missing path and assert the broker run directory is not created.

U `ensure_usage_broker_process` is `crates/jackin-usage/src/host/broker/service.rs` line 45 and follows the process executable, not an in-process fallback.

Availability of a missing sidecar is not a reason to serve in-process. P and U fail closed.
