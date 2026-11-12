# A14 T09 independent live verification

Date: 2026-11-10. Base commit: `fb0d396d6ebc57f8929c7f69c8066869d29add3c`. Worktree: `/tmp/jackin-a14-t09-verify`, branch `wt/a14-t09-verify`.

## Verdict

PASS. The independent local live proof passed after the terminal recovery recorded below. G-FAST also passed.

The reviewed fix changes only `crates/jackin-core/src/agent.rs`. It selects `https://api.z.ai/api/anthropic` and `anthropic-messages` for T09. No product code, test, decision, plan, design, blocker, or catalog file changed during this verification.

## Proof setup

The binary was `/tmp/jackin-a14-t09-verify/target/debug/jackin`, built with `mise exec -- mbx +1.99.1 build -p jackin`. `openshell --version` reported 0.1.7. On `jackin-local`, `jackin doctor` passed the binary, registration, health, pinned-version, and engine checks.

The proof used `XDG_CONFIG_HOME=/tmp/a14tv-xdg`. Symbolic links exposed the existing gateway registration and TLS material without copying it. The host Jackin configuration was not edited.

The temporary sentinel clone was `/tmp/jackin-a14tv-sentinel` at `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its role name was `a14tv-sentinel`. The manifest retained the codex, omp, and claude instruction mappings and had no role model pin. `jackin role check` exited 0. Every launch selected `glm-5.3` explicitly.

The sandbox was `jackin-a14tv-fnq0`, launch ID `a14tvompl01`, account `a14tv-omp-l`, and OpenShell provider `jackin-a14tv-omp-l-omp`. The derived image ID was `sha256:0162813f6e854100e5174a2e2e8d9432a87c2a32df0d1f33cf88d6ae9d5fb262`.

Each credential use resolved `omp --profile zai token zai` into `ZAI_API_KEY` in memory. The value was not printed, written, passed as a Jackin argument, or included in this evidence. The capture programs removed `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`, and `OPENAI_API_KEY` from child environments. They checked raw output for the host key before saving sanitized output. Raw environment and file scans stayed in memory and saved only match counts and byte counts.

## Image, runtime, and configuration

The first successful launch printed `image role: built` and `image derived: built`. A reload with the same launch ID printed `image derived: cached`, `image: skipped, the image is cached`, `sandbox: skipped, the sandbox exists`, and `upload: skipped, the project is uploaded`.

The sandbox reported `omp/18.18.3` and binary SHA256 `c49a84875d915c7d0de8bb69fe180b3bf53c16600bc4d5a61ea22ac2b3ac45c9`. It ran as `uid=1000(sandbox)` with no `sudo`. After the terminal recovery, omp PID 1095 was the child of `jackin-start omp` PID 370. The supervisor state was `running 1`.

The omp config, auth file, and `agent/models.yml` had mode 0600. Placeholder counts were one in `config.toml`, one in `auth.json`, and zero in `agent/models.yml`. The models file contained this provider configuration:

```yaml
providers:
  zai:
    baseUrl: https://api.z.ai/api/anthropic
    api: anthropic-messages
    apiKey: ZAI_API_KEY
