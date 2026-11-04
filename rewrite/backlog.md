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
