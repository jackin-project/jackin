# A14 final-head live proof for Claude Code

Date: 2026-11-10. Base commit: `e275950ca3f5d82d17dfa49789f9582d3487088b`. Worktree: `/tmp/jackin-a14-claude-final`, branch `wt/a14-claude-final`.

## Verdicts

| Tuple | Agent and provider | Exact requested model | jackin-docker | jackin-local |
|---|---|---|---|---|
| T13 | Claude Code with Kimi | `kimi-for-coding` | **FAIL** | **FAIL** |
| T15 | Claude Code with MiniMax | `MiniMax-M3` | **PASS** | **PASS** |
| T16 | Claude Code with MiMo | `mimo-v2.6-pro` | **PASS** | **PASS** |

T13 passed the print-mode reply, tool, credential, egress, shell, export, and plan-usage checks on both gateways. It failed the required supervised Claude check on both gateways. Interactive Claude tried `api.anthropic.com`, the provider-only policy refused that connection, and Claude exited 1. No running Claude direct child remained under `jackin-start`. Two attach sessions replayed the same failure. A successful print-mode call does not turn this failure into a pass.

T15 and T16 passed every requested check on both gateways. No SKIP result is reported as PASS.

## Proof setup

The binary was `/tmp/jackin-a14-claude-final/target/debug/jackin`, built with `mise exec -- mbx +1.99.1 build -p jackin`. OpenShell reported 0.1.7. `jackin doctor` passed the binary, gateway registration, gateway health, pinned version, and engine checks on both gateways.

The proof used a temporary source change enabling T13, T15, and T16 as `Supported` in `crates/jackin-core/src/agent.rs`. Those six changed lines were restored after the proof, leaving no production diff. The renderer, profile assets, credential variable names, tests, production configuration, blockers, plan, design, and decisions were not changed. No commit or push was created. The only commits were disposable in-sandbox export fixtures, and all their exported refs were deleted.

The proof used `XDG_CONFIG_HOME=/tmp/a14cfinal-xdg`. Its OpenShell registry path linked to the existing gateway registration without changing the host Jackin configuration. Every account, role, workspace, launch, provider, and sandbox owned by this run used the `a14cf` or `a14cfin` namespace.

