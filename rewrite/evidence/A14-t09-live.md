# A14 T09 live proof after the anthropic-messages correction

Date: 2026-11-10. Base commit: `fb0d396d6ebc57f8929c7f69c8066869d29add3c`. Worktree: `/tmp/jackin-a14-t09-live`, branch `wt/a14-t09-live`.

## Verdict

T09 passes on `jackin-docker` and `jackin-local` with omp 18.18.3, provider `zai`, model `glm-5.3`, and API `anthropic-messages`. Both gateways returned exact `PONG`, executed a real bash tool call, returned exact `TOOL-OK`, and reported nonzero token usage. No new product defect appeared.

No product code, test, decision, plan, design, blocker, or catalog file changed.

## Proof setup

The binary was `/tmp/jackin-a14-t09-live/target/debug/jackin`, built with `mise exec -- mbx +1.99.1 build -p jackin`. `openshell --version` reported 0.1.7. `jackin doctor` passed the binary, gateway registration, gateway health, pinned version, and engine checks on both gateways.

The proof used `XDG_CONFIG_HOME=/tmp/a14t09-xdg`. The isolated OpenShell registry named the same gateways. Symbolic links let OpenShell read the existing gateway TLS material without copying it. The host Jackin configuration was not changed and contains no `a14t09` entry.

The temporary sentinel clone was `/tmp/jackin-a14t09-sentinel` at `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its role name was `a14t09-sentinel`. The manifest kept the codex, omp, and claude instruction mappings and had no role model pin. `jackin role check` passed. Every launch selected `glm-5.3` explicitly with `--model glm-5.3`.

The only credential source was the host omp store. Each account and launch command resolved `omp --profile zai token zai` into `ZAI_API_KEY` in memory. The value was not printed, written, passed as an argument, or included in this evidence. The capture programs removed alternate paid-key variables from child environments. They checked output for the host key before saving sanitized output. Raw environment and file scans stayed in memory and saved only match counts and byte counts.

## Plan usage

The Z.AI quota endpoint reported plan `max`. Snapshots contain percentages only.

| Snapshot | `TIME_LIMIT` | Five-hour `TOKENS_LIMIT` | Weekly `TOKENS_LIMIT` |
|---|---:|---:|---:|
| Before T09 | 0% used | 1% used | 32% used |
| After T09 | 0% used | 1% used | 32% used |

The displayed percentages did not move. All four model requests reported nonzero tokens. Only the matching Z.AI provider was attached to each sandbox. `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`, and `OPENAI_API_KEY` were unset. These facts prove plan-funded use without a paid fallback. Client-reported cost fields are estimates and are not billing evidence.

## Docker proof

### Image and sandbox

The sandbox was `jackin-a14t09-cean` with launch ID `a14t09ompd01`, account `a14t09-omp-d`, and provider `jackin-a14t09-omp-d-omp`. The derived image was `sha256:cde9c06c5e48fa4a4334cf91798a82ca5c4b9608c86ffd30ccce517669975d67`.

The initial launch printed `image role: built` and `image derived: built`. A reload with the same launch ID printed `image derived: cached`, `image: skipped, the image is cached`, `sandbox: skipped, the sandbox exists`, and `upload: skipped, the project is uploaded`.

The sandbox reported `omp/18.18.3` and binary digest `c49a84875d915c7d0de8bb69fe180b3bf53c16600bc4d5a61ea22ac2b3ac45c9`. It ran as `uid=1000(sandbox)` with no `sudo`. omp PID 381 was the child of `jackin-start omp` PID 64. The supervisor state contained `running 1`.

The omp config, auth file, and `agent/models.yml` had mode 0600. Placeholder counts were one in `config.toml`, one in `auth.json`, and zero in `agent/models.yml`. The models file contained this exact provider configuration:

```yaml
providers:
  zai:
    baseUrl: https://api.z.ai/api/anthropic
    api: anthropic-messages
    apiKey: ZAI_API_KEY
