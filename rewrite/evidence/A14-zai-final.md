# A14 final-head live proof for Z.AI

Date: 2026-11-09. Base commit: `ebfb26c9c3961ceba28e0cab84396c5530648b20`. Worktree: `/tmp/jackin-a14-zai-final`, branch `wt/a14-zai-final`.

## Verdicts

- T09, omp with Z.AI: **FAIL**. The image and sandbox checks passed on both gateways. omp 18.18.3 selected `anthropic-messages` for `zai/glm-5.3` and received HTTP 404 with zero usage tokens. PONG and tool use did not succeed.
- T14, Claude Code with Z.AI: **PASS** on `jackin-docker` and `jackin-local`.
- T18, Codex with Z.AI: **PASS** on `jackin-docker` and `jackin-local`.

No product code, tests, catalog state, plan, design, blocker, or decision file changed.

## Proof setup

The binary was `/tmp/jackin-a14-zai-final/target/debug/jackin`, built with `mise exec -- mbx +1.99.1 build -p jackin`. `openshell --version` reported 0.1.7. `jackin doctor` passed the binary, gateway registration, gateway health, pinned version, and engine checks on both gateways.

The proof used `XDG_CONFIG_HOME=/tmp/a14zf-xdg`. The isolated OpenShell registry named the same gateways. Symbolic links let OpenShell read the existing gateway TLS material without copying it. The host Jackin configuration was not changed.

The temporary sentinel clone was `/tmp/jackin-a14zf-sentinel` at `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`. Its role name was `a14zf-sentinel`. The manifest kept the existing codex, omp, and claude instruction mappings and had no role model pin. `jackin role check` passed. Every launch selected `glm-5.3` explicitly with `--model glm-5.3`.

The only credential source was the host omp store. Each account command resolved `omp --profile zai token zai` into `ZAI_API_KEY` in memory. The value was not printed, written, passed as an argument, or included in this evidence. A temporary capture program removed the host key from child environments except for account and launch commands that require it. The program checked stdout and stderr before saving sanitized output.

The Z.AI plan reported `max`. Quota snapshots contained percentages only.

| Snapshot | Five-hour window | Weekly window | Zread monthly |
|---|---:|---:|---:|
| Before T09 | 0.0% used | 32.0% used | 0.0% used |
| After T09 | 0.0% used | 32.0% used | 0.0% used |
| After T14 | 1.0% used | 32.0% used | 0.0% used |
| After T18 | 1.0% used | 32.0% used | 0.0% used |

T09 consumed no plan quota. T14 moved the five-hour window by one displayed percentage point. T18 reported nonzero token usage on every response but did not move the displayed percentage. Only the matching Z.AI provider was attached to each sandbox. Alternate paid-key variables were unset. These facts prove plan use without a paid fallback for T14 and T18.

## T09: omp 18.18.3 with Z.AI

### Image and sandbox

The primary sandbox was `jackin-a14zf-8jhy` with launch ID `a14zfompd01`. The derived image was `sha256:0162813f6e854100e5174a2e2e8d9432a87c2a32df0d1f33cf88d6ae9d5fb262`. The initial launch printed `image derived: built`. A reload with the same launch ID printed `image derived: cached`, `image: skipped, the image is cached`, and `sandbox: skipped, the sandbox exists`.

The local sandbox was `jackin-a14zf-xz50` with launch ID `a14zfompl01`. Its launch used the cached derived image.

Both sandboxes reported `omp/18.18.3` and this binary digest:

```text
c49a84875d915c7d0de8bb69fe180b3bf53c16600bc4d5a61ea22ac2b3ac45c9
```

Both ran as `uid=1000(sandbox)` with no `sudo`. On docker, omp PID 381 was the child of `jackin-start omp` PID 64. On local, omp PID 688 was the child of `jackin-start omp` PID 373. Both supervisor state files contained `running 1`.

The omp config, auth file, and `models.yml` had mode 0600. Placeholder counts were one in `config.toml`, one in `auth.json`, and zero in `models.yml`. The model override named `https://api.z.ai/api/coding/paas/v4` and the environment variable name `ZAI_API_KEY`.

### Model failure

The fixed request was `Reply with exactly: PONG`, with `omp -p --mode json --no-session --model zai/glm-5.3`.

Docker and local returned exit 1 with the same response metadata:

```text
api=anthropic-messages
provider=zai
model=glm-5.3
stopReason=error
errorStatus=404
errorMessage=404 status code (no body)
totalTokens=0
```

This is the smallest failing condition on the final head. The stable omp pin selects an API that does not answer at the configured Z.AI coding endpoint. No PONG or real tool result occurred. These conditions are FAIL, not SKIP or PASS.

