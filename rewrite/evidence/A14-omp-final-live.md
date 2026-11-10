# A14 final-head omp live proofs

Date: 2026-11-10. Base commit: `e275950ca3f5d82d17dfa49789f9582d3487088b`. Worktree: `/tmp/jackin-a14-omp-final`, branch `wt/a14-omp-final`.

## Verdict matrix

| Tuple | Agent | Provider | Configured model | `jackin-docker` | `jackin-local` |
|---|---|---|---|---|---|
| T10 | omp | Kimi | `kimi-for-coding` | **PASS** | **PASS** |
| T11 | omp | MiniMax | `minimax-m3` | **PASS** | **PASS** |
| T12 | omp | MiMo | `mimo-v2.6-pro` | **PASS** | **PASS** |
| T21 | omp | Anthropic | `claude-haiku-5-5` | **PASS** | **PASS** |

Every final pair passed the complete battery: cached same-launch reload, uid 1000 and no sudo, direct `jackin-start` parentage, exact provider/model live reply, successful real tool use with nonzero usage, placeholder and host-key disclosure checks, provider-only policy and non-agent egress refusal, two PTY detach/reattach replays, persistent shell state, exact-ref export with unchanged host state, guarded purge refusal, export, post-export purge, and exact-resource cleanup. There are no final tuple/gateway failures and no SKIP verdicts.

T11 used the unmodified lowercase catalog ID `minimax-m3`. On both gateways the live reply metadata identified provider `minimax-code` and vendor canonical model `MiniMax-M3`. This is a case-only canonicalization of the requested lowercase ID, not a catalog guess or edit.

## Vehicle and credential handling

The binary was `/tmp/jackin-a14-omp-final/target/debug/jackin`, built after forcing the changed source with:

```sh
rtk mise exec -- mbx +1.99.1 build -p jackin
```

OpenShell was 0.1.7. `jackin doctor` passed the binary, gateway registration, gateway health, pinned version, and engine checks on both gateways. Sandboxes reported `omp/18.18.3`.

The proof build/run used the authorized temporary enablement of T10, T11, T12, and T21 in `crates/jackin-core/src/agent.rs`: four `TupleState::Unavailable` values became `Supported` and their four reasons became `None` (8 insertions, 8 deletions). Those edits were restored after the proof. No other production code or catalog model changed, no production diff remains, and nothing was committed or pushed.

The isolated configuration was `/tmp/a14of-xdg`. Its OpenShell gateway directories were symbolic links to the existing mTLS registrations. Jackin gateway entries were created through `jackin gateway add`; no host Jackin configuration or manual account/provider configuration edit was used.

The current sentinel source stayed at `6722f8024623c76726ce0293fc7e93da47b5ee74` and clean. Each pair received a new temporary clone and a unique role, workspace, account, launch ID, provider, and sandbox with prefix `a14of<tuple><d|l>`. The only temporary host-clone modification was the role name in `jackin.role.toml`.

Each account used the authorized registered file-backed source directly through `--credential file:PATH`:

- T10: `file:/tmp/jackin-plan-keys/kimi`
- T11: `file:/tmp/jackin-plan-keys/minimax`
- T12: `file:/tmp/jackin-plan-keys/mimo`
- T21: `file:/tmp/jackin-plan-keys/anthropic`

No 1Password command, side provider, legacy vehicle config, or gateway credential discovery was used. Key values were never placed in command arguments, chat, evidence, or artifact logs. The capture process read each source only to count and redact disclosure in memory. It removed alternate provider-key variables from child environments. Raw sandbox environment and file scans stayed in memory and saved only match counts and byte counts.

## Per-pair live receipts

All replies were exact `PONG`. Every tool request executed bash `ls -A` in `/sandbox/project`, returned the real sentinel root including `Dockerfile` and `jackin.role.toml`, had `isError=false`, finished with exact `TOOL-OK`, and reported nonzero usage. The process column is `omp PID <- jackin-start PID`.

| Pair | Sandbox | Provider resource | omp provider / reply model | PONG tokens | Tool tokens | Process |
|---|---|---|---|---:|---:|---|
| T10 docker | `jackin-a14of10-gtui` | `jackin-a14of10d-omp` | `moonshot` / `kimi-for-coding` | 8,353 | 8,618 | 384 <- 65 |
| T10 local | `jackin-a14of10-iviv` | `jackin-a14of10l-omp` | `moonshot` / `kimi-for-coding` | 8,351 | 8,563 | 686 <- 372 |
| T11 docker | `jackin-a14of11-vqlv` | `jackin-a14of11d-omp` | `minimax-code` / `MiniMax-M3` | 8,992 | 9,162 | 382 <- 64 |
| T11 local | `jackin-a14of11-arj0` | `jackin-a14of11l-omp` | `minimax-code` / `MiniMax-M3` | 8,980 | 9,144 | 687 <- 373 |
| T12 docker | `jackin-a14of12-qhdl` | `jackin-a14of12d-omp` | `xiaomi-token-plan-sgp` / `mimo-v2.6-pro` | 8,822 | 9,009 | 384 <- 64 |
| T12 local | `jackin-a14of12-svi7` | `jackin-a14of12l-omp` | `xiaomi-token-plan-sgp` / `mimo-v2.6-pro` | 8,800 | 8,986 | 688 <- 373 |
| T21 docker | `jackin-a14of21-x7x9` | `jackin-a14of21d-omp` | `anthropic` / `claude-haiku-5-5` | 14,322 | 14,597 | 382 <- 64 |
| T21 local | `jackin-a14of21-2gkb` | `jackin-a14of21l-omp` | `anthropic` / `claude-haiku-5-5` | 14,305 | 14,580 | 687 <- 373 |

