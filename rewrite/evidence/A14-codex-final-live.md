# A14 final-head live proof for Codex with Kimi, MiniMax, and MiMo

Date: 2026-11-10. Base commit: `e275950ca3f5d82d17dfa49789f9582d3487088b`. Worktree: `/tmp/jackin-a14-codex-final`, branch `wt/a14-codex-final`.

## Verdicts

| Tuple | Agent, provider, and exact model | `jackin-docker` | `jackin-local` |
|---|---|---|---|
| T17 | Codex, Kimi, `kimi-for-coding` | **PASS** | **PASS** |
| T19 | Codex, MiniMax, `MiniMax-M3` | **PASS** | **PASS** |
| T20 | Codex, MiMo, `mimo-v2.6-pro` | **PASS** | **PASS** |

Every cell passed cached reload, uid and process checks, exact live replies, a real shell tool call with nonzero usage, credential and file scans, provider-only egress, two attaches, shell access, guarded purge refusal, export, and post-export purge. No SKIP is counted as PASS.

No production change, test, design, plan, decision, or blocker change was committed or pushed. No source diff remains. The authorized proof-only `Supported` enablement for T17, T19, and T20 in `crates/jackin-core/src/agent.rs` was restored after the original proof and again after the lifecycle-only host-state re-proof. This evidence and the local audit trail are uncommitted. G-FAST was not run, as instructed.

## Proof setup

The binary was `/tmp/jackin-a14-codex-final/target/debug/jackin`, built with `mise exec -- mbx +1.99.1 build -p jackin`. The build exited 0. OpenShell reported 0.1.7. `jackin doctor` passed on both gateways, including gateway health, the pinned version, and the engine socket.

The proof used `XDG_CONFIG_HOME=/tmp/a14cf-xdg`. The isolated Jackin config named only `jackin-docker` and `jackin-local`. Symbolic links let OpenShell read the existing gateway TLS material without copying it. The host Jackin configuration was not changed.

The temporary role was `a14cfx-sentinel`, cloned at `/tmp/jackin-a14cfx-sentinel` from sentinel HEAD `6722f8024623c76726ce0293fc7e93da47b5ee74`. Only the role name changed. `jackin role check` passed. The workspace was `a14cfx`. Each launch selected the exact model explicitly.

The only credential sources were `/tmp/jackin-plan-keys/kimi`, `/tmp/jackin-plan-keys/minimax`, and `/tmp/jackin-plan-keys/mimo`. Metadata inspection showed mode 0600 for each source. No 1Password command ran. `/tmp/a14cf-capture.py` loaded the values into memory, removed all three source variables and alternate paid-key variables from child environments, and injected only the selected variable for account, launch, and usage commands. No value appeared in an argument, chat, metadata, retained output, or this evidence.

The capture tool checked stdout and stderr for all three source values before saving redacted output. Sandbox checks used SHA-256 to compare the source value against environment values and candidate tokens of at least 40 characters in readable files up to 4 MiB under `/sandbox`. They saved only counts, the broker value length, and whether the broker value equaled the source value. Sanitized artifacts remain under `/tmp/a14cf-artifacts`.

All six launches used derived image `70d1df6d545c50c9b735f3c7b8ff7530cbec0cfbdea55b0a2f0c3fcae724bc31` and Codex CLI 0.167.0 with binary SHA256 `2fe94d848540ff2a6868c7ebcc494126a974aa409711676d14fa0379e49e6143`. The first T17 docker launch built the role and derived image. Its same-launch reload reported `image derived: cached`, `image: skipped, the image is cached`, `sandbox: skipped, the sandbox exists`, and `upload: skipped, the project is uploaded`. Every later launch used the cached derived image, and every same-launch reload reported the same cached or skipped states.

Every sandbox ran as `uid=1000(sandbox)` with no `sudo`. The supervisor state was `running 1`. Codex was a direct child of `jackin-start codex --sandbox danger-full-access -c check_for_update_on_startup=false`. Every `config.toml` had mode 0600, zero placeholders, and no `auth.json`.

Each inspect result listed only the matching account provider, a loaded policy, and no closure violations. The environment and file scans found zero source-key matches and zero alternate paid-key variables. Inspect and all captured stderr streams also had zero source-key matches. The sandbox credential was a broker value, not the source value.

For each cell, `curl` failed with exit 7 for the selected provider endpoint, `example.com`, OpenAI, and the other two provider hosts. The profile permits only `/usr/local/bin/codex`, so these requests carried no provider credential. Codex succeeded through the same provider endpoint. This proves provider-only agent egress and denies credentials to non-agent binaries.