### Credential and network checks

On both gateways, the host key had zero matches in sandbox environment output, readable text files up to 4 MiB under `/sandbox`, `jackin inspect --json`, and captured stderr. The docker environment held a 55-character broker value in `ZAI_API_KEY`. `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`, and `OPENAI_API_KEY` were unset.

Only `jackin-a14zf-omp-d-omp` or `jackin-a14zf-omp-l-omp` was attached. The profile allows only `api.z.ai` and only omp. Requests from `curl` to `api.z.ai`, `example.com`, and `api.openai.com` failed with exit 7. omp reached the selected provider and received its 404 response. No other provider egress or paid fallback occurred.

### Terminal, export, and purge

Two docker PTY attaches exited 0 after replaying the omp 18.18.3 screen. Both detached with Ctrl-P Ctrl-Q. A shell pane printed `A14ZF-SHELL-OK`, returned uid 1000 and `/sandbox/project`, and exited 0. omp PID 381 remained a child of `jackin-start` afterward.

Sandbox commit `1cbfd23ce6cadb04885b63dd0ce27c5d703ae677` created unexported work. Guarded purge refused with exit 3 and `branch feature/openshell-native not exported`. Export created only this ref:

```text
refs/heads/jackin/jackin-a14zf-8jhy/feature/openshell-native  1cbfd23ce6cadb04885b63dd0ce27c5d703ae677
```

The temporary host HEAD stayed `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`. Its pre-existing role edit had the same SHA256 before and after export, `2848965dcefd62167dbd7e39fdef6f440b5fe442faf9520062892b9d10b0c27e`. Guarded purge then deleted the sandbox.

## T14: Claude Code 2.1.311 with Z.AI

### Image and sandbox

The primary sandbox was `jackin-a14zf-j033` with launch ID `a14zfclauded01`. The derived image was `sha256:0022bc2203ede2da89531e880caad557136a7db7b349846cdf955179130dc06c`. The initial launch printed `image derived: built`. The same-launch reload reported the derived image, sandbox, and upload as cached or skipped. The local sandbox `jackin-a14zf-yhiz` used the cached derived image.

Both sandboxes reported `2.1.311 (Claude Code)` and this binary digest:

```text
2e2496c9a8065df93acb0fca2d341f59a6b8777649b2e025ab8c9bcd5eef61f3
```

Both ran as uid 1000 with no `sudo`. Claude PID 370 was the child of `jackin-start claude` PID 64 on docker. Claude PID 676 was the child of PID 372 on local. Both supervisor state files contained `running 1`.

`settings.json` contained only this endpoint configuration and had mode 0600:

```json
{"env":{"ANTHROPIC_BASE_URL":"https://api.z.ai/api/anthropic"}}
```

`.credentials.json` had mode 0600 and one literal placeholder. The settings file had no auth entry and zero placeholders.

### Live response and tool use

The fixed docker request returned exit 0 with `result="PONG"`, `is_error=false`, `api_error_status=null`, and `modelUsage.glm-5.3.canonicalModel="glm-5.3"`. It reported 4,241 input tokens, 67 output tokens, and 8,000 cache-read tokens. The local request returned the same model and result metadata with 2,050 input tokens, 48 output tokens, and 10,176 cache-read tokens.

A docker stream-json request used one `Bash` tool call with `command="ls"`. The tool returned the real sentinel root listing, including `AGENTS.md`, `Dockerfile`, `instructions`, `jackin.role.toml`, `probe`, and `tests`. The final result was `TOOL-OK`, with two turns and no permission denials. The local tool request also returned `TOOL-OK` with two turns and no permission denials.

Claude printed an `unrecognized_model` warning for `glm-5.3`. The response still named the exact model and succeeded. Its `costUSD` fields had `costBasis="unknown"` and are client estimates, not evidence of paid fallback.

### Credential and network checks

The host key had zero matches in environment output, readable text files up to 4 MiB under `/sandbox`, inspect JSON, and captured stderr on both gateways. The environment held a 64-character broker value in `ANTHROPIC_AUTH_TOKEN`. `ZAI_API_KEY`, `ANTHROPIC_API_KEY`, and `OPENAI_API_KEY` were unset.

Only the matching T14 provider was attached. `curl` failed with exit 7 for `api.z.ai`, `example.com`, and `api.anthropic.com` because the policy restricts provider access to Claude. Claude reached `api.z.ai` and received the model responses. No other provider or paid fallback was configured.

### Terminal, export, and purge