T10 used `openai-completions` at `https://api.kimi.com/coding/v1`. T11 used `anthropic-messages` at `api.minimax.io`. T12 used `openai-completions` at `https://token-plan-sgp.xiaomimimo.com/v1`. T21 used `anthropic-messages` at `api.anthropic.com`.

Every same-launch reload printed `image derived: cached`, `image: skipped, the image is cached`, `sandbox: skipped, the sandbox exists`, and `upload: skipped, the project is uploaded`. The final T10 docker run also began from the cache populated by its preceding harness attempts; its same-launch reload independently proved all cached lines.

All sandboxes ran as `uid=1000(sandbox)` and had no `sudo`. omp was a direct child of `/bin/sh /usr/local/bin/jackin-start omp` as user `sandbox`, and the same parentage remained after the terminal checks. Inspect showed phase Ready, a loaded policy, no closure violations, and exactly the one provider resource named in the table.

## Credential, policy, and billing checks

The three omp files had mode 0600 in every pair. Placeholder counts were one in `/sandbox/.omp/config.toml`, one in `/sandbox/.omp/auth.json`, and zero in `/sandbox/.omp/agent/models.yml`. The rendered config named the requested provider/model; `models.yml` carried only the environment variable name, never the host key.

| Pair | Brokered environment metadata | Env bytes scanned | File bytes scanned | Non-agent curl results |
|---|---|---:|---:|---|
| T10 docker | `KIMI_API_KEY` length 55 | 659 | 683,889 | `api.kimi.com=7`, `example.com=7`, `api.openai.com=7` |
| T10 local | `KIMI_API_KEY` length 56 | 660 | 683,940 | `api.kimi.com=7`, `example.com=7`, `api.openai.com=7` |
| T11 docker | `MINIMAX_CODE_API_KEY` length 64 | 676 | 684,616 | `api.minimax.io=7`, `example.com=7`, `api.openai.com=7` |
| T11 local | `MINIMAX_CODE_API_KEY` length 63 | 675 | 685,003 | `api.minimax.io=7`, `example.com=7`, `api.openai.com=7` |
| T12 docker | `XIAOMI_TOKEN_PLAN_SGP_API_KEY` length 72 | 693 | 684,798 | `token-plan-sgp.xiaomimimo.com=7`, `example.com=7`, `api.openai.com=7` |
| T12 local | `XIAOMI_TOKEN_PLAN_SGP_API_KEY` length 73 | 694 | 683,987 | `token-plan-sgp.xiaomimimo.com=7`, `example.com=7`, `api.openai.com=7` |
| T21 docker | `ANTHROPIC_API_KEY` length 60 | 669 | 684,032 | `api.anthropic.com=7`, `example.com=7`, `api.openai.com=7` |
| T21 local | `ANTHROPIC_API_KEY` length 61 | 670 | 683,938 | `api.anthropic.com=7`, `example.com=7`, `api.openai.com=7` |

Only the selected credential variable was set in each sandbox. All alternate Kimi, MiniMax, MiMo, Z.AI, Anthropic, and OpenAI key variables were unset. The host key had zero matches in environment output, readable files up to 4 MiB under `/sandbox`, `jackin inspect --json`, and every captured stdout/stderr stream. A final independent scan of 654 artifacts totaling 9,970,516 bytes also found zero matches for each of the four host keys.

The selected provider host, an unrelated host, and an alternate paid-provider host all refused direct `curl` with exit 7, while omp completed both upstream model calls. Thus non-agent binaries received neither provider access nor the brokered credential. Only the selected provider was attached.

T10, T11, and T12 used their class-S plan credentials at the Kimi coding, MiniMax coding-plan, and Singapore MiMo Token Plan endpoints respectively. T21 used its class-A Anthropic API key at `api.anthropic.com`. The named credential was the only attached credential and produced the recorded nonzero upstream usage. No alternate paid credential or paid fallback was present. Client-reported cost estimates were not used as billing evidence.

## Terminal, export, and purge receipts