Two PTY attaches exited 0 after replaying the Codex 0.167.0 screen with the exact selected model. Both detached with Ctrl-P Ctrl-Q. A shell pane printed `A14CF-SHELL-OK`, returned uid 1000 and `/sandbox/project`, and exited 0. Subsequent in-sandbox Git commands succeeded.

For every cell, guarded purge refused with exit 3 before export. Export created the exact ref listed below. The post-export guarded purge exited 0. The temporary host HEAD stayed `6722f8024623c76726ce0293fc7e93da47b5ee74`, its index remained unchanged, and its only working-tree change remained `jackin.role.toml`. The working-tree diff SHA256 stayed `300a3773ea2e28325283a2e66ca115827fe9320352dcd77f975d7be9db1b0244` before and after every export. Every exported ref was deleted after purge.

## Host-state export receipts

Independent review on 2026-11-10 found that the original artifacts did not contain host HEAD, index, or working-tree receipts. A lifecycle-only re-proof on the same date added those receipts. It used the temporary role `a14cfh-sentinel`, workspace `a14cfh`, and isolated config `/tmp/a14cfh-xdg`. Each launch omitted `--task`. No `codex exec` or usage command ran, so this re-proof made no paid model requests.

Each cell committed the sentinel role name and a marker file in the sandbox, refused guarded purge with exit 3, captured host state, exported, captured host state again, purged, deleted the exported ref, and removed the exact account and provider. The `t17hs-*`, `t19hs-*`, and `t20hs-*` artifacts record those lifecycle commands and exit codes.

Every before/after pair has the same HEAD `6722f8024623c76726ce0293fc7e93da47b5ee74`, status ` M jackin.role.toml`, index-content SHA256 `f362752300d6233a54a6e3c5606614cbdfb7db4d0b8709082a37a9ca21519ceb`, empty cached-diff SHA256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`, cached-diff quiet exit 0, and working-tree diff SHA256 `c1974f08104849c7efd419bbe1ef60eb74c08ed0af72940800a2e81d05029959`. Every before receipt has no owned refs. Every after receipt has only the exact ref below. Every comparison receipt records `true` for all six unchanged fields.

The table gives each artifact stem under `/tmp/a14cf-artifacts`. Each stem names three files: `<stem>-before.json`, `<stem>-after.json`, and `<stem>-comparison.json`. These JSON receipts contain timestamps, Git state, and refs, with no credential values.

| Cell | Artifact stem | Exported ref and commit |
|---|---|---|
| T17 docker | `t17-host-state-d` | `refs/heads/jackin/jackin-a14cfh-a437/feature/openshell-native` at `84a42e931cb032b1d3eb75299bdd5f222144d1f0` |
| T17 local | `t17-host-state-l` | `refs/heads/jackin/jackin-a14cfh-zh25/feature/openshell-native` at `dff33cbfb508285c7b1328a645b1bb1a40e6f586` |
| T19 docker | `t19-host-state-d` | `refs/heads/jackin/jackin-a14cfh-736o/feature/openshell-native` at `db6a20ef753d765d8ba39f0bbc20bd1e8bb1e930` |
| T19 local | `t19-host-state-l` | `refs/heads/jackin/jackin-a14cfh-h2pf/feature/openshell-native` at `8d2660ff9d37a94bb6145d2d1adedd8645f44c0a` |
| T20 docker | `t20-host-state-d` | `refs/heads/jackin/jackin-a14cfh-bwrm/feature/openshell-native` at `ec3923273759ea20497fea50ffe1eea2f968340a` |
| T20 local | `t20-host-state-l` | `refs/heads/jackin/jackin-a14cfh-f2xb/feature/openshell-native` at `4c627b07a6d754fa6bae4389908857672132f2d1` |

## T17: Codex with Kimi

### Docker

The sandbox was `jackin-a14cfx-dfaj`, launch `a14cfxkd01`, account `a14cf-kimi-d`, and provider `jackin-a14cf-kimi-d-codex`. Codex PID 363 had `jackin-start` PID 64 as its parent.

The rendered config named `kimi-for-coding`, provider `kimi`, `https://api.kimi.ai/coding/v1`, `KIMI_API_KEY`, and `wire_api = "responses"`.

