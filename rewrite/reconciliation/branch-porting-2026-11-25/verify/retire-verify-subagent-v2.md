# retire-verify-subagent-v2: body-level verification of the `preserve/subagent-v2-01a1aec6-…` refs

Verifier: Sonnet 5.5, read-only. `origin/main` = `85a921a93` (fetched; includes #1302, the P14 pagination fix and the P20 fix `3bb5151fb`). Plan: `jackin-branch-porting-plan.md` (section 3 exclusions, section 5 P01-P21).

## Verdict: 13 of 13 SAFE (a 14th ref, `…01a1afa2-…`, is also SAFE). 0 VALUE, 0 UNCERTAIN. No MISSING-VALUE item.

## Structure (this changes the review size)

The 14 refs are not 14 independent ~2400-commit lines. They are one linear history with one extra commit each.

- Merge base with main: `470fc5d6f` (on main). The shared chain is about 1000 commits long, ends at `3773637b5` ("feat: native key-agent sync and placeholder configs (A5)") and is the A2/A3/A3M/A4/A5 multi-agent work.
- Every ref = a chain commit + exactly one commit "chore(preserve): snapshot uncommitted worktree before local cleanup". `git rev-list --count <tip>..<ref>` is 1 for every ref; the ref-to-tip distance is 0 to 25 chain commits.
- The whole shared chain (`470fc5d6f..3773637b5`) is reachable from `origin/codex/openshell-verified-checkpoint-20261108`, `origin/codex/openshell-integration-checkpoint-20261108-122311`, `origin/preserve/pr1264-merge`, `origin/preserve/stash-w8-port-feature-openshell-native` and the 21 `preserve/t3/*` refs (`git branch -r --contains` for `3773637b5`, `a439b5f0e`, `3aabbbb2b`, `c380cbee8`, `1ab5ce492`). Deleting the subagent-v2 refs therefore loses only the 14 snapshot commits. Those touch only `crates/` plus `Cargo.toml` and `Cargo.lock` (no `.github`, `.velnor` or `rewrite/` path).
- Two things were reviewed: (1) the 14 snapshot deltas (`git diff <parent> <ref>`), and (2) the newest tip's whole tree against main, which covers the shared chain.

## Method

1. `git fetch`; `git merge-base`, `git rev-list --count` and `git diff --stat` between all refs, as above.
2. Tip tree against main. Lever `v2b/nov2.py` (scratchpad): for `git diff -U0 470fc5d6f 5e1f8c2db -- crates/`, every added line whose stripped text is absent from the whole `origin/main` tree (168,267 distinct lines). Result: 2602 absent lines in 302 files. Added `fn` names absent on main were listed separately (49 files).
3. The product hunks (1059 absent lines in 99 `src/` files, excluding xtask and gemini) were read in full; test, e2e and example hunks were read by name and keyword. Each group was checked against main by symbol or test name with `git grep` on `origin/main` (`v2b/gg.sh`).
4. Snapshot deltas. Lever `v2b/snap.py`: each snapshot's added `crates/` lines classified as present on main, present only in the tip tree (covered by step 2) or absent from both. The absent-from-both lines (173 in total) were read one by one.
5. P-item check: each of the 14 trees was grepped for any fix of P02, P04, P16 and P20.

Limits. Presence is exact stripped-line containment plus symbol or test-name checks. Main refactors much code, so each "not on main" group was judged by reading and by symbol lookup, not by line identity alone. No code was built or run.

## Per-ref verdicts

Columns: ref block (full name is `preserve/subagent-v2-01a1aec6-ddf6-7742-ab2e-93c65dabe01b-01a1<block>-…`), head, parent chain commit, added `crates/` lines in the snapshot commit, of those present on main, present only in the tip tree, absent from both main and tip.

| ref block | head | parent | added | on main | tip only | absent both | verdict |
|---|---|---|---|---|---|---|---|
| aedd | `62b483ee4e75` | `3aabbbb2b` | 729 | 632 | 85 | 12 | SAFE |
| aefc | `8c427113e53e` | `c380cbee8` | 1008 | 947 | 41 | 20 | SAFE |
| af34 | `e1dceb179bef` | `add5dcb1a` | 1359 | 1226 | 115 | 18 | SAFE |
| af57 | `86fc1d637aee` | `cf84ce4d9` | 399 | 337 | 62 | 0 | SAFE |
| af6f | `1dbe3f65a1e2` | `8492f7070` | 209 | 179 | 5 | 25 | SAFE |
| af7b | `16005c405aae` | `26d562a82` | 352 | 288 | 45 | 19 | SAFE |
| af8d | `15c12d19e544` | `0208d5997` | 84 | 79 | 1 | 4 | SAFE |
| afa2 | `1c1cabdae44f` | `6ea38ba1d` | 89 | 83 | 3 | 3 | SAFE (14th ref, not in the retire-verify-1 table) |
| afaa | `8d1b528a680d` | `4afe8a64c` | 103 | 103 | 0 | 0 | SAFE |
| afcb | `c5a98a0870c9` | `1ab5ce492` | 242 | 144 | 44 | 54 | SAFE |
| afdb | `1190c7efac65` | `d66910de5` | 39 | 19 | 5 | 15 | SAFE |
| afe7 | `43fc09eeaf78` | `cf32585a0` | 20 | 8 | 10 | 2 | SAFE |
| aff2 | `f424cce6020c` | `a439b5f0e` | 607 | 521 | 85 | 1 | SAFE |
| b002 | `5e1f8c2dbac1` | `3773637b5` | 384 | 326 | 58 | 0 | SAFE |

Every snapshot is an earlier uncommitted worktree state of the same rewrite line. Its lines are on main, or in the tip tree (reviewed below), or in the small absent-from-both set, which is superseded design (below). Nothing in a ref tree beyond the chain is uncovered by the tip review.

## Tip tree (`5e1f8c2db`) against main: grouped classification

Overall picture: the tip is an older rewrite state. Main is ahead of it in almost every subsystem (project-root identity `verify`, credential-env collision check, `has_provider` generalised from `chatgpt_login`, 22-row catalog, per-agent image stages, instance incarnation/kill/probe, `Redactor` on provider writes, an `issued` pagination set).

| group | files | classification and evidence |
|---|---|---|
| Codex-only agent/catalog (12-tuple `TUPLES`, `read_codex`, `CodexSection`, `[codex] model`) | `jackin-core/src/agent.rs`, `manifest.rs`, `config*.rs`, `assets/t0x-*.yaml`, `tests/catalog.rs` | on main, superseded: 22-row catalog (`all_22_catalog_rows_match_the_approved_matrix`), `read_agents`/`validate_agents`. Tuple promotion (T04/T06/T07/T08) is blocked by D29 (section 3), so the tip's "supported" rows must not return. |
| Gemini Code Assist usage reader, `dispatch.rs::gemini_usage`, 12 gemini tests | `jackin-usage-provider-gemini`, `tests/usage/providers/gemini.rs` | excluded (section 3): removed by #1296 (`4a2b869c8`). |
| Provider usage readers (kimi limits/summary, openrouter credits, grok on-demand, minimax, zai, opencode, claude/codex) | `usage/jackin-usage-provider-*`, `tests/usage/providers/*` | on main, reworked by #1296. 403 is deliberately `Provider(text)` on main (`dispatch.rs::reason`); grok on-demand is `== Some(true)` with fixtures `billing_credits_on_demand_off/on` and test `on_demand_disabled_leaves_no_on_demand_window`; zai gains `TIME_LIMIT`; minimax has `token_plan_no_end_seconds`. Cent rounding in OpenRouter is the same on tip and main: P02. |
| Usage HTTP client | `provider-core/src/http.rs` | on main, superset: `redirect(Policy::none())` at `http.rs:95,156`, `Retry-After` date parsing. |
| Usage credential/OMP/OpenCode readers | `jackin-usage/src/credential.rs`, `jackin-session/src/credentials.rs` | covered by P01. The tip's `extract_omp_api_key` is a raw text scan of SQLite bytes (snapshot `1dbe3f65a`), later `rusqlite` `SQLITE_OPEN_READ_ONLY`; main has the `rusqlite` read (`credentials.rs:223`) and P01 specifies the bounded capture. |
| `jackin-core/src/fs.rs` | `fs.rs` | on main: the tip differs only by inlined closures against main's named helpers (pure refactor). Same `lock_file` and `to_string_lossy` list weaknesses on both: P04. |
| Provider rollback (`sync_if_absent`, `forget_locked`) | `jackin-session/src/accounts.rs`, `launch/run.rs` | covered by P16: the tip has the same delete-on-failed-read weakness; no `WriteEffect` or receipt in any ref tree. |
| Account gateway scoping | `accounts.rs`, tests `…never_need_the_gateway`, `…connects_to_nothing`, `rm_of_…never_connects…gateway_down` | on main under provider-generic names (`edit.rs:120`, `locked_view.rs:171`, `account_manage.rs:385`, `gateway_rm.rs:144`, `needs_gateway` at `accounts.rs:408`). Further scoping is P13. |
| Stale-launch fences | `tests/accounts/stale_launch.rs` | on main (7 tests, incl. `a_launch_planned_before_the_account_lost_its_provider_fails_and_creates_no_provider`). |
| Provider sync and missing key before sandbox | `tests/launch/provider.rs` | on main as `the_provider_is_synced_once_and_a_missing_key_stops_before_the_sandbox`. |
| Placeholder validation (printenv, `PLACEHOLDER_SCHEME`) | `launch/run.rs`, `tests/launch/credentials.rs` | on main: `PlaceholderMissing/Invalid` (`run.rs:62,66`), test `t01_malformed_placeholders_fail_without_echo_or_auth_state`. |
| Guarded purge identity (`created_at` compare) | `lifecycle.rs` | on main (`lifecycle.rs:165`, `replaced = match (view.created_at, now.created_at)`). |
| Role dirty-tree hashing (`ls-files -z -co --exclude-standard`) | `roles.rs`, `tests/plan/roles*.rs` | on main and stronger: ignored files are hashed (`a_dirty_role_tree_hashes_tracked_untracked_and_ignored_files`, `a_git_ignored_role_input_is_hashed_and_staged`, link and exec-bit digests). The tip's "not ignored" expectation is superseded by design. |
| 1Password `section` disambiguation | `op.rs`, `source.rs`, `tui/onepassword.rs` | on main (`op.rs:113 section: Option<OpSectionInfo>`, `section_of`). |
| Export bundle Sha1/Sha256 trailer, `export_memory` example | `export/bundle.rs`, `examples/export_memory.rs` | on main, newer (`gix_hash::Hasher`, `ObjectFormat` check, `tests/export/bundle_limits.rs`). |
| gRPC provider secret redaction, TLS handshake message | `grpc/provider.rs`, `grpc/connect.rs` | on main (`provider.rs:48 redact`, `Redactor::new` at 257 and 293; `connect.rs:53`). |
| Pagination | `grpc/sandbox.rs` | on main: `issued.insert` repeated-token rejection (P14, landed), `MAX_PAGES`, `TOKEN_MAX_BYTES`. |
| Multi-agent image Dockerfile and key | `jackin-image/src/plan.rs`, `key.rs` | on main (`AGENT_STAGE_ANCHOR`, `archive_stages`, `agent_stage`, `scan_absent_dockerfile`, `scan_absent_paths`). Final-image alias validation is P15. |
| Non-lead instance start (`setsid script -qec`, `mkfifo`, `pkill`, `agent_names` text scan) | `multi_session.rs`, `multi.rs`, `commands/agent.rs` | on main, replaced: `stop_instance`, `restart_instance`, `kill_instance`, `instance_incarnation`, `probe_instance`, `sandbox_agent_names`, `instance_argv`; no `setsid` or `mkfifo` remains on main. |
| Agent instances order and label | `jackin-core/src/instance.rs` | on main, stronger: `canonical()` keeps the lead first and hashes a `lead-first-v1:` preimage; credential-env collision error; `ProjectRoot::verify`. The tip's `sorted()` is the older form. |
| Wait/budget | `jackin-core/src/wait.rs` | on main (retire-verify-1 already checked the waited+probed budget). |
| TUI backend refresh scheduler (`Refreshes`, `Cause`, `Refreshing`) | `jackin-tui/src/backend.rs` | excluded: section 6 (no second console refresh scheduler; Termrock messages own scheduling). |
| TUI fleet/observe/panes/files/launch/effects/project | `jackin-tui/src/*` | on main, superset. Fleet freshness is P07; role manifest reload is P08; picker file kinds are P18 (main `files.rs` already uses `stat_following`, lines 34 and 66). Back-pressure `queued` in `panes.rs` is on main (`panes.rs:79,104,115`). |
| CLI | `jackin/src/commands/{account,load,agent,attach}.rs`, `error.rs`, `main.rs` | on main, superset: serde `IssueView` replaces the hand-rolled `json_string`; `RestartError::Gateway(Cancelled)` exit class (`error.rs:184`); interrupt to cancel (`main.rs:98`). Unsaved-workspace note is P10. |
| Testkit and `junie_tui` console fixtures | `jackin-testkit/src/*` | excluded: Termrock UI owns the console (section 3); `junie_tui` pins are stale. |
| xtask e2e steps, scenario, baseline gate | `crates/xtask/**` | on main or excluded: Codex-only steps generalised; `JUNIE_TUI` baseline pin replaced by `TERMROCK`; the J15 `unwrap_or_default` fail-open (all 14 trees still have it) is fixed on main by `3bb5151fb` (`j15.rs` has 0 `unwrap_or_default`): P20. |
| `jackin-session/examples/*` | live drivers | on main with the same function sets; not product behavior. |

## Snapshot deltas: the 173 absent-from-both lines, by ref

All read. All are superseded design iterations; none is a missing invariant.

- `62b483ee4` (12): old Codex-only `system_config`/`code_mode_host` test constants, TUI `agent()` projection and `admit` call shape, a `bindings.cursor` schema-test variant (main has the `bindings.zai/fooai` cases).
- `8c427113e` (20): single-agent `AGENT_*` build args (23/13 count assertions); main builds per-agent `AGENT{n}_*` args.
- `e1dceb179` (18): per-agent `AGENT_<NAME>_VERSION/BINARY/SHA256_*` args; same function on main as `agent_stage`.
- `1dbe3f65a` (25): the OMP key text-scan hack (`extract_omp_api_key`) and an OMP YAML config form; main reads the SQLite store through `rusqlite`; bounded capture is P01.
- `16005c405` (19): `sync_locked_c` without an `agent` argument (older signature of the same rollback code, P16).
- `15c12d19e` (4): `rusqlite::Connection::open_with_flags(READ_ONLY)` (on main, `credentials.rs:223`) and a Z.AI endpoint constant (on main in `agent.rs`, T09).
- `1c1cabdae` (3): scan-absent plumbing, on main (`scan_absent_dockerfile`). `8d1b528a6`, `86fc1d637`, `5e1f8c2db`: 0 lines.
- `c5a98a087` (54), `1190c7efa` (15), `43fc09eea` (2): the `setsid`/`nohup`/`mkfifo`/`script -qec` non-lead start design with `shell_quote` and run directories. The line itself later dropped it and main replaced it (instance row above).
- `f424cce60` (1): a re-export line.

## P02 / P04 / P16 / P20 check (the earlier helper's (c) items)

Searched all 14 trees. `jackin-core/src/fs.rs` has no `O_NOFOLLOW`, `custom_flags` or anchored open (only the one `to_string_lossy` in `list`). `jackin-session/src/accounts.rs` has no `WriteEffect`, `ProviderWrite` or `compensate`. The OpenRouter provider has no exact-decimal path. `xtask/src/e2e/steps/j15.rs` has `unwrap_or_default` in all 14. No ref carries a better version of P02, P04, P16 or P20; those tasks proceed from the plan's section 1 donors, not from this lineage. (`010238a10`, the P16 donor commit, is reachable from the Codex checkpoints and is not in this chain's tree.)

## MISSING-VALUE items

None. Nothing in the 14 refs is an invariant missing from `origin/main` that is not already a P01-P21 task or an excluded category.

## Residual risk

- Exact-line containment cannot prove that a main refactor kept every sub-condition. The high-risk groups (guarded purge, role hashing, redaction, pagination, placeholder, instance label, export hash, account gating) were therefore each confirmed with a symbol or test on main, listed above.
- Reachability: the shared chain survives through the Codex checkpoint donors, which the plan keeps until P01-P21 close. If those donors are deleted later, the chain remains only via `preserve/pr1264-merge`, `preserve/stash-w8-port-feature-openshell-native` and `preserve/t3/*`. That is a separate retire decision.
