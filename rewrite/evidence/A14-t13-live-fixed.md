# A14 T13 supervised live re-proof

Date: 2026-11-10. Base commit: `8c444a97ad719e0f087bd94c77ae1255e78d64ff`. Worktree: `/tmp/jackin-a14-t13-live-fixed`, branch `wt/a14-t13-live-fixed`.

## Verdicts

| Gateway | Verdict | Exact supervised failures |
|---|---|---|
| `jackin-docker` | **FAIL** | `/status` reported `Model: Default (claude-opus-5-5)`, not `kimi-for-coding`. The final supervisor state was `running 2`, not `running 1`. |
| `jackin-local` | **FAIL** | The interactive banner reported `Opus 5.5`, not `kimi-for-coding`. The exact `T13-PTY-OK` reply was not obtained. The final supervisor state was `running 2`, not `running 1`. |

The auth-token correction worked on both gateways. The sanitized supervisor record delivered `ANTHROPIC_AUTH_TOKEN` without `ANTHROPIC_API_KEY`, settings contained only the Kimi base URL, and no captured interactive session attempted `api.anthropic.com`. These facts do not satisfy the exact-model requirement. No SKIP or print-mode result is reported as PASS.

The independent hermetic review in `/tmp/a14-t13-auth-review.md` is `REVIEW_PASS`. Its recorded full gate passed 2,236 tests. This run did not repeat full G-FAST.

## Setup and credential delivery

The proof binary was built with `mise exec -- mbx +1.99.1 build -p jackin`. OpenShell reported 0.1.7. `jackin doctor` passed on both gateways.

The only temporary source change set T13 to `Supported` in `crates/jackin-core/src/agent.rs`. It was restored after the proof. `git diff --exit-code -- crates/jackin-core/src/agent.rs` exited 0, so no proof enablement remains. No commit or push was created.

The isolated configuration used `XDG_CONFIG_HOME=/tmp/a14t13fix-xdg`. Unique resources used the `a14t13fix` namespace. The temporary sentinel was `/tmp/jackin-a14t13fix-sentinel` at `6722f8024623c76726ce0293fc7e93da47b5ee74`, with the role renamed `a14t13fix-sentinel`. The account was `a14t13fix-claude-kimi`, and the exact provider was `jackin-a14t13fix-claude-kimi-claude`.

The only credential source was `/tmp/jackin-plan-keys/kimi`, an existing private mode-0600 file. Jackin read it through the `file:` credential source. No `op` command ran. No credential value entered chat, logs, command arguments, or evidence. Capture programs compared output with the source value in memory and saved only sanitized output or match counts.

Every sandbox ran Claude Code 2.1.311 as uid 1000 with no `sudo`. The binary SHA256 was `2e2496c9a8065df93acb0fca2d341f59a6b8777649b2e025ab8c9bcd5eef61f3`. The initial docker launch built the derived image. Every same-launch reload reported the derived image, sandbox, and upload as cached or skipped.

The rendered settings on both gateways were exactly:

```json
{"env":{"ANTHROPIC_BASE_URL":"https://api.kimi.ai/coding/"}}
```

The settings contained no auth-token or API-key entry. The sanitized `env-kinds` supervisor record contained `main ANTHROPIC_AUTH_TOKEN placeholder` and no `ANTHROPIC_API_KEY`. The redacted exec environment also contained `ANTHROPIC_AUTH_TOKEN` without `ANTHROPIC_API_KEY`. The generic `.credentials.json` retained one `ANTHROPIC_API_KEY` placeholder, but no API-key value was delivered. Settings had zero placeholders, and the credentials file had one.

Docker `/status` independently reported `Auth token: ANTHROPIC_AUTH_TOKEN` and `Anthropic base URL: https://api.kimi.ai/coding/`.

Each sandbox had only `jackin-a14t13fix-claude-kimi-claude` attached and no policy closure violation. The profile permits only `api.kimi.ai` for `/usr/local/bin/claude`. Non-agent `curl` returned exit 7 for `api.kimi.ai`, `example.com`, `api.anthropic.com`, and `api.openai.com` on both gateways. The source credential had zero matches in raw environment output, regular files up to 4 MiB under `/sandbox`, inspect JSON, and captured stdout and stderr.

## Supervised interactive results