The fixed request returned exact `PONG`, provider `kimi`, model `kimi-for-coding`, and 1,784 tokens. The tool request ran `/bin/bash -lc 'ls -A /sandbox/project'`, returned the real sentinel tree including `AGENTS.md` and `jackin.role.toml`, finished with exact `TOOL-OK`, and reported 838 tokens.

The scan covered 176 files and 5,205,153 bytes. It found zero source-key matches in the environment and files.

Guarded purge refused before export. Export created `refs/heads/jackin/jackin-a14cfx-dfaj/feature/openshell-native` at `59c0106432c8fce9a33edadc2f8a75a907697e09`. Post-export purge deleted the sandbox.

### Local

The sandbox was `jackin-a14cfx-wkpb`, launch `a14cfxkl01`, account `a14cf-kimi-l`, and provider `jackin-a14cf-kimi-l-codex`. Codex PID 666 had `jackin-start` PID 372 as its parent.

The fixed request returned exact `PONG` with the same provider and model and 37 tokens. The tool request returned the real sentinel tree and exact `TOOL-OK` with 604 tokens. The scan covered 175 files and 5,184,701 bytes with zero source-key matches.

Guarded purge refused with `branch feature/openshell-native not exported`. Export created `refs/heads/jackin/jackin-a14cfx-wkpb/feature/openshell-native` at `bb61c965de5931eccf1ca819bdab54df027f922e`. Post-export purge deleted the sandbox.

### Plan funding

The live Kimi quota reported these values.

| Snapshot | Five-hour used | Weekly used |
|---|---:|---:|
| Before T17 | 29% | 65% |
| After T17 | 33% | 67% |

Only the matching Kimi provider was attached in each cell. Alternate paid-key variables were absent. The measured plan consumption and successful Kimi responses prove plan-funded use without a paid fallback.

## T19: Codex with MiniMax

### Docker

The sandbox was `jackin-a14cfx-zmmv`, launch `a14cfxmd01`, account `a14cf-minimax-d`, and provider `jackin-a14cf-minimax-d-codex`. Codex PID 361 had `jackin-start` PID 64 as its parent.

The rendered config named exact uppercase `MiniMax-M3`, provider `minimax`, `https://api.minimax.io/v1`, `MINIMAX_CODE_API_KEY`, and `wire_api = "responses"`.

The fixed request returned exact `PONG`, provider `minimax`, model `MiniMax-M3`, and 9,708 tokens. The tool request ran `ls -A /sandbox/project`, returned the real sentinel tree, finished with exact `TOOL-OK`, and reported 10,022 tokens. The scan covered 175 files and 5,204,853 bytes with zero source-key matches.

Guarded purge refused before export. Export created `refs/heads/jackin/jackin-a14cfx-zmmv/feature/openshell-native` at `bb4312713f718f6cfabbc74b200dbcfc42f17215`. Post-export purge deleted the sandbox.

### Local

The sandbox was `jackin-a14cfx-e29r`, launch `a14cfxml01`, account `a14cf-minimax-l`, and provider `jackin-a14cf-minimax-l-codex`. Codex PID 676 had `jackin-start` PID 371 as its parent.

The fixed request returned exact `PONG` with the same exact provider and model and 9,708 tokens. The tool request ran `ls -A`, returned the real sentinel tree, finished with exact `TOOL-OK`, and reported 10,030 tokens. The scan covered 177 files and 5,460,492 bytes with zero source-key matches.

Guarded purge refused before export. Export created `refs/heads/jackin/jackin-a14cfx-e29r/feature/openshell-native` at `a0fb440a305388f8407519f5c86298d19f2ad92a`. Post-export purge deleted the sandbox.

### Plan funding

The MiniMax five-hour plan window moved from 0% used to 1% used. The weekly window remained at 1% used. Only the matching MiniMax provider was attached in each cell, and alternate paid-key variables were absent. The plan-window movement and successful MiniMax responses prove plan-funded use without a paid fallback.

## T20: Codex with MiMo

### Docker

The sandbox was `jackin-a14cfx-29y2`, launch `a14cfxid01`, account `a14cf-mimo-d`, and provider `jackin-a14cf-mimo-d-codex`. Codex PID 364 had `jackin-start` PID 64 as its parent.

The rendered config named `mimo-v2.6-pro`, provider `mimo`, `https://token-plan-sgp.xiaomimimo.com/v1`, `XIAOMI_TOKEN_PLAN_SGP_API_KEY`, and `wire_api = "responses"`. It retained top-level `web_search = "disabled"` throughout the live runs. No manual config or side provider was used.

