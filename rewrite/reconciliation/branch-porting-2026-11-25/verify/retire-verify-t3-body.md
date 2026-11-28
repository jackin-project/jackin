# Retire verification, body-level pass: 21 `preserve/t3/…` refs, `preserve/stash-w8-port-feature-openshell-native` (+ HEAD parent `7bd1b7036`), `preserve/pr1264-merge`

Verifier: Sonnet 5.5, read-only. `origin/main` = `85a921a93` (SSH fetch failed as expected; the HTTPS-rewrite fetch ran and moved no ref that matters here). Plan: `jackin-branch-porting-plan.md` section 3 (preserve), section 5 (P01-P21), section 6 (rejected restorations, cited where the user's "section 3" list is not the source of the rule). Scripts and raw data: `.../manifest/t3body/` (`union.py`, `atrisk.py`, `inter.py`, `inter2.py`, `gg.sh`, `show.py`, `U.pkl`, `inter-all.pkl`).

## Verdicts

**SAFE 23 | VALUE 0 | UNCERTAIN 0 | MISSING-VALUE 1 (test-only, low, non-blocking, MV-1).**

The previous UNCERTAIN class is closed. The residual was not 300 lines of unclassified product code. The body-level pass shows the residual is a superseded older design of code that main has since rewritten and hardened, plus one regression-test gap (MV-1) that does not change any ref verdict.

## Structural findings (they change what "deleting these 23 refs" can lose)

Measured with `git rev-list <ref> --not <other refs>`, `git cherry`, `git diff <parent> <ref>`.

1. **Every one of the 21 t3 refs holds exactly one commit that no other ref has: its own tip.** For t3-0, 1, 2, 10-16, 19-25 the tip tree equals its parent's tree (`git diff <parent> <ref>` is empty). For t3-26, 27, 28 (parent `7bf7fe261`) and t3-29 (parent `589b3b6f4`) the tip adds only `rewrite/GOAL-ALL-AGENTS.md` (+164 lines; blob differs from main's rewritten file `cefd3dac1`, which #1292 updated; docs, excluded).
2. **Parents of 17 t3 refs (`5076ca2a0` T-C, `4691c0089` T-A, `573c3a09e` T-B) are ancestors of two kept `PORT-THEN-RETIRE` refs:** `codex/openshell-verified-checkpoint-20261108` and `codex/openshell-integration-checkpoint-20261108-122311`. So T-C/T-A/T-B content survives deleting these refs.
3. **`7bf7fe261` (T-D), `589b3b6f4` (T-E), `7bd1b7036` and `bbe6b5bb7` are NOT ancestors of any KEEP/PORT/HOLD ref.** They are held only by retire-class refs (these 23, `sup019/fix-1264`, `sup020/…`, `swarm/*`, `wt/a11b…`, `wt/a14-*`). Deleting the 23 refs alone loses nothing, but finishing the whole retirement makes 151 commits (142 non-merge) unreachable (`git rev-list <23 refs> --not main <6 PORT refs> docs/debris-ratification-2026-11-14`). Because of this, T-D/T-E/pr1264 content was the focus of the body-level pass below, not just the T-C lines.
4. The stash `f2da2d215` has 2 parents only (HEAD `7bd1b7036` and index `e3781270a`, both tree `28c846980`): **no untracked third parent**. Its worktree delta vs `7bd1b7036` is 8 paths.
5. `bbe6b5bb7` (pr1264-merge) is the only commit unique to that ref (parents `aa218ea83`, `9792bdd88`, both reachable from other refs).

## What was compared

| Set | Size | How |
|---|---|---|
| A. Lines added by T-C/T-A/T-B/T-D/T-E/`7bd1b7036` (merge-base `470fc5d6f`) that are absent from the same file on main, non-excluded jackin crates (excl. `.github .velnor docs rewrite mise Cargo xtask usage testkit vendor .lock .md .toml .json`) | 2172 lines in 150 files | `union.py` over the six `novel-*.tsv` files |
| A1. of A, only in T-D/T-E/`7bd1b7036` (not in the kept ancestors) | **393 lines in 39 files** | `atrisk.py` |
| A2. of A, also in T-C/T-A/T-B (held by kept checkpoints) | 1779 lines | read per file, see tables |
| B. Intermediate-only lines: added by one of the 142 non-merge commits unique to the 23 refs, absent from main, `7bd1b7036`, `bbe6b5bb7`, `f0021a0`, `589b3b6f4`, `7bf7fe261` | **467 lines in 38 files** (plus 3070 lines under `.audit/`, excluded: "do not import `.audit` trees", section 3) | `inter2.py` |
| C. pr1264 intermediate lines absent from main | 799 (the earlier 632 was a tree-wide count) | earlier `pr1264-absent.tsv`, re-read |

Read in full: A1 (393), B (467), the product files of A2 (plan, run, multi, multi_session, files, native, lifecycle, credentials, accounts, roles, source, op, host, attach, clipboard_writer, bundle, core/fs/wait/ids/paths/instance, cli/load/account/error, image/plan, jackin-start, openshell grpc, tui backend/effects/files/project/fleet/panes). Main-side evidence is `git grep` on `origin/main` (`gg.sh`), cited as `file:line`.

## Hunk-by-hunk disposition (residual group first)

Legend: ON-MAIN = same or stronger behavior on main; EXC-n = excluded by plan rule; Pxx = covered by task.

### `jackin-session` plan / launch / multi-agent

| Snapshot hunk | Disposition | Evidence |
|---|---|---|
| `plan.rs`: `PlannedFiles{codex_config, auth, muse_trust, omp_models}`, `lead_tuple`, `muse_trust_file`, `plan_omp_models`, per-agent `agent_config_file(is_omp, …)` (16 D-only lines) | ON-MAIN, redesigned per instance | `plan.rs:93` (`PlannedFiles{static_files, env, multi, layout}`), `plan.rs:570` `agent_config_file`, `plan.rs:643` `plan_static_files` (config + auth + omp models + muse trust for every instance, relocated to home), `plan.rs:527` `validate_instances`. Tests `plan.rs:1108 muse_trust_file_only_for_muse`, `plan.rs:1155`, `tests/plan/omp_models.rs:148,164,227`. |
| `plan.rs` T04 `close policy for all instances` + Muse root read-only (intermediate `3f07774c2`) | ON-MAIN, stronger | `plan.rs:409-431` `launch_policy` calls `close_for_instances` and `with_read_only("/")` when Docker is off and a Muse instance exists; tests `tests/plan/policy_root.rs:55,94`. |
| `plan.rs` intermediate A12 unit tests (165 lines, `914d8e8ef`): Claude lead slot writes `/sandbox/.claude/settings.json` 0600 and no home copy; non-lead Claude gets `/sandbox/.jackin/homes/claude/.claude/settings.json` and the lead config stays | ON-MAIN | `plan.rs:1203 agent_config_claude_lead_slot_pins_the_settings_file`; `tests/launch/multi_instance.rs:446`, `tests/launch/static_files.rs:146-170`. |
| `plan.rs` `resolve_instances` default branch `agent/<name>`, `collect_instance_accounts`, `lead_chatgpt_account` | ON-MAIN | `plan.rs:754,691,726`. |
| `plan.rs` `TMPDIR=/tmp` default (intermediate `7f02be3a2`, `0ede60f68`) | ON-MAIN | `plan.rs:434-445` `apply_agent_env_defaults` (MiMo). |
| `files.rs`: `render_kimi_config(&ProviderName)` with `api_key_env = "KIMI_API_KEY"` and `placeholder`; `render_mimo_config()` (xiaomi baseURL `token-plan-sgp`, `{env:MIMO_API_KEY}`); `render_native_config`; `grok_auth_file(issuer)` `{issuer:{key:placeholder}}`; `codex_home_config_file` | EXC (section 3: no promotion of T04/T06/T07/T08; Kimi managed OAuth decision, `rewrite/decisions.tsv` 2026-11-04, #1296) and ON-MAIN where behavior is general | `files.rs:259` `render_kimi_config(region)` (regional, OAuth), `plan_static_files` returns empty for T06/T07/T08 (`plan.rs:~655`), mimo `mimocode.json` path only in the spec (`agent.rs:863`), `tests/plan/native.rs:12,84,121`. Grok moved to external-session helper config (`tests/plan/grok.rs:35,54,85`). Codex per-instance home config: `tests/plan/codex.rs:111,161`. |
| `files.rs` `render_claude_config(tuple)` (base URL only, no model, no auth env) and `claude_config_file(tuple)` | ON-MAIN, stronger (exact model, typed unknown-model error) | `files.rs:224`, `tests/plan/claude.rs:70,107,227,259,298`. |
| `launch/run.rs`: placeholder `printenv` validation (`PlaceholderMissing/Invalid`, `starts_with(PLACEHOLDER_SCHEME)`, no `\n`) | ON-MAIN | `run.rs:62,66`; tests `tests/launch/credentials.rs:215,232` (`t01_missing…`, `t01_malformed_placeholders_fail_without_echo_or_auth_state`, key name only in the error). |
| `run.rs`: writes of `files.codex_config`, `files.auth`, `files.muse_trust`, per-instance omp auth/models and Claude config | ON-MAIN (per-instance static files) | `plan.rs:643`; `tests/launch/static_files.rs`, `tests/launch/omp_models.rs:90,109,152,165`. |
| `run.rs`: `config.workspace(workspace).is_none()` fence | ON-MAIN, stronger | `run.rs:463-475` (`WorkspaceRemoved` and `WorkspaceReplaced`); `tests/launch/workspace_fence.rs:83`. |
| `run.rs` `sync_provider(...)` rollback and provider sync | P16 (and on main `AccountError::RollbackFailed`) | `tests/accounts/failed_add.rs:132,168`. |
| `multi.rs`: non-lead codex home config via `codex_home_config_file(tuple, model, workdir)` and `TupleNotSupported` on unknown tuple | ON-MAIN | `plan_static_files`; `tests/plan/codex.rs:111`. |
| `multi_session.rs`: `git worktree add -b` after `mkdir` of `rsplit_once('/')` parent; `pkill -x <agent>`; `agent_names` JSON text scan; `HeadState::Born/Unborn` probe | ON-MAIN, replaced and hardened | `multi_session.rs:44-110` `WORKTREE_PREPARE` (component-by-component `cd -P`, symlink and admin-dir checks, rebuild path); instance stop/kill by incarnation (`tests/launch/instance_identity.rs:779-1055`); `sandbox_agent_names`. |
| `lifecycle.rs` guarded purge `created_at` compare; provider-rule and profile asset derivation per agent | ON-MAIN | `lifecycle.rs:165-175` (`None` on either side counts as replaced), `lifecycle.rs:230-285`. |
| `credentials.rs`: `read_host_auth` login errors `LoginUnreadable/NotRegular/TooLarge`; Grok `{issuer:{key}}` session key; OMP `$HOME/.omp/agent/agent.db`; 1Password static-key `op read --account`; `parse_claude_keychain_secret` returns bare token | ON-MAIN or superseded | `credentials.rs:170-280` (errors), tests `tests/accounts/login.rs:73-181`; Grok full OAuth grant `credentials.rs:885-917`; OMP `jackin_core::OMP_HOST_STORE` + `read_omp_host_key` (P01 bounds); 1Password accounts are usage-only on main (`config.rs:593-602` `is_usage_only`, `check_launchable`), so `resolve_static_key` returning `MissingStaticKey` for them is deliberate (#1296); Claude login keeps the whole grant (`credentials.rs:957-1005`). |
| `accounts.rs`: `sync_if_absent`, `forget_locked`, `holds`, `plan_account_edits_inner(agents)`, `publish_static_login`, `forget_login` over `Agent::ALL` | ON-MAIN, generalised; rollback gaps are P16, gateway scope is P13 | `tests/accounts/edit.rs:443,465`, `peers.rs:163-626`, `stale_launch.rs`, `lock.rs`. |
| `roles.rs`: `ls-files -z -co --exclude-standard` (ignored files NOT hashed); `origin/{rev}^{commit}` | opposite decision on main (ignored files ARE hashed, stricter) / P12 | `roles.rs:625` uses `ls-files -z -co`; tests `a_dirty_role_tree_hashes_tracked_untracked_and_ignored_files`, `a_git_ignored_role_input_is_hashed_and_staged`. P12 owns revision checkout. |
| `source.rs`/`op.rs`/`tui/onepassword.rs`: 1Password `section` disambiguation | ON-MAIN | `op.rs` `section: Option<…>`, `source.rs` match on `field.section`. |
| `export/bundle.rs`, `examples/export_memory.rs`: `Sha1`/`Sha256` hand hashers | ON-MAIN, replaced | `gix_hash` trailer check, `tests/export/bundle_limits.rs`. |
| `host.rs` `chrono::Local…local_minus_utc`; `clipboard_writer.rs` `Instant` deadline | ON-MAIN, replaced | `host.rs:47` (`jiff`), `clipboard_writer.rs:139` (`clock.monotonic()`). |
| `tests/launch/credentials.rs`: Codex `auth.json` shape, `base64url` id_token decode, `last_refresh` from clock | **MV-1** (see below) | |
| `tests/launch/provider.rs` missing-login launch | ON-MAIN at the reader and launch-wrapper level | `tests/accounts/login.rs:73-181` (`LoginUnreadable/NotRegular/TooLarge`), `tests/launch/provider.rs:9-45` (`ProviderUnavailable` before sandbox, no provider created). Only the pairing "ChatGPT login file gone at launch" is not one test; not value. |
| `tests/launch/multi_instance.rs`, `instance_identity.rs`, `worktree.rs`, `fence.rs`, `upload_timeout.rs`, `omp_models.rs` (Omp/codex variants, `agents_label`, `agents_json`, `agent/omp` branch, `pkill -x omp`) | ON-MAIN, richer | `tests/launch/instance_identity.rs:97-1055` (non-lexical lead attach, legacy sorted identity refused, `lead-first-v1`, stale/reused identity refused), `multi_instance.rs:170-581`, `tests/launch/multi_instance.rs:650` (`agent/omp`). |

### `jackin-core`

| Hunk | Disposition | Evidence |
|---|---|---|
| `agent.rs` / `tests/catalog.rs` (75 D-only): tuple states and reasons (`vendor clarification pending`, `uppercase MiniMax-M3 live proof pending`, `no API key on this host`, T13-T22 table, `TUPLES: [..;12]`), per-agent specs (kimi/mimo/antigravity pins, `.mimo`), `ANTHROPIC_API_KEY` env for T13 | EXC-D29 (section 3: do not promote or re-label tuple states from old logs). Main: 22-row matrix, T13 uses `ANTHROPIC_AUTH_TOKEN` | `tests/catalog.rs:223,257,281,296,310`; `assets/t13-claude-kimi.yaml:12`; `agent.rs:1148-1176` (T21 Supported, T22 Unavailable). Typed `UnknownModel` and exact-model acceptance covered at `catalog.rs:236-272,327`. |
| `config.rs`: `omp_api_key()` / `has_provider` for Claude/Codex API-key accounts only holding an OMP provider; HostLogin arms | ON-MAIN, generalised | `config.rs:553-640` (`auth_class`, `publishable_tuple`, `has_provider`, `is_usage_only`). `peers.rs:180` now asserts no omp provider for a Codex+file account because T22 is Unavailable (D29). |
| `instance.rs` `sorted()` label hash, `tests/agent_instances.rs` goldens (`235f8027…`, `ac08b369…`) | superseded | main hashes `lead-first-v1:` preimage with lead first (`instance.rs` `canonical`), tests `tests/launch/instance_identity.rs:289-330`. |
| `fs.rs` closures, `wait.rs` elapsed budget, `ids.rs scp_form`, `paths.rs ProjectRoot(HostPath)` | ON-MAIN, stronger | `fs.rs:117` `DirBuilder…mode(0o700)`; `wait.rs:52-77` budgets `waited+probed`; `ids.rs:329-355` rejects `?#`, options, `://`; `paths.rs:199-213` verified root (P04 extends). |
| `manifest.rs`, `config/edit.rs`, `select.rs`, `render.rs`, `store.rs`, `labels.rs`, `boundary.rs`, `provider.rs`, `tests/config_schema.rs`, `tuple_account.rs` | ON-MAIN (formatting or provider-generic) | spot-checked; no behavior in the added lines absent from main. |

### CLI, image, openshell, tui

| Hunk | Disposition | Evidence |
|---|---|---|
| `commands/load.rs` `json_string` text parser for `gh issue view`, `--task-file` read, cancel messages, `--*-for` | ON-MAIN, replaced | `load.rs:366-403` typed `IssueView`; `load.rs:280-310` bounded `read_task_file` with cancel; `load.rs:99,102`; `cli.rs:619-631`. |
| `commands/account.rs`: `--credential env:NAME needs a set variable`, `account default`, role bind only Codex, `unknown_account` | ON-MAIN, generalised | `account.rs:162,306,317`; `error.rs:48` `ApiKeyNeedsAgent`. |
| `error.rs` cancelled exit class for `RestartError::Gateway(Cancelled)`; `main.rs` interrupt | ON-MAIN | `error.rs:184-188`. |
| `jackin-image/src/plan.rs` per-agent stages, `--same-as ${AGENT<n>_BINARY}`, `_SYSTEM_CONFIG` | ON-MAIN | `plan.rs:193-270` (`AGENT_STAGE`, scan anchors); final-image alias validation is P15. |
| `images/jackin-start` `record_codex`, env-kinds record v1 | ON-MAIN, generalised | `jackin-start:124-179` `record_agent`, `jackin-env-kinds 2`; tests `jackin_start.rs:247-526`. |
| `jackin-openshell/src/grpc/provider.rs` redactor wrapping; `grpc/sandbox.rs` `MAX_PAGES`, `created_time`, `page.next_page_token == token`; `connect.rs` TLS message | ON-MAIN | `grpc/provider.rs:48,257,293`; `sandbox.rs:25,135,269-291` (`issued` set, token length cap: P14 landed). |
| `tui/backend.rs` `Refreshes`, `Cause`, `Refreshing` RAII guard (panic releases in-flight), `a_panic_while_refreshing_lets_the_next_refresh_start` | **EXC** (section 6, "a second console refresh scheduler from the checkpoint"); panic completion is reported by main's bridge | `backend.rs:195-249` (`RequestRefresh`, `crashed::refresh(ticket)`), `crashed.rs`. |
| `tui/effects.rs` (`Cannot start sandbox … while …`, `AccountPlan::Offline`), `console.rs` (`junie_tui`), `launch.rs`, `fleet.rs unreadable`, `project.rs` `sandbox_id`, `files.rs`, `panes.rs`, `bridge.rs`, `usage.rs`, `persist.rs`, `crashed.rs` | EXC (Termrock owns the UI; `junie_tui` is retired) or ON-MAIN | unreadable sandboxes: `effects.rs:611-617`, `project.rs:678`; picker file kinds: `files.rs:34,66`, `effects.rs:222,251` (P18 closed on main); back-pressure `panes.rs:79,86,104`; fleet freshness is P07. |
| `tests/launch/picker.rs`, `tests/console/*`, `tests/tie.rs`, `doctor.rs` | EXC (old UI) | |
| tests with `gpt-5.1-codex` (A/B/C) vs `gpt-6.1-sol` (D) | ON-MAIN (`gpt-6.1-sol`, `agent.rs:623`) | |

### Intermediate-only lines (set B, 467 lines in 38 files)

All from the A10-A14 commits `c539ca4f3 … 57c4d1f9a`:

| Lines | Disposition |
|---|---|
| `tests/plan/availability.rs` 131, `tests/cli/load.rs` 50, `tests/launch/credentials.rs` 23 (`t01_refusal_*`), `tests/plan/errors.rs` 8, `agent.rs` 29, `catalog.rs` 39, picker 57 | EXC-D29: per-tuple `unavailable`/`blocked` reason strings and refusal tests for T01/T03/T10/T11/T12/T13/T16/T20. Main has its own matrix: `tests/plan/availability.rs`, `catalog.rs:223`, `tests/plan/errors.rs:11-65`. Generic invariant "a refused tuple has no provider, sandbox or build effect" is on main as `a_wrong_provider_secondary_refuses_the_public_launch_path_without_effects` (`tests/launch/provider.rs`). |
| `accounts.rs` 8, `edit.rs` 2, `core/config.rs` 3, `cli/error.rs` 2, `host_login.rs` 2 | ON-MAIN: all-agent forget/hold, `ApiKeyNeedsAgent` (`error.rs:48`). |
| `multi_instance.rs` 11, `instance_identity.rs` 14, `agent_instances.rs` 9 | ON-MAIN: `failed_codex_provider_publication_cleans_up_only_codex_and_preserves_sibling_providers` (`multi_instance.rs:334`), integrate (`instance_identity.rs:514-650`), lead-first identity. |
| `effects.rs` 12 | EXC (Termrock wording). |
| remaining 70 lines (`plan/codex.rs`, `claude.rs`, `omp_models.rs`, `agents_manifest.rs`, `seams/build.rs`, `cli/build.rs`, `files.rs`, `lib.rs`, T13-T22 yaml hosts) | ON-MAIN or EXC (old catalog values, model strings, `--agent omp --provider claude` build selection now valid because T21 is Supported). |

No hunk in A1 or B encodes an invariant that is both absent from main and unowned by P01-P21 or section 3/6, except MV-1.

### `crates/usage` and `xtask`/`testkit`

Not re-read line by line. Checked the three behaviors the earlier report flagged as risky: OpenRouter `/credits` 403 leaves the key-limit window (`connected/openrouter.rs:58-70` `Err(Forbidden) => {}`, fixtures `credits_403.json`); 403 is `Provider` by #1296 (`dispatch.rs:188`); Retry-After HTTP date parsed (`http.rs:426`). Gemini removed by #1296 (EXC). Exact money = P02; OMP/OpenCode bounds = P01.

## MISSING-VALUE items

### MV-1 (test-only, low). The Codex ChatGPT placeholder `auth.json` shape is not pinned by any test on main

- **Invariant.** The `auth.json` the launch writes for a ChatGPT-login Codex account must be exactly `{auth_mode:"chatgptAuthTokens", OPENAI_API_KEY:null, tokens:{id_token (3-part JWT, header alg none), access_token = placeholder, refresh_token:"", account_id}, last_refresh = launch clock}`, mode 0600, with no extra keys. `chatgptAuthTokens` is what keeps Codex from running its own refresh with an empty refresh token.
- **Files.** Behavior on main: `crates/jackin-session/src/files.rs:73-90` (`render_auth_json`), `launch/run.rs:781`, `jackin-core/src/agent.rs:534` (`CODEX_PLACEHOLDER_ID_TOKEN`). Missing test: `crates/jackin-session/tests/launch/credentials.rs:179-211` (`t01_launch_writes_placeholder_auth_without_host_tokens`) asserts only `access_token`, `refresh_token == ""`, `OPENAI_API_KEY` null and no host token leak. A repo-wide `git grep chatgptAuthTokens origin/main -- crates/*/tests` finds no match; `xtask` j08 `judge_auth_text` uses behavior fixtures that do not name `auth_mode`.
- **Introduced by.** `git log -S'chatgptAuthTokens' -- crates/jackin-session/tests/launch/credentials.rs` gives `364b3314a` (2026-11-02, "test(session): prove the launch executor against the fake gateway"); code from `ddd296b40`. The test was in the T-C..T-E snapshots (all six), is not in the #1264 squash `f683619b5`, and survives on kept `origin/codex/openshell-verified-checkpoint-20261108:…/credentials.rs:63`, `preserve/w1-fixes-20261109`, `preserve/session-boundaries-20261109`.
- **Why it matters.** A drift of `AUTH_MODE` or of the key set would only show in a live Codex run.
- **Acceptance test.** Extend `t01_launch_writes_placeholder_auth_without_host_tokens`: parse the written file; assert the exact key set, `auth_mode == "chatgptAuthTokens"`, `tokens` has exactly 4 keys with `account_id == ACCOUNT_ID`, `id_token == CODEX_PLACEHOLDER_ID_TOKEN`, `last_refresh == <fake clock rfc3339>`, mode `0600`; mutation check: changing `AUTH_MODE` to `"chatgpt"` makes it fail.
- **Retirement effect.** None for these 23 refs (content also lives on the kept codex checkpoint). Record it in the work register before the last holder (`codex/openshell-verified-checkpoint`, `w1-fixes`, `session-boundaries`) is retired, or drop it consciously.

Candidates examined and cleared (not MISSING-VALUE): guarded-purge replacement check, worktree preparation, workspace-removed fence, 1Password key resolution (usage-only accounts), role-tree ignored files (opposite design), per-tuple refusal strings, Kimi/Mimo placeholder configs, refresh scheduler panic guard (section 6), peers `replaces provider` test (D29, T22 Unavailable), `ApiKeyNeedsAgent`, T09 `omp_base_url` (main `agent.rs:161` is the corrected anthropic endpoint of `436781af9`).

## `preserve/pr1264-merge`

`bbe6b5bb7` merges `9792bdd88` into `aa218ea83`; `9792bdd88` is an ancestor of the PR head `f0021a0` (tree = squash `f683619b5`). The 799 absent lines were re-read:

- 683 lines in hunks the 26 later PR commits replaced. Sampled `git diff 9792bdd88 f0021a0` for `commands/load.rs` (adds bounded task-file read with cancel, typed `gh` JSON, `--*-for` agent checks, `lead_provider`), `fs.rs` (named helper fns, pure refactor), `config.rs`, `credentials.rs`: every replacement is a hardening or refactor and lands on main.
- 114 pure-removal lines, product ones reconfirmed against main: `multi_session` parent-dir creation (now `WORKTREE_PREPARE`), `lifecycle` generated provider rules (`lifecycle.rs:234`), `load.rs` text JSON parsing (`IssueView`), `panes.rs` queued accounting (`panes.rs:104` `bytes.len().max(1)`), `bundle.rs`/`export_memory.rs` sha imports (gix-hash), `cli/usage.rs` 403 credits (deliberate Provider classification, `credits_403.json`). Usage-crate removals follow #1296.
- 2 fixture JSON lines: old Kimi schema.

**No value found.** Intermediate-only lines beyond the tree-level count (set B above) add nothing.

## Per-ref final verdicts

All 21 t3 refs are `refs/remotes/origin/preserve/t3/M2EzYzljMGI1OTE0Y2FhMzQ0YzFlMDA3YjNkNmEwNjc/<n>`.

| Ref | Tip | Parent / tree relation | Verdict | Reason |
|---|---|---|---|---|
| t3/0 | `f6c2bff00` | same tree as `5076ca2a0` (T-C) | SAFE | empty tip; parent held by kept codex checkpoints |
| t3/1 | `c9a4ad50c` | same tree as `4691c0089` (T-A) | SAFE | same |
| t3/2 | `d9c34f15f` | T-A | SAFE | same |
| t3/10 | `1e928c0af` | T-A | SAFE | same |
| t3/11 | `8d1e781bf` | T-A | SAFE | same |
| t3/12 | `10034b2a4` | T-A | SAFE | same |
| t3/13 | `52a1aabe0` | T-A | SAFE | same |
| t3/14 | `517097494` | T-A | SAFE | same |
| t3/15 | `7f1f0f92c` | T-A | SAFE | same |
| t3/16 | `7fc4466a9` | T-A | SAFE | same |
| t3/19 | `abe950fac` | same tree as `573c3a09e` (T-B) | SAFE | empty tip; T-B held by kept checkpoints; Git-role loading on main (`observe.rs`, P08 for local freshness) |
| t3/20 | `146052403` | T-B | SAFE | same |
| t3/21 | `e99b4a424` | T-B | SAFE | same |
| t3/22 | `d7a22844c` | T-B | SAFE | same |
| t3/23 | `cd4fc9ee4` | T-B | SAFE | same |
| t3/24 | `fe7c87c9e` | T-B | SAFE | same |
| t3/25 | `22cd55144` | T-B | SAFE | same |
| t3/26 | `d7a1c0ef1` | `7bf7fe261` (T-D) + `rewrite/GOAL-ALL-AGENTS.md` | SAFE | tip adds obsolete goal prose only; T-D product tree covered by the hunk tables (393 D-only lines, 467 intermediate lines, no value except MV-1 test gap) |
| t3/27 | `1ae7d7701` | T-D + GOAL | SAFE | same |
| t3/28 | `814b61d33` | T-D + GOAL | SAFE | same |
| t3/29 | `4f4ea8bf5` | `589b3b6f4` (T-E) + GOAL | SAFE | same; T-E differs from T-D by docs only |
| stash-w8 `f2da2d215` | HEAD `7bd1b7036`, index `e3781270a` | 2 parents, no untracked parent | SAFE | 8 paths: `mise.lock`, `mise.toml` pins (hk 2.12.0, mr-boxington 1.30.0, musl target), `rewrite/*` docs, one T21 reason string (`agent.rs:1157` "final live proof pending with Anthropic API key"; main now has T21 Supported and different text). All excluded. `7bd1b7036` product tree = T-E |
| pr1264-merge `bbe6b5bb7` | parents `aa218ea83`, `9792bdd88` | only unique commit | SAFE | see section above |

## Conditions and limits

- The three t3 groups T-D/T-E/`7bd1b7036` are reachable only from retire-class refs. Verdict SAFE refers to content, which I have compared; it does not say the commits remain reachable after the whole manifest is applied.
- Presence evidence is exact trimmed-line containment plus symbol, test-name and behavior checks on `origin/main` (not builds or test runs). Where main restructured code, the equivalent was read, not only grepped (worktree script, `launch_policy`, `lifecycle.rs`, `credentials.rs`, `peers.rs`).
- A2 (1779 lines also present in the kept T-C/T-A/T-B ancestors) were checked for the product files listed above; usage, xtask and testkit hunks stay at the earlier report's group level plus the three spot checks.