```

### Live response and tool use

The fixed request was `Reply with exactly: PONG`, with `omp -p --mode json --no-session --model zai/glm-5.3`. It returned exit 0, exact `PONG`, `api=anthropic-messages`, `provider=zai`, `model=glm-5.3`, `stopReason=stop`, and 9,013 total tokens.

A second request ran the bash tool once with `command="ls -A"`. The tool returned the real sentinel root listing, including `AGENTS.md`, `Dockerfile`, `instructions`, `jackin.role.toml`, `probe`, and `tests`. The tool result had `isError=false`. The final response was exact `TOOL-OK` with 9,181 total tokens.

### Credential and network checks

The host key had zero matches in 659 bytes of raw environment output, 688,645 bytes of readable text files up to 4 MiB under `/sandbox`, `jackin inspect --json`, and every captured stderr stream. The environment held a 54-character broker value in `ZAI_API_KEY`. The three alternate paid-key variables were unset.

Inspect listed only `jackin-a14t09-omp-d-omp`, a loaded policy, and no closure violations. Requests from `curl` to `api.z.ai`, `example.com`, and `api.openai.com` failed with exit 7. omp reached `api.z.ai` and received both model responses. No other provider egress or paid fallback occurred.

### Terminal, export, and purge

Two PTY attaches exited 0 after replaying the omp 18.18.3 screen. Both detached with Ctrl-P Ctrl-Q. A shell pane printed `A14T09-SHELL-OK`, returned uid 1000 and `/sandbox/project`, and exited 0. omp PID 381 remained a child of `jackin-start` afterward.

Sandbox commit `ac54d53a3aea6ed579f9c32d40b1f9be20a72f4b` created unexported work. Guarded purge refused with exit 3 and `branch feature/openshell-native not exported`. Export created only this ref:

```text
refs/heads/jackin/jackin-a14t09-cean/feature/openshell-native  ac54d53a3aea6ed579f9c32d40b1f9be20a72f4b
```

The temporary host HEAD stayed `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its only working-tree change remained `jackin.role.toml`, with diff SHA256 `74dc76bc66c717bfa19e7b12f4d41b53728fdbd0dab224d9f3b366bcf95b3871` before and after export. Guarded purge then deleted the sandbox.

## Local proof

### Image and sandbox

The sandbox was `jackin-a14t09-bntb` with launch ID `a14t09ompl01`, account `a14t09-omp-l`, and provider `jackin-a14t09-omp-l-omp`. It used the same derived image, `sha256:cde9c06c5e48fa4a4334cf91798a82ca5c4b9608c86ffd30ccce517669975d67`. The initial launch and same-launch reload reported the cached derived image. The reload also skipped the existing sandbox and uploaded project.

The sandbox reported `omp/18.18.3` and the same binary digest as docker. It ran as uid 1000 with no `sudo`. omp PID 676 was the child of `jackin-start omp` PID 372. The supervisor state contained `running 1`.

All three omp files had mode 0600 and the same placeholder counts as docker. The config and models file named `https://api.z.ai/api/anthropic`, `anthropic-messages`, `ZAI_API_KEY`, and `zai/glm-5.3`.

### Live response and tool use

The fixed request returned exit 0, exact `PONG`, the required API, provider, and model metadata, `stopReason=stop`, and 8,998 total tokens.

The tool request ran `ls -A`, returned the real sentinel root listing with `isError=false`, and finished with exact `TOOL-OK`. It reported 9,166 total tokens.

### Credential and network checks

The host key had zero matches in 659 bytes of raw environment output, 688,665 bytes of readable text files up to 4 MiB under `/sandbox`, inspect JSON, and every captured stderr stream. The environment held a 54-character broker value in `ZAI_API_KEY`. The three alternate paid-key variables were unset.

Inspect listed only `jackin-a14t09-omp-l-omp`, a loaded policy, and no closure violations. `curl` failed with exit 7 for `api.z.ai`, `example.com`, and `api.openai.com`. omp reached the selected provider and received both responses. No other provider egress or paid fallback occurred.

### Terminal, export, and purge

Two PTY attaches exited 0 after replaying the omp 18.18.3 screen and detaching. A shell pane printed `A14T09-SHELL-OK`, returned uid 1000 and `/sandbox/project`, and exited 0. omp PID 676 remained a child of `jackin-start` afterward.

Sandbox commit `1c22337b69c808daa76ebc90e3c11019b123089a` caused guarded purge to refuse with exit 3 and the unexported-branch error. Export created only this ref:

```text
refs/heads/jackin/jackin-a14t09-bntb/feature/openshell-native  1c22337b69c808daa76ebc90e3c11019b123089a
```

The host HEAD, working-tree change set, and diff SHA256 stayed unchanged. Guarded purge then deleted the sandbox.

## Cleanup

Both owned sandboxes were removed. Both exported refs were deleted. Both owned accounts and providers were removed. The `a14t09` workspace and `a14t09-sentinel` role were removed. The temporary sentinel clone was deleted after those checks.

The final docker sandbox inventory contains only the pre-existing `jackin-jackin-rqw3`, `jackin-jackin-3yuk`, and `jackin-arch-pr-dqq7`. The final local inventory contains only the pre-existing `jackin-a14t02b-995e`. Foreign providers were not changed. The original sentinel checkout remains clean and was not used as the project.

The role and derived images remain in the BuildKit cache under the unique `a14t09-sentinel` tags. Jackin has no image-removal command. Direct Docker removal was not used because only `jackin-image` may start Docker.

## Defects

No new product defect appeared. The earlier HTTP 404 is resolved on both gateways.

## G-FAST

Command: `mise exec -- mbx +1.99.1 xtask ci --fast`

Result: exit 0 in 410.267 seconds. Nextest ran 2,168 tests in 250.445 seconds, with 2,168 passed and 0 skipped. The run also passed fmt, clippy, deny, shear, comments, layers, pins, containers, errors, baseline, screen, demo-gate, and reuse. The fast gate skipped the docs build as defined by the repository gate.
