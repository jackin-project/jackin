# A14 Z.AI supported-tuple recheck

Date: 2026-11-10. Base commit: `e275950ca3f5d82d17dfa49789f9582d3487088b`. Worktree: `/tmp/jackin-a14-zai-recheck`, branch `wt/a14-zai-recheck`.

## Verdict matrix

| Tuple | Agent and provider | `jackin-docker` | `jackin-local` |
|---|---|---|---|
| T09 | omp 18.18.3, Z.AI `glm-5.3` | **PASS** | **PASS** |
| T14 | Claude Code 2.1.311, Z.AI `glm-5.3` | **PASS** | **PASS** |
| T18 | Codex 0.167.0, Z.AI `glm-5.3` | **PASS** | **PASS** |

Every pair passed cached rebuild, uid 1000/no-sudo, direct-child supervision, exact live response and real successful tool use with nonzero usage, placeholder and host-key hygiene, provider-only egress, non-agent network denial, detach/reattach replay, shell persistence, exact export with unchanged host state, guarded purge followed by export/purge, and plan-funded use without a paid fallback. No SKIP was treated as PASS.

No production code, test, catalog, plan, design, blocker, decision, commit, or remote state changed. No full G-FAST was run, as requested.

## Proof setup

The binary was `/tmp/jackin-a14-zai-recheck/target/debug/jackin`, built from the pinned head with `mise exec -- mbx +1.99.1 build -p jackin`; the build exited 0 in 61.090 seconds. `openshell --version` reported `openshell 0.1.7`. `jackin doctor` exited 0 on both gateways and passed binary, registration, health, pinned-version, and engine checks.

The proof used `XDG_CONFIG_HOME=/tmp/a14zr-xdg`. The isolated Jackin config named only `jackin-docker` and `jackin-local`. Symbolic links exposed their existing OpenShell gateway registrations and TLS material without copying it. The host Jackin configuration was not edited.

The temporary sentinel clone was `/tmp/jackin-a14zr-sentinel` at `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its only host working-tree change was the role name `a14zr-sentinel` in `jackin.role.toml`; the manifest retained the codex, omp, and claude instruction mappings and had no model pin. `jackin role check` exited 0. Every launch selected `--model glm-5.3` explicitly.

The only credential source was the sanctioned host omp account path: each account and launch command resolved `omp --profile zai token zai` into `ZAI_API_KEY` in memory. The value was not printed, written, passed as an argument, or included in evidence. No 1Password command was used. Capture helpers removed `ZAI_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`, and `OPENAI_API_KEY` from ordinary child environments and checked output for the host key before saving it. Raw environment and file scans stayed in memory and saved only match and byte counts.

### Owned resources

| Tuple | Gateway | Sandbox | Launch ID | Account | Provider | Derived image |
|---|---|---|---|---|---|---|
| T09 | docker | `jackin-a14zr-wxmq` | `a14zrompd01` | `a14zr-omp-d` | `jackin-a14zr-omp-d-omp` | `sha256:cde9c06c5e48fa4a4334cf91798a82ca5c4b9608c86ffd30ccce517669975d67` |
| T09 | local | `jackin-a14zr-glvo` | `a14zrompl01` | `a14zr-omp-l` | `jackin-a14zr-omp-l-omp` | same |
| T14 | docker | `jackin-a14zr-td1n` | `a14zrclauded01` | `a14zr-claude-d` | `jackin-a14zr-claude-d-claude` | `sha256:07095d49c126de3efa7b98813fd7b46a20a78751f2116d124ac1d2480b0fdbbf` |
| T14 | local | `jackin-a14zr-5gty` | `a14zrclaudel01` | `a14zr-claude-l` | `jackin-a14zr-claude-l-claude` | same |
| T18 | docker | `jackin-a14zr-i25v` | `a14zrcodexd01` | `a14zr-codex-d` | `jackin-a14zr-codex-d-codex` | `sha256:70d1df6d545c50c9b735f3c7b8ff7530cbec0cfbdea55b0a2f0c3fcae724bc31` |
| T18 | local | `jackin-a14zr-xc5o` | `a14zrcodexl01` | `a14zr-codex-l` | `jackin-a14zr-codex-l-codex` | same |

Every same-launch-ID reload exited 0 and printed `image derived: cached`, `image: skipped, the image is cached`, `sandbox: skipped, the sandbox exists`, and `upload: skipped, the project is uploaded`. Every inspect showed Ready, exactly the one matching provider above, a loaded policy, and no closure violations.

### Plan usage

The Z.AI quota endpoint reported plan `max`. Snapshots contain percentages only.

| Snapshot | `TIME_LIMIT` | Five-hour `TOKENS_LIMIT` | Weekly `TOKENS_LIMIT` |
|---|---:|---:|---:|
| Before T09 | 0% used | 0% used | 32% used |
| After T09 | 0% used | 1% used | 32% used |
| After T14 | 0% used | 1% used | 32% used |
| After T18 | 0% used | 1% used | 32% used |

All 12 model requests reported nonzero usage. Only the matching Z.AI provider was attached to each sandbox, and all alternate paid-key variables were unset. T09 moved the displayed five-hour window by one percentage point. T14 and T18 did not move the displayed percentage, but their response-level usage was nonzero. These observations prove plan-funded use without a paid fallback. Client-reported cost fields are estimates and were not used as billing evidence.

## T09: omp with Z.AI

The sanctioned rendered provider was:

```yaml
providers:
  zai:
    baseUrl: https://api.z.ai/api/anthropic
    api: anthropic-messages
    apiKey: ZAI_API_KEY