The temporary sentinel clone was `/tmp/jackin-a14cfinal-sentinel` at `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its only host edit named the role `a14cfinal-sentinel`. The existing Claude instruction mapping was retained, with no role model pin. `jackin role check` passed. Every launch selected the exact model explicitly.

The authorized credential sources were the existing private files `/tmp/jackin-plan-keys/kimi`, `/tmp/jackin-plan-keys/minimax`, and `/tmp/jackin-plan-keys/mimo`. Metadata inspection found regular mode-0600 files. Jackin read them through `file:` credential sources. No `op` command ran. No credential value was printed, written to a log, passed as an argument, or included in this evidence. A local capture program removed provider and paid-fallback variables from child environments, compared output with the source value in memory, and saved only sanitized output or match counts.

Each tuple used a unique account and provider on each gateway.

| Tuple | Account | Exact provider |
|---|---|---|
| T13 | `a14cf-claude-kimi` | `jackin-a14cf-claude-kimi-claude` |
| T15 | `a14cf-claude-minimax` | `jackin-a14cf-claude-minimax-claude` |
| T16 | `a14cf-claude-mimo` | `jackin-a14cf-claude-mimo-claude` |

## Common image, configuration, and hygiene results

Every sandbox ran Claude Code 2.1.311 as uid 1000 with no `sudo`. The Claude binary SHA256 was `2e2496c9a8065df93acb0fca2d341f59a6b8777649b2e025ab8c9bcd5eef61f3`. The derived image was `sha256:0022bc2203ede2da89531e880caad557136a7db7b349846cdf955179130dc06c`.

The initial T13 docker launch built the derived image. Every same-launch reload reported `image derived: cached`, `image: skipped, the image is cached`, `sandbox: skipped, the sandbox exists`, and `upload: skipped, the project is uploaded`. The T15 and T16 launches also used the cached derived image, and their reloads proved the same cached path.

Every rendered `settings.json` had mode 0600, zero placeholders, and only the sanctioned base URL. No settings file contained `ANTHROPIC_AUTH_TOKEN` or `ANTHROPIC_API_KEY`. No manual `CLAUDE_CONFIG_DIR`, side provider, or manual configuration was used.

| Tuple | Rendered settings |
|---|---|
| T13 | `{"env":{"ANTHROPIC_BASE_URL":"https://api.kimi.ai/coding/"}}` |
| T15 | `{"env":{"ANTHROPIC_BASE_URL":"https://api.minimax.io/anthropic"}}` |
| T16 | `{"env":{"ANTHROPIC_BASE_URL":"https://token-plan-sgp.xiaomimimo.com/anthropic"}}` |

Every `.credentials.json` had mode 0600 and one placeholder. T13 received its broker credential as `ANTHROPIC_API_KEY`. T15 and T16 received theirs as `ANTHROPIC_AUTH_TOKEN`. The redacted environment listings contained no alternate provider or paid-fallback variable.

The source credential had zero matches in each sandbox's raw environment output, readable regular files up to 4 MiB under `/sandbox`, `jackin inspect --json`, and captured stdout and stderr. The capture logs contain only byte counts and zero match counts for raw scans. No host key appeared in process arguments or evidence.

Each sandbox had only its matching tuple provider attached and no policy closure violation. For every tuple on both gateways, non-agent `curl` returned exit 7 for the provider host, `example.com`, `api.anthropic.com`, and `api.openai.com`. Claude reached the selected provider through its permitted binary path. Non-agent binaries received no usable provider route or paid-fallback credential.

## Live replies and real tool use

Every print-mode request below returned exit 0, exact `PONG`, `is_error=false`, and `api_error_status=null`. Every tool request executed one real `Bash` call for `ls -A /sandbox/project`, returned the sentinel root listing with `is_error=false`, finished with exact `TOOL-OK`, and had no permission denials.

| Cell | PONG model metadata | PONG input, output, and cache-read tokens | Tool input, output, and cache-read tokens |
|---|---|---:|---:|
| T13 docker | `kimi-for-coding` | 4,608 / 29 / 6,912 | 2,300 / 118 / 20,992 |
| T13 local | `kimi-for-coding` | 2,036 / 20 / 9,472 | 2,273 / 122 / 20,992 |
| T15 docker | `MiniMax-M3` | 12,057 / 4 / 182 | 12,232 / 60 / 12,444 |
| T15 local | `MiniMax-M3` | 12,030 / 4 / 197 | 12,198 / 53 / 12,447 |
| T16 docker | `mimo-v2.6-pro` | 4,805 / 18 / 7,424 | 5,076 / 89 / 19,584 |
| T16 local | `mimo-v2.6-pro` | 4,020 / 18 / 8,192 | 2,226 / 91 / 22,400 |

The T15 request, assistant messages, and `modelUsage` key used exact uppercase `MiniMax-M3`. Claude Code's client-side `canonicalModel` field normalized it to `minimax-m3`. This is client metadata, not a different requested model. T13 and T16 retained their exact canonical model names. Claude printed an `unrecognized_model` warning for all three custom models. Client `costUSD` fields had `costBasis="unknown"`, so they are not billing evidence.

## Supervised process, replay, and shell persistence

| Cell | Process result | Terminal result |
|---|---|---|
| T13 docker | No Claude child remained. `jackin-start claude` PID 64 reported `exited 2 1`. | Two attaches exited 0 and replayed the `api.anthropic.com` failure. Shell persistence passed. |
| T13 local | No Claude child remained. `jackin-start claude` PID 372 had only its polling child. | Two attaches exited 0 and replayed the same failure. Shell persistence passed. |
| T15 docker | Claude PID 373 was the direct child of `jackin-start` PID 64. State stayed `running 1`. | Two attaches exited 0 and replayed the Claude screen. Shell persistence passed. |
| T15 local | Claude PID 678 was the direct child of `jackin-start` PID 372. State stayed `running 1`. | Two attaches exited 0 and replayed the Claude screen. Shell persistence passed. |
| T16 docker | Claude PID 356 was the direct child of `jackin-start` PID 64. State stayed `running 1`. | Two attaches exited 0 and replayed the Claude screen. Shell persistence passed. |
| T16 local | Claude PID 676 was the direct child of `jackin-start` PID 372. State stayed `running 1`. | Two attaches exited 0 and replayed the Claude screen. Shell persistence passed. |

Each PTY attach detached with Ctrl-P Ctrl-Q. Each shell printed `A14CF-SHELL-OK`, returned uid 1000 and `/sandbox/project`, and wrote a marker. A later sandbox command read `A14CF-SHELL-PERSIST`, proving shell filesystem persistence. T15 and T16 kept their Claude direct child after the shell exited.

### Dated T13 failure

On 2026-11-10, both T13 attach replays contained these exact errors for attempts 1 and 2:

```text
Failed to connect to api.anthropic.com: ECONNREFUSED
jackin-start: claude exited with code 1
```

The docker supervisor state was `exited 2 1`. The local replay reported the same two failed attempts. The settings file still contained only `https://api.kimi.ai/coding/`, and the only attached provider was `jackin-a14cf-claude-kimi-claude`. The print-mode requests made through that sanctioned configuration succeeded. The interactive failure is therefore specific to the required supervised path, not a missing live credential or a print-mode provider failure.