The fixed PTY prompt was:

```text
Use the Bash tool once to run ls -A /sandbox/project. After the tool succeeds, reply with exactly T13-PTY-OK.
```

The driver used `jackin attach`, completed Claude's first-run screens, selected `Yes, I trust this folder`, and submitted the prompt and Enter separately. Claude's auto permission mode executed the directory tool without a separate approval prompt.

On docker, the supervised session ran `ls -A /sandbox/project`, listed one directory, and returned exact `T13-PTY-OK`. The subsequent interactive `/status` reported `Model: Default (claude-opus-5-5)`. This is direct evidence of the wrong model, so the cell is FAIL.

On local, the supervised session also ran `ls -A /sandbox/project`. The first timed `/status` input was appended to the task, and the model answered that extra text instead of returning `T13-PTY-OK`. A second fixed-prompt attempt did not produce the exact reply before detach. The local cell therefore lacks the required exact reply. Its interactive banner reported `Opus 5.5`, which independently fails the exact-model requirement.

Both cells recorded zero `api.anthropic.com` attempts in the interactive, status, and replay captures. No manual model selection, manual settings, side provider, or `claude -p` substitute was used.

Two later reattach sessions on each gateway exited 0 and replayed the interactive screens. Shell entry and exit preserved the same Claude child in each sandbox.

| Cell | Final direct-child relationship | Final supervisor state | Shell persistence |
|---|---|---|---|
| Docker | `/usr/local/bin/claude` PID 2130, child of `jackin-start` PID 64 | `running 2` | `A14CF-SHELL-PERSIST` read after shell exit |
| Local | `/usr/local/bin/claude` PID 1410, child of `jackin-start` PID 372 | `running 2` | `A14CF-SHELL-PERSIST` read after shell exit |

The initial state on each gateway was `running 1`. Driver errors on the workspace trust screen required one restart per gateway before the successful tool runs. The final `running 2` states are recorded as failures, not rewritten as `running 1`.

## Subscription usage and paid fallback

Before the proof, Kimi reported 59% five-hour usage and 77% weekly usage. After both cells, it reported 68% five-hour usage and 81% weekly usage. Only the Kimi provider was attached to each sandbox. Alternate paid-provider variables were absent, and non-agent access to paid-provider hosts failed. No paid fallback was configured or observed. Client model labels and estimated costs are not used as billing evidence.

## Export and guarded purge

Guarded purge refused each sandbox with exit 3 and `branch feature/openshell-native not exported`. Export created the exact expected ref, and guarded purge then deleted the sandbox.

| Cell | Sandbox | Launch ID | Exported commit |
|---|---|---|---|
| Docker | `jackin-a14t13f-qci5` | `a14t13fixd01` | `777c6ca0ca434c7a344c92a506e6a96f6fd8a38b` |
| Local | `jackin-a14t13f-ex46` | `a14t13fixl01` | `3e3c7878c280eb96b6756ae0604cee6c7f09a3c3` |

The refs were `refs/heads/jackin/jackin-a14t13f-qci5/feature/openshell-native` and `refs/heads/jackin/jackin-a14t13f-ex46/feature/openshell-native`, each at the listed commit. Both refs were deleted after the post-export purge.

The temporary host HEAD stayed `6722f8024623c76726ce0293fc7e93da47b5ee74`, with only `jackin.role.toml` modified. Before and after each export, the working-diff SHA256 was `ad790408619542a818d04b2e13e1492c6fb293aa35e946be9b6421473af14f28`, the cached-diff SHA256 was `01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b`, and the index-listing SHA256 was `f362752300d6233a54a6e3c5606614cbdfb7db4d0b8709082a37a9ca21519ceb`. Export did not change the host tree, index, or HEAD.

## Cleanup and remaining records

Both sandboxes and both exported refs were deleted. The account, workspace, and role registration were removed. Exact provider cleanup covered both gateways without broad shared-account deletion. The explicit local provider delete reported `not found` because that instance was already absent. Final provider lists confirmed no `a14t13fix` provider on either gateway. Foreign resources were not changed.

The temporary sentinel and isolated configuration remain for independent verification. The worktree retains only this evidence, the blocker update, and sanitized `.audit` records. The T13 production enablement is restored. No full G-FAST, commit, or push occurred.