```

Both sandboxes reported `omp/18.18.3`, binary SHA256 `c49a84875d915c7d0de8bb69fe180b3bf53c16600bc4d5a61ea22ac2b3ac45c9`, uid 1000, no `sudo`, and supervisor state `running 1`. `config.toml`, `auth.json`, and `agent/models.yml` all had mode 0600. Placeholder counts were one, one, and zero respectively.

### Docker

Omp PID 366 was the direct child of `jackin-start omp` PID 64. The fixed `omp -p --mode json --no-session --model zai/glm-5.3` request returned exact `PONG`, `api=anthropic-messages`, `provider=zai`, `model=glm-5.3`, `stopReason=stop`, and 9,027 total tokens. A second request ran exactly one bash tool call with `command="ls -A"`, returned the real sentinel root listing with no tool error, and finished with exact `TOOL-OK` and 9,181 total tokens.

The host key had zero matches in 657 bytes of environment output, 690,775 bytes of readable files up to 4 MiB under `/sandbox`, the 554-byte inspect JSON, and captured stderr. The environment held a 53-character broker value in `ZAI_API_KEY`; `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`, and `OPENAI_API_KEY` were unset. Non-agent `curl` requests to `api.z.ai`, `example.com`, and `api.openai.com` each failed with exit 7. Omp reached only the selected provider.

Two PTY attaches exited 0 after replaying the omp 18.18.3 screen and detaching with Ctrl-P Ctrl-Q. Two shells exited 0, printed `A14ZR-SHELL-OK` then `A14ZR-SHELL-PERSISTED`, and returned uid 1000 and `/sandbox/project`. Omp remained PID 366 under PID 64 afterward.

Sandbox commit `abf75b47266707236b198f80d1cc434d8fb321c1` created clean, committed, unexported work. Guarded purge refused with exit 3 and `branch feature/openshell-native not exported`. Export created exactly:

```text
refs/heads/jackin/jackin-a14zr-wxmq/feature/openshell-native  abf75b47266707236b198f80d1cc434d8fb321c1
```

The host HEAD, status, working-tree diff SHA256, and empty-index diff SHA256 stayed unchanged. Guarded purge then exited 0 and deleted the sandbox.

### Local

Omp PID 692 was the direct child of `jackin-start omp` PID 372. The fixed request returned exact `PONG`, the required API/provider/model metadata, `stopReason=stop`, and 8,998 total tokens. The tool request ran `ls -A`, returned the real sentinel root listing with no tool error, and finished with exact `TOOL-OK` and 9,166 total tokens.

The host key had zero matches in 659 bytes of environment output, 690,811 bytes of readable files, the 554-byte inspect JSON, and captured stderr. The environment held a 55-character broker value in `ZAI_API_KEY`; all three alternate paid-key variables were unset. The same three non-agent `curl` requests each failed with exit 7.

Both attaches and both persistent-shell checks exited 0 with the same observations as docker. Omp remained PID 692 under PID 372. Sandbox commit `0c9f4012a8e1f3c32aee3442784f69bc1c4d587f` caused guarded purge to refuse with exit 3. Export created exactly:

```text
refs/heads/jackin/jackin-a14zr-glvo/feature/openshell-native  0c9f4012a8e1f3c32aee3442784f69bc1c4d587f
```

Host state stayed unchanged, and guarded purge then exited 0.

## T14: Claude Code with Z.AI

The sanctioned `settings.json` contained only the base URL and had mode 0600 and zero placeholders:

```json
{"env":{"ANTHROPIC_BASE_URL":"https://api.z.ai/api/anthropic"}}
```

`.credentials.json` had mode 0600 and one literal placeholder. Both sandboxes reported `2.1.311 (Claude Code)`, binary SHA256 `2e2496c9a8065df93acb0fca2d341f59a6b8777649b2e025ab8c9bcd5eef61f3`, uid 1000, no `sudo`, and supervisor state `running 1`.

Claude printed an `unrecognized_model` warning for `glm-5.3`. Every result nevertheless named canonical model `glm-5.3`, used only the configured Z.AI base URL and matching attached provider, exited 0, and had `is_error=false` and `api_error_status=null`.

### Docker

Claude PID 370 was the direct child of `jackin-start claude` PID 64. The fixed JSON request returned exact `PONG` in one turn with 4,252 input tokens, 48 output tokens, and 8,000 cache-read tokens. The stream-json request made exactly one `Bash` call with `command="ls"`, received the real sentinel root listing with `is_error=false`, and returned exact `TOOL-OK` in two turns with no permission denials. It reported 4,441 input tokens, 62 output tokens, and 20,224 cache-read tokens.

The host key had zero matches in 680 bytes of environment output, 1,487,268 bytes of readable files, the 563-byte inspect JSON, and captured stderr. `ANTHROPIC_AUTH_TOKEN` held a 63-character broker value; `ZAI_API_KEY`, `ANTHROPIC_API_KEY`, and `OPENAI_API_KEY` were unset. Non-agent `curl` requests to `api.z.ai`, `example.com`, and `api.anthropic.com` each failed with exit 7. Claude reached only the selected provider.

Two PTY attaches exited 0 after replaying the Claude Code screen and detaching. Both persistent-shell checks exited 0 with the required markers, uid 1000, and `/sandbox/project`. Claude remained PID 370 under PID 64.

Sandbox commit `1cd49798f6194a3974015d03c07a6c1c47a69b43` caused guarded purge to refuse with exit 3 and the unexported-branch error. Export created exactly:

```text
refs/heads/jackin/jackin-a14zr-td1n/feature/openshell-native  1cd49798f6194a3974015d03c07a6c1c47a69b43
```

Host HEAD, working tree, and index stayed unchanged. Guarded purge then exited 0.

### Local

Claude PID 676 was the direct child of `jackin-start claude` PID 373. The fixed request returned exact `PONG` with 4,237 input tokens, 39 output tokens, and 8,000 cache-read tokens. The tool request made exactly one successful `Bash ls` call, returned the real sentinel root listing, and finished with exact `TOOL-OK` in two turns with no permission denials. It reported 2,232 input tokens, 59 output tokens, and 22,400 cache-read tokens.

The host key had zero matches in 679 bytes of environment output, 1,494,630 bytes of readable files, the 563-byte inspect JSON, and captured stderr. `ANTHROPIC_AUTH_TOKEN` held a 63-character broker value; all alternate paid-key variables were unset. The same three non-agent `curl` requests each failed with exit 7.

Both attaches and both persistent-shell checks exited 0. Claude remained PID 676 under PID 373. Sandbox commit `738afedd3c9cb4d6445d8641ff8f0881189f2e27` caused guarded purge to refuse with exit 3. Export created exactly:

```text
refs/heads/jackin/jackin-a14zr-5gty/feature/openshell-native  738afedd3c9cb4d6445d8641ff8f0881189f2e27
```

Host state stayed unchanged, and guarded purge then exited 0.

## T18: Codex with Z.AI

The sanctioned `config.toml` had mode 0600 and zero placeholders. `auth.json` was absent. The custom provider configuration was:

```toml
model = "glm-5.3"
model_provider = "zai"

