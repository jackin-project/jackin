# TASK-011 gap audit

Read-only. Product code was not edited. `cargo` and `mbx` were not run. This audit does not approve TASK-011.

## Trees

| Tree | Path | HEAD |
| --- | --- | --- |
| Integration | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` | `738e2cfa95731b8ee59e705f52e38c802c12a3a8` |
| Main M | `/Users/donbeave/Projects/jackin-project/wt-baseline-M` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| Policy P | `/Users/donbeave/Projects/jackin-project/wt-baseline-P` | `fba69d60c2373156dc0dcecee8e085c83776966f` |

History reads used `git show` / `git grep` in `/Users/donbeave/Projects/jackin-project/jackin`. Nothing was checked out.

Source commit `d04467f05f1183008322047b1828c201e551e947` (`Add model and effort overrides to load`) trims the model, rejects an empty model, parses `low` / `medium` / `high` / `max`, writes absent `model_override` and `effort` as JSON null, and assigns both fields on `LoadOptions`. It does not itself implement restore. M is an ancestor of integration. P is not. M already trims at `crates/jackin/src/cli/role.rs:99-104` and refuses restore reuse when `opts.model` is set at `crates/jackin-runtime/src/runtime/launch/launch_pipeline.rs:1765` and `:1794`. Integration has the same behavior at the split leaves below. Line numbers are the integration tree.

## Verdicts

| Carry point | Verdict | Where |
| --- | --- | --- |
| Trim and reject an empty load model | ALREADY_PRESENT | `crates/apps/jackin/src/cli/role.rs:99-104`. Whitespace-only `"  "` fails in `crates/apps/jackin/src/cli/role/tests/case_01.rs:107-113`. A trimmed `"  provider/model-id  "` is kept at `case_01.rs:83-103`. Programmatic whitespace-only model fails as `EmptyModel` at `crates/services/jackin-runtime-launch-load-options/src/load_options.rs:343-344`, locked by `crates/services/jackin-runtime/src/runtime/launch/programmatic/tests/case_01.rs:347-353`. |
| Effort enum `low` / `medium` / `high` / `max` | ALREADY_PRESENT | `crates/core/jackin-core/src/reasoning_effort.rs:18-41` and `:77-80`. CLI parser delegates at `crates/apps/jackin/src/cli/role.rs:68-70` and `:107-109`. Help lists the four spellings at `crates/apps/jackin/src/cli/role/tests/case_01.rs:322-332`. Round-trip of all four is `crates/core/jackin-core/src/reasoning_effort/tests.rs:7-14`. Unknown `extreme` is rejected at `case_01.rs:107-113`. |
| Absent dry-run `model_override` and `effort` stay null | ALREADY_PRESENT | Keys are always emitted at `crates/apps/jackin/src/app/load_cmd.rs:796-797`. Absent values are null at `crates/apps/jackin/src/app/load_cmd/tests/case_01.rs:175-190`. An explicit pair is `case_02.rs:203-218`. Text prints the effective model and effort, including `default`, at `load_cmd.rs:933-948`. |
| Model and effort reach `LoadOptions`, not only the parser | ALREADY_PRESENT | `apply_load_model_effort` assigns both at `crates/apps/jackin/src/app/load_cmd.rs:195` and `:579-585`. Absent stays absent and an override is forwarded at `load_cmd/tests/case_02.rs:222-238`. Fields live on the load-options leaf at `crates/services/jackin-runtime-launch-load-options/src/load_options.rs:89-95`. |
| Launch carries both | ALREADY_PRESENT | Host launch copies `opts.model` and `opts.effort` at `crates/services/jackin-runtime/src/runtime/launch/launch_pipeline/launch_core/orchestrate/environment_prepare.rs:121-122` and `:163-164`. Capsule maps are filled at `orchestrate/helpers.rs:123-134`. The model override is trimmed again and fans out only to the selected agent at `crates/services/jackin-runtime-launch-capsule-setup/src/capsule_setup.rs:184-205`. Effort fans out at `capsule_setup.rs:213-224`. Fixture `capsule_setup/tests/case_01.rs:127-169`. Codex and Claude lane env is `crates/services/jackin-runtime-launch-load-options/src/lane_env.rs:32-50`, locked by `programmatic/tests/case_02.rs:49-81`. |
| Dry-run leaf uses the model override | ALREADY_PRESENT | `resolve_dry_run_model_projection` takes `model_override` at `crates/services/jackin-runtime-launch-dry-run/src/dry_run.rs:132-145` and calls the same capsule projection. The CLI passes `model.as_deref()` at `load_cmd.rs:164-169`. Effort is not a projection input; the app JSON still emits it. That is not a drop. |
| Restore does not drop a parsed model or effort | ALREADY_PRESENT | A set model or effort makes current-role reuse incompatible at `crates/services/jackin-runtime/src/runtime/launch/launch_pipeline.rs:1709-1716`, so the fast path is skipped at `:1200-1208` and the normal pipeline applies the values. Exact-container restore fails closed at `:1731-1749`. Tests `launch_pipeline/tests/restore_reuse_intent.rs:13-22` and `:46-62`. `related_restore_load_options` uses `LoadOptions::default()` for model and effort at `crates/services/jackin-runtime-launch-restore/src/restore.rs:220-234`. That call is only the `RebuildRelatedRole` arm inside the compatible-reuse branch, which already requires both fields to be `None`. A parsed override is not discarded there. |
| Programmatic account selection does not swallow the launch model | ALREADY_PRESENT | A synthesized configuration stores `model: None` at `crates/services/jackin-runtime-launch-programmatic-selection/src/selection.rs:234-241`. That is the account configuration pin. The launch override stays on `LoadOptions` and is applied later by `resolved_instance_models`. Replacing a hand-built configuration that already has the same account is refused so its model is not discarded (`selection.rs:23-26`). |
| Capsule session metadata | ALREADY_PRESENT | Spawn reads the launch-config maps at `crates/apps/jackin-capsule/src/daemon/session_lifecycle/spawn.rs:107-108`. `AgentRecord` stores session id, codename, instance, account, provider, and times only (`crates/apps/jackin-capsule/src/daemon.rs:211-225`). It does not store model or effort. `d04467f0` does not require those history fields. Child argv and env trim the model again and skip an empty value at `crates/apps/jackin-capsule/src/session/agent_cmd.rs:183-228`. |
| Restore the untrimmed `42c6f8ed` load `--model` | REJECT | `42c6f8ed91571e68c080c2ed7c6ac62a045d6553:crates/jackin/src/cli/role.rs:99-103` rejects a blank string and then returns `Ok(s.to_owned())`, keeping surrounding spaces. Integration stores `s.trim()` at `role.rs:100-104`. Launch and capsule trim again at `capsule_setup.rs:185-186`, `lane_env.rs:45`, and `agent_cmd.rs:223`. Do not put the untrimmed CLI string back on load or restore. |
| Add a CLI option the selected backend cannot honor | REJECT | `8dac3d6eb651c4515ffedeefa3f616b64b07e1bf` has `LoadOptions.prompt` ("Initial prompt handed to the agent's first session") at `crates/jackin-runtime/src/runtime/launch.rs:187-188` and treats `opts.prompt` as a restore-incompatible option at `launch_pipeline.rs:1768` and `:1796`. Integration `LoadOptions` has no `prompt` field (`load_options.rs:34-118`). Do not add that option. Amp has no model argv and no effort env: `agent_cmd.rs:190` returns an empty argv, and `lane_env.rs:41-43` returns no env. The amp fixture is `session/tests/case_02.rs:281-282` and `programmatic/tests/case_02.rs:85-88`. Do not add an amp flag for a knob that runtime does not honor. Exact restore already refuses a model or effort the existing container cannot apply (`launch_pipeline.rs:1744-1749`). |

No carry point is PORT. Parse tests and the launch and restore carry both exist. No carry point is BLOCKED: the leaf files were readable at the pinned SHAs.

## Not a product edit

`d04467f0` is the parser, dry-run nulls, and `LoadOptions` assignment. Integration already has that assignment and the later launch and restore carry from the historical restore work, with the trimmed parser kept and the untrimmed parser and initial-prompt field left out.

Required commands were not run:

```sh
mise exec -- mbx +1.99.0 nextest run -p jackin -E 'test(load)'
mise exec -- mbx +1.99.0 nextest run -p jackin-runtime -E 'test(launch) | test(restore)'
mise exec -- mbx +1.99.0 nextest run -p jackin-capsule -E 'test(model) | test(effort)'
```

Status: NOT_RUN. This note does not approve TASK-011.

## Edit list

Empty at the audited HEAD. Later commit `ef5ca1df` copies `model` and `effort` inside `related_restore_load_options`. The pipeline still skips that rebuild when either value is set (`launch_pipeline.rs` `current_role_reuse_is_compatible`). The copy does not open a new launch path. The direct function test now keeps a supplied pair. This note does not approve TASK-011.