```

The omp config named the same base URL, `credentialEnv: ZAI_API_KEY`, and `default: zai/glm-5.3`.

## Live response and tool use

The fixed request was `Reply with exactly: PONG`, with `omp -p --mode json --no-session --model zai/glm-5.3`. It exited 0. An assertion over the saved JSON events confirmed exact `PONG`, `api=anthropic-messages`, `provider=zai`, `model=glm-5.3`, `stopReason=stop`, and 8,998 total tokens.

A second request ran the bash tool once with `command="ls -A"`. The real result listed the sentinel root, including `AGENTS.md`, `Dockerfile`, `instructions`, `jackin.role.toml`, `probe`, and `tests`. The result had `isError=false`. The final assistant message was exact `TOOL-OK` with the same API, provider, and model metadata, `stopReason=stop`, and 9,158 total tokens.

Client-reported cost fields were not used as billing evidence.

## Credential and network checks

The host key had zero matches in 655 bytes of raw environment output, 2,147,684 bytes from readable files up to 4 MiB under `/sandbox`, the 554-byte inspect JSON, and every captured stderr stream. The environment held a 54-character broker value in `ZAI_API_KEY`. The three alternate paid-key variables were unset.

Across 29 capture records, stderr totaled 9,339 bytes with zero host-key matches. A separate scan of all 751,676 saved artifact bytes also found zero matches.

Inspect listed only `jackin-a14tv-omp-l-omp`, a loaded policy, and no closure violations. Requests from `curl` to `api.z.ai`, `example.com`, and `api.openai.com` each failed with exit 7. omp reached Z.AI and returned both model responses. Only the selected provider was attached, and no alternate paid-key environment variable was present.

## Terminal replay and shell persistence

The first terminal result differed from the source evidence. Four attach attempts with the verification binary and one control attempt with the earlier live binary returned `read files timed out after 30s`. Direct exec, inspection, and shell attach still worked. The cause is unproven.

I stopped and started only `jackin-a14tv-fnq0`. The supervisor then reported `waiting`. A same-launch-ID reload restarted omp from the cached image. Two subsequent 80-by-24 PTY attaches replayed the omp 18.18.3 screen, detached with Ctrl-P Ctrl-Q, and exited 0.

One shell printed `A14TV-SHELL-OK`, returned uid 1000 and `/sandbox/project`, and wrote a temporary marker. A second shell read the marker and printed `A14TV-SHELL-PERSISTED`. Both shells exited 0. Omp remained a child of `jackin-start`, and the supervisor remained `running 1`.

## Export and guarded purge

Sandbox commit `dbe498c08beda2f78fd8a2033f9b57a1b1f6bc3a` created unexported work. Guarded purge refused with exit 3. Its error named the uncommitted manifest and `branch feature/openshell-native not exported`.

Export created only this ref:

```text
refs/heads/jackin/jackin-a14tv-fnq0/feature/openshell-native  dbe498c08beda2f78fd8a2033f9b57a1b1f6bc3a
```

The temporary host HEAD stayed `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its only working-tree change remained `jackin.role.toml`, with diff SHA256 `d921eaacc6cb42eab59eb8499fd5ad141b626955c03ff576527ced6dfbd16179` before and after export.

Export does not include uncommitted work. I reverted the intentional role-name edit only in the sandbox, leaving the exported commit and the temporary host edit untouched. Guarded purge then deleted the sandbox.

## Plan usage

The Z.AI quota endpoint reported plan `max`. Snapshots contain percentages only.

| Snapshot | `TIME_LIMIT` | Five-hour `TOKENS_LIMIT` | Weekly `TOKENS_LIMIT` |
|---|---:|---:|---:|
| Before | 0% used | 1% used | 32% used |
| After | 0% used | 1% used | 32% used |

The displayed percentages did not move. This granularity cannot measure the two model calls. The plan credential, plan level, nonzero response tokens, single attached provider, and absent alternate paid keys support plan-funded use. Client cost fields are estimates, not invoices.

## G-FAST

The exact command exited 0:

```text
mise exec -- mbx +1.99.1 xtask ci --fast
```

Nextest ran 2,168 tests with 2,168 passes and no skips. The `deny`, `shear`, `comments`, `layers`, `pins`, `containers`, `errors`, `baseline`, `screen`, `demo-gate`, and `reuse` steps also passed.

## Cleanup

The owned sandbox was removed. The exported ref was deleted. The owned account, provider, workspace, and role were removed. The temporary sentinel clone was deleted. The original sentinel checkout remained clean at `6722f8024623c76726ce0293fc7e93da47b5ee74`.

The final local sandbox inventory contains only the pre-existing `jackin-a14t02b-995e`. Foreign providers remained. Host Jackin account, role, and workspace listings each had zero `a14tv` matches.

The unique role and derived images remain in the BuildKit cache. Jackin has no image-removal command. Direct Docker removal was not used.

## Corrections and limits

The config review is at `/tmp/jackin-a14-t09-config/rewrite/evidence/A14-t09-config-review.md`, not in the live worktree.

The source live evidence does not mention an attach failure or the sandbox-only manifest cleanup before final purge. This verification needed both recovery steps. The initial attach timeout has no proven root cause. The successful response, tool, terminal, export, and purge checks above are direct observations after the recorded recovery.