The fixed request returned exact `PONG`, provider `mimo`, model `mimo-v2.6-pro`, and 8,485 tokens. The tool request ran `ls -A`, returned the real sentinel tree, finished with exact `TOOL-OK`, and reported 452 tokens. The scan covered 177 files and 5,868,601 bytes with zero source-key matches.

Guarded purge refused before export. Export created `refs/heads/jackin/jackin-a14cfx-29y2/feature/openshell-native` at `9a887667a50ae257a207dd0c10a37cb23f6a18e2`. Post-export purge deleted the sandbox.

### Local

The sandbox was `jackin-a14cfx-56z7`, launch `a14cfxil01`, account `a14cf-mimo-l`, and provider `jackin-a14cf-mimo-l-codex`. Codex PID 668 had `jackin-start` PID 373 as its parent. The rendered config again retained `web_search = "disabled"`.

The fixed request returned exact `PONG` with the same provider and model and 36 tokens. The tool request ran `ls -A`, returned the real sentinel tree, finished with exact `TOOL-OK`, and reported 3,845 tokens. The scan covered 175 files and 5,185,228 bytes with zero source-key matches.

Guarded purge refused before export. Export created `refs/heads/jackin/jackin-a14cfx-56z7/feature/openshell-native` at `7e5d095cc8819615fad6ab27cd51370256ec0c84`. Post-export purge deleted the sandbox.

### Plan funding

The authorized credential is the Singapore MiMo Token Plan source, and both successful cells used only `token-plan-sgp.xiaomimimo.com` through the matching MiMo provider. Every response reported nonzero usage. Alternate paid-key variables were absent. This proves Token Plan-funded use without a paid fallback.

`jackin usage` reports `unsupported` for MiMo because this head has no machine-readable MiMo quota adapter. This is a reporting limitation, not an unrun live check. The live plan endpoint, exact Token Plan credential source, provider attribution, and nonzero successful usage are the billing evidence.

## Dated failures and corrections

All dates are 2026-11-10.

- The first T17 docker attempt failed before sandbox creation. The generated name `jackin-a14cfin-nku9` already belonged to a concurrent run with role `a14cfinal-sentinel`, workspace `a14cfinal`, and a different image and agent hash. Jackin exited 1 and ordered a new launch ID. The foreign sandbox was not changed. This proof moved to the unique `a14cfx` resources and passed.
- The first credential file scan exited 127 with `/bin/bash: line 1: python3: command not found`. It was not counted as a pass. The same check ran successfully with the sandbox's installed Perl runtime, and all later scans used Perl.
- The first T17 docker export published marker commit `f03a8c39cd7c097f3e03e9d5face7adc00f81166` but warned that uncommitted work remained. Guarded purge still exited 3 because the temporary role-name edit was uncommitted in the sandbox. The sandbox committed that role edit, and a second export published final commit `59c0106432c8fce9a33edadc2f8a75a907697e09`. Guarded purge then exited 0. The host tree, index, and HEAD stayed unchanged.
- Every remaining guarded purge refusal was the expected unexported-work guard, not a product failure. Each refusal included the sandbox name and `branch feature/openshell-native not exported`, exited 3, and was followed by a successful export and purge.
- Independent review found no durable host-state receipts for the original exports. The lifecycle-only `a14cfh` re-proof added twelve before/after receipts and six comparison receipts without paid model calls. It also corrected the report's stale statement that the temporary `agent.rs` enablement remained. The source diff is now restored.

## Cleanup

All six owned sandboxes were removed through post-export guarded purge. All six exported refs were deleted. The six owned accounts were removed. Account removal had already deleted their providers, so the exact provider-delete commands reported each owned provider as not found. No broad shared-account deletion was used.

The `a14cfx` workspace and `a14cfx-sentinel` role were removed. Both temporary sentinel clones were deleted. The account, workspace, and role inventories are empty in the isolated config. No `a14cfx` sandbox or exported ref remains.

The lifecycle-only `a14cfh` sandboxes, refs, accounts, providers, workspace, role, and temporary clone were also removed. Its isolated account, workspace, and role inventories are empty, and no `a14cfh` sandbox or exported ref remains.

Foreign sandboxes and providers were not changed. In particular, concurrent `a14cfinal` sandboxes and `jackin-a14cf-claude-*` providers belong to other runs and were left alone. The role and derived images remain in the BuildKit cache under their unique tags because Jackin has no image-removal command. No direct Docker, Podman, Apple container, or host agent execution was used.