## Plan funding and paid-fallback checks

`jackin usage --json` returned live plan windows for Kimi and MiniMax. It returned `unsupported` for MiMo, so this proof claims no MiMo quota delta.

| Provider | Before | After the tuple | Final snapshot after cleanup |
|---|---|---|---|
| Kimi | 28% five-hour, 65% weekly | 34% five-hour, 67% weekly | 39% five-hour, 69% weekly |
| MiniMax | 1% five-hour, 1% weekly | 1% five-hour, 1% weekly | 1% five-hour, 1% weekly |
| MiMo | Usage windows unsupported | Usage windows unsupported | Usage windows unsupported |

Kimi's displayed plan usage increased after its live calls. The evidence does not assign the later increase after cleanup to a specific client. MiniMax's displayed percentages did not move, but its quota endpoint remained available and every model call reported nonzero usage. MiMo used the authorized Token Plan credential only through `token-plan-sgp.xiaomimimo.com`, and every model call reported nonzero usage.

For all three tuples, only the matching plan provider was attached, alternate paid-key variables were absent, and paid-provider egress failed for non-agent binaries. These observations support plan-funded use with no paid fallback. No client-estimated cost is used as billing proof.

## Export, guarded purge, and unchanged host state

Every cell created a disposable sandbox commit. Guarded purge refused each sandbox with exit 3 and `branch feature/openshell-native not exported`. Export created exactly the expected ref at the sandbox commit. Guarded purge then deleted the sandbox.

| Cell | Sandbox | Launch ID | Exported commit |
|---|---|---|---|
| T13 docker | `jackin-a14cfin-nku9` | `a14cfkimid01` | `57f18794cb42cdfae56273bccb68e230a1f87f2e` |
| T13 local | `jackin-a14cfin-zag4` | `a14cfkimil01` | `c3a2e912cd97f985ddc86c481fcd4feeabd92084` |
| T15 docker | `jackin-a14cfin-i7k8` | `a14cfmmd01` | `4891bdd867d8e03b0ef548e650945dcdb69944df` |
| T15 local | `jackin-a14cfin-zsdd` | `a14cfmml01` | `a3a457589860482414c5bf044f5bf4d03fd6501e` |
| T16 docker | `jackin-a14cfin-out3` | `a14cfmimod01` | `e150495401325d918b83c0d03767ea89e38f6cc6` |
| T16 local | `jackin-a14cfin-7zbf` | `a14cfmimol01` | `0897124396024f68b104a2ce7cd6e90ad1a5ad96` |

Each exact ref had this form, with the cell's sandbox and commit substituted:

```text
refs/heads/jackin/<sandbox>/feature/openshell-native  <exported-commit>
```

The temporary host HEAD stayed `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its status stayed one modified `jackin.role.toml`. Before and after every export, the working-diff SHA256 was `c3f28216ddd085c719ed3c8338d7a54d7b74cf4b1827556dcb5264e29ec70f6d`, the cached-diff SHA256 was `01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b`, and the index-listing SHA256 was `f362752300d6233a54a6e3c5606614cbdfb7db4d0b8709082a37a9ca21519ceb`. Export did not change the host tree, index, or HEAD.

## Cleanup

All six sandboxes were deleted through the guarded post-export purge path. All six exported refs were deleted. No `refs/heads/jackin` ref remains in the temporary sentinel.

The three accounts were removed. Exact provider cleanup covered both gateways without broad shared-account deletion. The three explicit local provider deletes reported `not found` because those instances were already absent. Final provider lists confirmed that no `a14cf` provider remains on either gateway. Foreign providers and sandboxes were not changed.

The `a14cfinal` workspace and `a14cfinal-sentinel` role registration were removed. The temporary sentinel clone and isolated configuration remain for independent review. The role and derived images remain in the BuildKit cache under their unique tags. Jackin has no image-removal command, and no direct Docker, Podman, or host agent execution was used.

The three-tuple enablement was restored after the proof. The worktree retains only this evidence and sanitized `.audit` capture artifacts, with no production diff. No full G-FAST run was requested or performed.