Two 80-column PTY attaches per pair replayed the omp 18.18.3 screen, detached with Ctrl-P Ctrl-Q, and exited 0. A first shell printed `A14OF-SHELL-OK`, returned uid 1000 and `/sandbox/project`, and wrote a marker. A second shell read it and printed `A14OF-SHELL-PERSISTED`. Both shells exited 0 and omp remained under `jackin-start`.

Each pair created one sandbox commit on `feature/openshell-native`. Guarded purge refused with exit 3 and the exact error form:

```text
error: the work in sandbox <sandbox> is not exported (branch feature/openshell-native not exported); run jackin export <sandbox> or pass --force
```

Export created exactly one namespaced ref, shown below. The temporary host clone HEAD remained `6722f8024623c76726ce0293fc7e93da47b5ee74`; its status remained only ` M jackin.role.toml`; its diff SHA256 and index SHA256 were byte-identical before and after export. Guarded purge then exited 0 and deleted the sandbox.

| Pair | Exported ref | Commit |
|---|---|---|
| T10 docker | `refs/heads/jackin/jackin-a14of10-gtui/feature/openshell-native` | `ea442bd038ced6f2a6af569461fa2ddfc138206e` |
| T10 local | `refs/heads/jackin/jackin-a14of10-iviv/feature/openshell-native` | `e0d203a3c250e1dc0e8353bc243d03f78ceefadd` |
| T11 docker | `refs/heads/jackin/jackin-a14of11-vqlv/feature/openshell-native` | `2cd69ad02be7482d2227617c9e372439c623bd28` |
| T11 local | `refs/heads/jackin/jackin-a14of11-arj0/feature/openshell-native` | `88e0b520fc43b8abbf692a12c27c9b0dff211ba8` |
| T12 docker | `refs/heads/jackin/jackin-a14of12-qhdl/feature/openshell-native` | `8caafac3a17029da169f198d1fc2321bd552c3ba` |
| T12 local | `refs/heads/jackin/jackin-a14of12-svi7/feature/openshell-native` | `9bca64778c8e1d65f909253fe698bbd0c1a8599b` |
| T21 docker | `refs/heads/jackin/jackin-a14of21-x7x9/feature/openshell-native` | `60ece79d359d09d6825ff1112721fd5a42712384` |
| T21 local | `refs/heads/jackin/jackin-a14of21-2gkb/feature/openshell-native` | `73ee4883acef757d80fe947bcd828f9828e4bf1a` |

Every exported ref was deleted after the post-export purge.

## Dated harness corrections, not product failures

The final matrix counts only the complete final runs above. Earlier proof-harness mistakes were corrected and their owned resources removed; none is hidden or converted from a product SKIP into PASS.

- 2026-11-10T04:31:58Z, T10 docker: the capture wrapper rejected its own argument order with `the following arguments are required: --key`. No launch occurred.
- 2026-11-10T04:33:03Z, T10 docker: the wrapper incorrectly forced `DOCKER_CONTEXT=default`; image build stopped with `ERROR: use \`docker --context=orbstack buildx\` to switch to context "orbstack"`. Account, workspace, role, and provider cleanup succeeded; no sandbox existed.
- 2026-11-10T04:34:04Z, T10 docker: launch and cached reload succeeded, but the wrapper parsed the launch output's `sandbox<TAB>launch-id` as one invalid sandbox value. `jackin inspect` exited 2 with `invalid value for '<SANDBOX>'`. The owned sandbox `jackin-a14of10-gtui` was force-purged before the valid rerun; remaining owned resources were removed exactly.
- 2026-11-10T04:34:45Z, T10 docker: exact PONG and the real successful tool call had already passed when the wrapper read `args` and `isError` from the wrong JSON levels. Its assertion reported `args: None, isError: None` despite the recorded bash `ls -A`, real listing, `TOOL-OK`, and 8,586 tokens. The parser was corrected and the complete pair rerun. This was not a live-tool failure.
- 2026-11-10T04:37:18Z, T10 local: with no explicit context, the local default builder stopped with `ERROR: use \`docker --context=default buildx\` to switch to context "default"`. The wrapper then pinned `orbstack` for docker and `default` for local. Cleanup succeeded; no sandbox existed.

No final product defect or final tuple/gateway failure occurred, so no blocker update is claimed.

## Cleanup and retained review material

Final inventory checks found zero `a14of` sandboxes and zero `a14of` providers on either gateway. The isolated account, role, and workspace listings contained no proof entries. No temporary sentinel clone or exported ref remained. The original sentinel checkout remained clean at its original HEAD. Foreign resources and the registered host accounts were not changed.

Unique role and derived images remain in BuildKit cache. Jackin has no image-removal command; no direct Docker, Podman, or host agent execution was used.

Sanitized capture receipts and summary assertions remain under `/tmp/a14of-artifacts` for independent review. The four temporary catalog flips were restored after the proof; only this evidence file remains uncommitted. No full G-FAST, commit, or push was performed.