Two docker PTY attaches exited 0 and replayed the same Claude example edit screen. A shell pane printed `A14ZF-SHELL-OK` and exited 0. Claude PID 370 remained a child of `jackin-start` afterward.

Sandbox commit `43c71ca97a4dde7aa8714a06c435bb8b634db17b` caused guarded purge to refuse with exit 3. Export created `refs/heads/jackin/jackin-a14zf-j033/feature/openshell-native` at that commit. The host HEAD and role-diff SHA256 stayed unchanged. Guarded purge then deleted the sandbox.

## T18: Codex 0.167.0 with Z.AI

### Image and sandbox

The primary sandbox was `jackin-a14zf-dlxb` with launch ID `a14zfcodexd01`. The derived image was `sha256:0523330587b3c8fb45d5f4a4f945bf089c5a3347ae816e56a066350a99ae1bed`. The initial launch printed `image derived: built`. The same-launch reload reported the derived image, sandbox, and upload as cached or skipped. The local sandbox `jackin-a14zf-w8io` used the cached derived image.

Both sandboxes reported `codex-cli 0.167.0` and this binary digest:

```text
2fe94d848540ff2a6868c7ebcc494126a974aa409711676d14fa0379e49e6143
```

Both ran as uid 1000 with no `sudo`. Codex PID 362 was the child of `jackin-start codex` PID 64 on docker. Codex PID 667 was the child of PID 372 on local. The launch arguments included `--sandbox danger-full-access` and `check_for_update_on_startup=false`.

`config.toml` had mode 0600 and contained `model="glm-5.3"`, `model_provider="zai"`, `base_url="https://api.z.ai/api/v1"`, `env_key="ZAI_API_KEY"`, and `wire_api="responses"`. It contained zero placeholders. `auth.json` was absent.

### Live response and tool use

The fixed docker request returned exit 0 and exact `PONG`, with `model: glm-5.3`, `provider: zai`, and 7,733 tokens used. Local returned exact `PONG` with the same model and provider and 1,653 tokens used.

Docker then ran `/bin/bash -lc 'ls -A'` in `/sandbox/project`. The command returned the real sentinel root listing, and Codex replied `TOOL-OK` with 9,581 tokens used. Local ran `/bin/bash -lc 'ls -a'`, returned the listing, and replied `TOOL-OK` with 447 tokens used.

Codex warned that it had no local metadata for `glm-5.3` and used fallback metadata. The response headers still reported the exact requested model and provider.

### Credential and network checks

The host key had zero matches in environment output, readable text files up to 4 MiB under `/sandbox`, inspect JSON, and captured stderr on both gateways. The environment held a 54-character broker value in `ZAI_API_KEY`. `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`, and `OPENAI_API_KEY` were unset.

Only the matching T18 provider was attached. `curl` failed with exit 7 for `api.z.ai`, `example.com`, and `api.openai.com`. Codex reached `api.z.ai` through its permitted binary path. No other provider or paid fallback was configured.

### Terminal, export, and purge

Two docker PTY attaches exited 0 and replayed the Codex 0.167.0 screen with `glm-5.3`. A shell pane printed `A14ZF-SHELL-OK` and exited 0. Codex PID 362 remained a child of `jackin-start` afterward.

Sandbox commit `11d4236140c479257bd3548318476e21544581e4` caused guarded purge to refuse with exit 3. Export created `refs/heads/jackin/jackin-a14zf-dlxb/feature/openshell-native` at that commit. The host HEAD and role-diff SHA256 stayed unchanged. Guarded purge then deleted the sandbox.

## Cleanup

All six owned sandboxes were removed. The three exported refs were deleted. All six owned accounts were removed. The four remaining T14 and T18 providers were explicitly deleted after account removal. T09 account removal had already removed its two providers. The `a14zf` workspace and `a14zf-sentinel` role were removed.

Foreign sandboxes and providers were not changed. The final gateway inventory contains only the pre-existing foreign sandboxes. The original sentinel checkout was not used as the project and was not changed.

The role and derived images remain in the BuildKit cache under the unique `a14zf-sentinel` tags. Jackin has no image-removal command. Direct Docker removal was not used because only `jackin-image` may start Docker.

## G-FAST

Command: `mise exec -- mbx +1.99.1 xtask ci --fast`

Result: exit 0 in 383.625 seconds. Nextest ran 2,168 tests in 237.537 seconds, with 2,168 passed and 0 skipped. The run also passed fmt, clippy, deny, shear, comments, layers, pins, containers, errors, baseline, screen, demo-gate, and reuse. The fast gate skipped the docs build as defined by the repository gate.