[model_providers.zai]
base_url = "https://api.z.ai/api/v1"
env_key = "ZAI_API_KEY"
name = "Z.AI"
wire_api = "responses"
```

Both sandboxes reported `codex-cli 0.167.0`, binary SHA256 `2fe94d848540ff2a6868c7ebcc494126a974aa409711676d14fa0379e49e6143`, uid 1000, no `sudo`, and supervisor state `running 1`. The supervisor and direct-child arguments included `--sandbox danger-full-access` and `check_for_update_on_startup=false`.

Codex warned that local metadata for `glm-5.3` was unavailable and used fallback metadata. Every response header still reported the exact requested model and provider, and every command exited 0.

### Docker

Codex PID 362 was the direct child of `jackin-start codex` PID 64. The fixed request returned exact `PONG`, `model: glm-5.3`, `provider: zai`, and 1,678 tokens used. The second request executed exactly one real `/bin/bash -lc 'ls -A'` command in `/sandbox/project`, returned the sentinel root listing successfully, and finished with exact `TOOL-OK` and 8,254 tokens used.

The host key had zero matches in 661 bytes of environment output, 1,078,141 bytes of readable files, the 560-byte inspect JSON, and captured stderr. `ZAI_API_KEY` held a 55-character broker value; `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`, and `OPENAI_API_KEY` were unset. Non-agent `curl` requests to `api.z.ai`, `example.com`, and `api.openai.com` each failed with exit 7. Codex reached only the selected provider.

Two PTY attaches exited 0 after replaying the Codex 0.167.0 screen with `glm-5.3` and detaching. Both persistent-shell checks exited 0 with the required markers, uid 1000, and `/sandbox/project`. Codex remained PID 362 under PID 64.

Sandbox commit `88b47ba17feb5adf53ab518a33556c8c6bd4e7cb` caused guarded purge to refuse with exit 3 and the unexported-branch error. Export created exactly:

```text
refs/heads/jackin/jackin-a14zr-i25v/feature/openshell-native  88b47ba17feb5adf53ab518a33556c8c6bd4e7cb
```

Host HEAD, working tree, and index stayed unchanged. Guarded purge then exited 0.

### Local

Codex PID 667 was the direct child of `jackin-start codex` PID 373. The fixed request returned exact `PONG`, `model: glm-5.3`, `provider: zai`, and 1,676 tokens used. The tool request executed one successful `/bin/bash -lc 'ls -A'`, returned the real sentinel root listing, and finished with exact `TOOL-OK` and 9,959 tokens used.

The host key had zero matches in 661 bytes of environment output, 1,080,275 bytes of readable files, the 560-byte inspect JSON, and captured stderr. `ZAI_API_KEY` held a 55-character broker value; all alternate paid-key variables were unset. The same three non-agent `curl` requests each failed with exit 7.

Both attaches and both persistent-shell checks exited 0. Codex remained PID 667 under PID 373. Sandbox commit `be44388e405a7294241f07ccb3342a684214b63c` caused guarded purge to refuse with exit 3. Export created exactly:

```text
refs/heads/jackin/jackin-a14zr-xc5o/feature/openshell-native  be44388e405a7294241f07ccb3342a684214b63c
```

Host state stayed unchanged, and guarded purge then exited 0.

## Host-state invariant

Across every export, the temporary host HEAD stayed `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its status stayed exactly ` M jackin.role.toml`; the working-tree diff SHA256 stayed `d38e14dcd7ae5d2a7e236a3a804d2f9165aeba49e21944c735f817f17b8f592d`; and the cached-diff SHA256 stayed the empty hash `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`. Before/after ref inventories show that each export added only its named ref. Thus host HEAD, working tree, and index were unchanged by all six exports.

