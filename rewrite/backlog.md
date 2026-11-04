# Backlog: LOW findings deferred under the lean process

Each row is a LOW finding that a review raised and the orchestrator deferred. A later batch may pick a row up; none blocks delivery.

| Source | Finding | Suggested fix |
|---|---|---|
| PR 1264 security U4-2 | Sandbox branches that differ only by case or Unicode normalization make every export fail on a case-insensitive host filesystem (the agent blocks only its own export). | Refuse or rename colliding sandbox branch names before the update-ref transaction. |
| PR 1264 security U4-3 | `rev-parse --verify` on a missing ref falls through to other namespaces; not exploitable. | Use `show-ref --verify` for exact ref reads. |
| PR 1264 security U7 | The usage lock file follows a symlink and creates a dangling target; needs write access to the 0700 state dir. | Open the lock with O_NOFOLLOW. |
| PR 1264 U6 residual | A plan that is offline at planning time takes no lock; a concurrent change of the account to a ChatGPT login can leave a provider (console only). | Re-read the config under the account lock for every plan. |
| PR 1264 U6 residual | `account add` connects every configured gateway, so a slow peer slows an add that needs one gateway. | Connect only the gateways the plan touches. |
| PR 1264 U4 | Repository settings such as a clean filter or `info/exclude` can still hide work from the purge check (documented in D08). | A temp-index `write-tree` comparison. |
| Secret scanning | The Kingfisher baseline is line-number fragile; any edit to a flagged line breaks the check. | Rewrite the credential-shaped fixtures so no baseline entry is needed. |
| Secret scanning | `check:secrets-scan` reads a real `target/` when run locally. | Scan the index or a git archive. |
| Leak-check fixtures | `jackin-openshell/tests/gateway/exec.rs:58`, `providers.rs:50`, `connect.rs:28` use the old fixture style (3-character windows of letters and digits). | Convert to the symbol-in-every-window style (`rewrite/reconciliation/leak-flake.md`). |
| Leak check | `lifecycle.rs:181` runs after the provider is deleted, so it never covers the stored provider record. | Move the check before the delete. |
| Baseline scanner | A literal split with `concat!` evades the demo-literal scanner. | Scan expanded string constants, or accept and document. |
| Baseline B2c | Motion, theme and clock `AppOptions` are pinned only by the functional baselines, not by `entry.rs`. | Add entry rows. |
| Baseline B3b | `command_unavailable` is unreachable through the app; pinned as data only. | Remove it or reach it. |
| Velnor | velnor-actions 0.1.72 hard-codes jactionlint 2.3.0; has no grouped-job option; accepts directory check inputs. | Upstream fixes in tailrocks/velnor-new. |
| Architect role | The `task-jackin-pins` CI job follows a mutable jackin branch, so a jackin tool bump turns role PRs red until the pins are copied. | Accept; bump role pins with jackin bumps. |
| PR 1264 N6, N19, N23 | Non-UTF-8 paths and branch names are decoded lossily (`config/render.rs:17`, `facts.rs:98`, `export/host.rs:159`, `export/sandbox.rs:340`); Linux only. | Refuse non-UTF-8 in `HostPath::parse` and the branch parsers. |
| PR 1264 N7, N16 | `Session::power`, `write_file`, `read_files` and `work_state_of` (`export.rs`) mint their own `CancellationToken`, so the console cannot cancel them. | Thread the caller token; add an xtask check against `CancellationToken::new()` in non-test source. |
| PR 1264 N8 | Two concurrent `load --launch-id X` can both run `setup`; D11 idempotence limits the harm. | Hold `lock::hold` around the hook step. |
| PR 1264 N10 | A failed provider update leaves the provider without refresh. | Make the Present check read the refresh status so the next launch repairs it. |
| PR 1264 N14 | `list_by_selector` drops malformed sandbox records. | Return the parse error. |
| PR 1264 N15 | `EngineCert::check` is syntactic (P5.4 design); a mismatched cert fails at the first `docker` call. | Parse X.509 and match the key. |
| PR 1264 N24 | `read_chatgpt_login` uses an unbounded read. | `read_regular` with a credential-sized cap. |
| PR 1264 N29 | An in-image symlink on a `read_write` path is not refused by the derived scan. | Pass the policy `read_write` list to the scan. |
| PR 1264 N30 | `--env` and manifest defaults may hold `op://` or `openshell:resolve:` values. | Run `PlainEnvValue` validation in `resolve_env`. |
| PR 1264 wave 2 LOW-1 | U4b tests have no `refs/headsx/...` target, so a prefix check without the slash passes. | Add the row. |
| PR 1264 wave 2 LOW-2, LOW-3, LOW-4 | U8: the recheck-before-bundle order is not pinned; no linked-worktree-with-alternates row; the hostile tests drop the alternates. | Add the rows; keep the alternates in `hostile_launch`. |
| PR 1264 wave 2 LOW-5 | U10: no CLI row sends a lone Esc key. | Add a CLI row that expects `\x1b` in the main input. |
| PR 1264 N31 residual | Same-account stale overwrite in the usage cache. | In `Cache::save`, keep the entry with the larger `good.fetched_at`. |
| PR 1264 N36 residual | Host worktree occupancy is checked only before the download (`export.rs`). | Run the worktree half of `guard()` again just before `publish`. |
| PR 1264 wave 3 F2 | The final-tag lock is exclusive and taken before the cache lookup, so launches of one image take turns until the first is `Ready`, with no progress event. | A shared lock for launches, exclusive for `--rebuild`; emit an `ImageEvent` on a lock wait. |
| PR 1264 wave 3 F3 | `git remote set-head origin --auto` fails on a cached load when the origin HEAD dangles, even for a rev-pinned role. | Ignore the set-head failure when the role pins a rev. |
| PR 1264 wave 3 F5 | Role `a` locks `roles/a.lock`, which is the clone path of a role named `a.lock`. | Put locks in a separate directory. |
| PR 1264 wave 3 F7, F9 | An existing 0755 `roles/` is not repaired; `role check` skips the size and name checks of `load`. | Repair the mode; share the load checks. |
| PR 1264 wave 3 F10, LOW-4 | `power` and the `find`/`check_builder` path observe the token only in some calls; `FakeEngine` ignores the token. | Thread the token (cancellation family with N7, N16). |
| PR 1264 wave 3 LOW-1, LOW-2, LOW-3 | No row pins "staged bytes equal hashed bytes"; the N21 row asserts only the parsed policy; the cache-hit final-tag hold has no row. | Add the rows (patch in the wave 3 red/green report). |
| PR 1264 wave 3 LOW-8 | The manifest, policy, Dockerfile and hooks are read twice (`inspect`, then `snapshot`); the hashed bytes are the staged bytes, but parsing reads separately. | Parse from the snapshot bytes. |
| Baseline B6 review | The sim's account-removal scrub of workspace account references has no test. | Add a row in `tests/effects.rs`. |
| Baseline B6 review | `AccountsScreen.saving` never clears without a reply; a late success closes the top layer. | Clear on a timeout or on any reply; close only its own layer. |
| Baseline B6 review | `perform` and `settle` are two parallel matches; an effect in one only never replies. | One table, or an exhaustive check that each effect settles. |
| Baseline B5b review | `World::workspace_repo` needs a launched instance, so a new workspace has no repository link. | Carry the repository on the workspace record (J2 origin). |
| Baseline B6 red/green LOW-1 | The capsule still writes `last_seen_secs` every tick (`capsule.rs`, `app.rs`); no test pins the sim ownership while attached. | Move it to the backend and add the attached row. |
| Baseline B6/B8a red/green LOW-2..5 | Usage refresh not driven by a test; `Clear pane` menu item, the shell `pwd` reply and one double-close break are unobserved. | Add the rows. |
| Baseline B8a review | `ViewportOps` doc names the Capsule; `BackendSource` only passes calls through; the `y` copy key ignores `keyboard_copy()`. | Tidy in B8b. |
| Baseline B7a review L3 | A `Ready` for an instance that is not Running strands the cockpit (no Cancel, `x` inert). | Treat it as a failure reply with a visible reason. |
| Baseline B7a review L4 | A Blocked launch cannot be cancelled with `x`, though the status line promises it. | Route `x` to `LaunchFeed::cancel` while Blocked (J4). |
| Baseline B7a review L5, L6 | First-session daemon code exists twice (`app.rs`, `manager.rs`); the attach builds a daemon only with a cockpit page in the stack. | One helper; record the limit in J4. |
| Baseline B4c review L7 | `env_edit_block` is called only by the fixture, so jackin's projection can diverge. | Call it from `openshell::account` (J2). |
| Baseline B4c review L8, L10 | Fixed gating uses the optional `short` text; the wizard destination radio is disabled with no reason when `fixed` is absent. | Gate on the `Reason`; always carry a reason. |
| Baseline B4c review L9 | The `ChoiceDialog`, `FormDialog` and `FileBrowser` fixes are not shown in the showcase (AGENTS.md). | Add showcase pages without moving approved baselines. |
| Baseline B4c UX F3 | With the fixed Read-only box focused, the browser footer still shows `Space Choose`. | Hints follow focus. |
| Baseline B4c red/green | The B4c PTY tests lack T21 assertions 3, 4, 6 and the unrelated click of 7; one always-true assertion (`jackin.rs` destination test); `ReadyInstance.run_id` unused. | Add the assertions; drop the field or use it. |
| J1 review | C1 misses `use ::ratatui::…` and `set_*` calls on receivers not named `buffer`. | Match by path resolution, not text. |
| J1 review | The refresh flag stays set after a panic; `config.load()` and `role_docs` run outside the timeout; the role manifest cache never refreshes and caches errors. | Guard the flag with a drop guard; time the whole step; key the cache by file state. |
| J1 review | Instance status maps `inspect` errors and transitional phases to "restore available"; ambiguous data is shown as a fact (Codex mode `Sync`, missing default gateway as engine Missing). | Map to an unknown state with a reason. |
| J1 review | No test pins that an account never shows a credential; `demo-gate` passes silently when the measured packages are missing; `screen.rs:180` panics on truncated source; `JACKIN_NO_MOTION=00` differs from the baseline rule; one weak assertion in `fixture_mode.rs`. | Add the test; fail when packages are missing; return an error; share the rule. |
| Process | The J1a and J1 merge commits (ca58042b9, 36490896c) lack `Signed-off-by`; pushed, so not rewritten. | Merge with `--signoff` from now on. |
| J1 fix round | The work-state cache misses a host-side commit or push while the sandbox state is unchanged, until the sandbox changes or the console restarts; the work-read bound and the uncached Unknown result have no test. | Key the cache on the host branch tips too; add a concurrency gauge to the fake. |
| J2 | `FileSource` is synchronous, so a slow host listing blocks the key handler. | Make the listing an effect with a reply, like `ProbeFolder`. |
| J2 | A rename reads the saved entry just before `edit`, so a change by another handle in between is lost. | Read inside the edit closure. |
| OpenShell 0.1.7 R1 | 0.1.7 admits Stop on a Completed or failed-main sandbox and can restart it; jackin still refuses (D09, `lifecycle.rs`) and the live contract check `ended_main_process_stop_start` was not rerun on 0.1.7. | Rerun the live contract suite; if restart works, simplify D09 and `Session::power`. |
| Testkit | The fake gateway pipes stdin to `/bin/sh`, which is dash on Linux; a test that sends a command and its data in separate writes can hang there (fixed for the OSC 52 test only). | Make the fake shell read line by line, or document the rule in the testkit. |
| P10.2 residual | `gateway add` takes no account lock, and a new login account not yet in the config is outside the set `gateway rm` locks, so a `gateway rm` racing an `account add` or launch of a new login account can leave its provider on the removed gateway. | A gateway-set lock. |
| Load-sensitive tests | `process_runner a_128_mib_line_does_not_grow_process_memory` (60 s git timeout) and `edit after_any_two_account_edits` (10 s guard) fail at load average about 80 and pass alone. | Scale the guards to measured work, or quarantine with the cause. |
| Baseline B8b review | The `drain` budget is not enforced and has no test; `PaneCommand::Detach` is never constructed; server notices keep bidi controls; `ViewportOps::draws_track` is a new required method on a public trait. | Enforce and test the budget; construct Detach in J5; strip bidi; give the method a default. |
| Baseline B8b UX L1-L6 | Attaching hint hides that `Ctrl+B x` closes the pane; the viewing look repeats itself and shows a cursor; the detached note is dimmed; the Codex close dialog says "detaches"; the shell close says "Unsaved state"; the info label could be `Program title`. | Apply the review wording. |