## Dated failures and corrections

No live tuple or gateway failed on 2026-11-10. The following setup/harness failures were not converted into SKIP or PASS and were corrected before the relevant proof:

- The first T09 launch on each gateway exited 1 with `error: no workspace for /private/tmp/jackin-a14zr-sentinel; run jackin workspace add`. No sandbox was created. I added the isolated `a14zr` workspace and reran the same launch IDs successfully.
- Two initial account-capture invocations placed the helper's `--secret` option after its label, causing Python `PermissionError: [Errno 13] Permission denied: '--secret'` before any account command ran. Correcting the helper argument order allowed both account commands to exit 0. No credential appeared in the error or artifacts.
- The first workspace-removal command omitted a gateway and exited 1 with `error: several gateways are configured and none is the default (jackin-docker, jackin-local); pass --gateway`. Repeating it with `--gateway jackin-docker` exited 0; final docker and local listings were empty.
- The initial unscoped account-inventory command used `jackin account list` and exited 2 with `error: unrecognized subcommand`. The corrected per-gateway `jackin account ls --gateway ...` commands succeeded and supplied the cleanup inventory.

These were harness/setup corrections, not product defects. The earlier T09 HTTP 404 did not recur.

## Secret sweep and cleanup

A final in-memory sweep of 425 sanitized artifact and isolated-config files, totaling 2,248,750 bytes, found zero host-key matches. This includes inspect JSON and every captured stderr stream; per-sandbox environment and file scans also each found zero matches.

All six owned sandboxes were removed by guarded post-export purge. All six exported refs were deleted by exact ref name. All six owned accounts were removed by exact account and gateway. Final provider inventories contained no `a14zr` provider, so no remaining provider required deletion; no broad or shared-account/provider removal was used. The `a14zr` workspace and `a14zr-sentinel` role were removed. The temporary sentinel clone was deleted. The original sentinel checkout remained clean at `6722f8024623c76726ce0293fc7e93da47b5ee74` and was not used as the project.

Foreign resources were not targeted. The unique role and derived images remain in BuildKit cache under `a14zr-sentinel` tags. Jackin has no image-removal command, and no direct Docker, Podman, or host agent execution was used.
