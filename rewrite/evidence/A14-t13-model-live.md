# A14 T13 integrated-model supervised live proof

Date: 2026-11-10. Base commit: `15b0191fa4b2c3834ae5df3780f1b530783cf7b5`. Worktree: `/tmp/jackin-a14-t13-model-live`, branch `wt/a14-t13-model-live`.

## Verdicts

| Gateway | Verdict | Final sandbox | Exact model | Exact reply | Final supervisor |
|---|---|---|---|---|---|
| `jackin-docker` | **PASS** | `jackin-a14t13m-qux2` | `/status`: `Model: kimi-for-coding` | `T13-PTY-OK` after `ls -A /sandbox/project` listed one directory | `running 1` |
| `jackin-local` | **PASS** | `jackin-a14t13m-115q` | `/status`: `Model: kimi-for-coding` | `T13-PTY-OK` after the directory tool listed one directory | `running 1` |

Both final fresh launches completed the real interactive PTY task with no restart and no `running 2` state. No manual `/model`, manual settings, `ANTHROPIC_MODEL`, or print-mode substitute was used. Both interactive sessions recorded zero `api.anthropic.com` attempts.

An earlier docker launch, `jackin-a14t13m-als5`, ended at `exited 1 1` when the initial driver mishandled the trust screen before submitting a task. It is not used as proof. That sandbox and its exported ref were cleaned. The passing docker result above comes from the separate fresh `a14t13mld02` launch.

The integrated change has 2,311 tests passing and independent model review `REVIEW_PASS`. This run did not repeat full G-FAST.

## Setup and rendered configuration

The proof binary was built with `mise exec -- mbx +1.99.1 build -p jackin`. OpenShell reported 0.1.7. `jackin doctor` passed on both gateways.

The only temporary source change set T13 to `Supported` in `crates/jackin-core/src/agent.rs`. It was restored after the proof. `git diff --exit-code -- crates/jackin-core/src/agent.rs` exited 0. No commit or push was created.

The isolated configuration used `XDG_CONFIG_HOME=/tmp/a14t13ml-xdg`. Unique resources used the `a14t13ml` namespace. The temporary sentinel was `/tmp/jackin-a14t13ml-sentinel` at `6722f8024623c76726ce0293fc7e93da47b5ee74`, with the role renamed `a14t13ml-sentinel`. The account was `a14t13ml-claude-kimi`, and the exact provider was `jackin-a14t13ml-claude-kimi-claude`.

The only credential source was `/tmp/jackin-plan-keys/kimi`, an existing private mode-0600 file. Jackin read it through the `file:` credential source. No `op` command ran. Capture programs compared output with the source value in memory and saved only sanitized output or match counts. No credential value entered chat, logs, command arguments, or evidence.

Both final settings documents contained exactly the required model and base URL. The JSON serializer placed `env` before `model`:

```json
{"env":{"ANTHROPIC_BASE_URL":"https://api.kimi.ai/coding/"},"model":"kimi-for-coding"}
```

No settings field contained an authentication entry, API key, model environment selector, or placeholder. `.credentials.json` was `{}` with zero placeholders. The sanitized supervisor record delivered `main ANTHROPIC_AUTH_TOKEN placeholder` and no `ANTHROPIC_API_KEY`. The redacted environment contained `ANTHROPIC_AUTH_TOKEN` without `ANTHROPIC_API_KEY` or `ANTHROPIC_MODEL`.

Both `/status` captures independently reported `Auth token: ANTHROPIC_AUTH_TOKEN`, `Anthropic base URL: https://api.kimi.ai/coding/`, and `Model: kimi-for-coding`. Setting sources were `User settings`.

Every sandbox ran Claude Code 2.1.311 as uid 1000 with no `sudo`. The binary SHA256 was `2e2496c9a8065df93acb0fca2d341f59a6b8777649b2e025ab8c9bcd5eef61f3`. Every same-launch reload reported the derived image, sandbox, and upload as cached or skipped.

Each final sandbox had only `jackin-a14t13ml-claude-kimi-claude` attached and no policy closure violation. The profile permits only `api.kimi.ai` for `/usr/local/bin/claude`. Non-agent `curl` returned exit 7 for `api.kimi.ai`, `example.com`, `api.anthropic.com`, and `api.openai.com`. The source credential had zero matches in raw environment output, regular files up to 4 MiB under `/sandbox`, inspect JSON, and captured stdout and stderr.

## Supervised interactive proof

The PTY driver completed the first-run theme, security, and workspace-trust screens. It allowed the terminal replay to settle before sending keys. It selected `Yes, I trust this folder` with separate selection and confirmation events. The task required an actual Bash call, not a simulated response.

On docker, Claude displayed `$ ls -A /sandbox/project`, reported `listed 1 directory`, and returned the assistant line `T13-PTY-OK`. On local, Claude also reported `listed 1 directory` and returned the same exact assistant line. `/status` was submitted only after task completion, so it did not contaminate either task.

Two later reattach sessions on each gateway exited 0 and replayed the successful interactive screens. Shell entry and exit preserved the same direct child.

| Cell | Direct-child relationship | Shell persistence | Final state |
|---|---|---|---|
| Docker | `/usr/local/bin/claude` PID 376, child of `jackin-start` PID 64 | `A14CF-SHELL-PERSIST` read after shell exit | `running 1` |
| Local | `/usr/local/bin/claude` PID 683, child of `jackin-start` PID 372 | `A14CF-SHELL-PERSIST` read after shell exit | `running 1` |

No final proof used `Default`, `claude-opus-5-5`, or `Opus 5.5`. Claude warned only that the custom model was unrecognized and assumed a 200k context.

## Subscription usage and paid fallback

Before the proof, Kimi reported 27% five-hour usage and 93% weekly usage. After both final cells, it reported 34% five-hour usage and 96% weekly usage. Only the Kimi provider was attached to each sandbox. Alternate paid-provider variables were absent, and non-agent access to paid-provider hosts failed. No paid fallback was configured or observed. Client cost labels are not used as billing evidence.

## Export and guarded purge

Guarded purge refused each final sandbox with exit 3 and `branch feature/openshell-native not exported`. Export created the exact expected ref, and guarded purge then deleted the sandbox.

| Cell | Launch ID | Exported ref | Exported commit |
|---|---|---|---|
| Docker | `a14t13mld02` | `refs/heads/jackin/jackin-a14t13m-qux2/feature/openshell-native` | `79347713b009e6f1f34af2171c7adf0ef85613c8` |
| Local | `a14t13mll01` | `refs/heads/jackin/jackin-a14t13m-115q/feature/openshell-native` | `054b3e2e5d4fb49f2934120c2ee17f5b61fb9dde` |

Both refs were deleted after the post-export purge. The discarded first docker attempt also completed guarded purge, export, and ref deletion before the final fresh launch.

The temporary host HEAD stayed `6722f8024623c76726ce0293fc7e93da47b5ee74`, with only `jackin.role.toml` modified. Before and after every export, the working-diff SHA256 was `9dc56c8d0bd4baea9d5c0387f6734caccad0178e76e1b0786216a6ab7988ae73`, the cached-diff SHA256 was `01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b`, and the index-listing SHA256 was `f362752300d6233a54a6e3c5606614cbdfb7db4d0b8709082a37a9ca21519ceb`. Export did not change the host tree, index, or HEAD.

## Cleanup and remaining records

All sandboxes and exported refs were deleted. The account, workspace, and role registration were removed. Exact provider cleanup covered both gateways without broad shared-account deletion. The explicit local provider delete reported `not found` because that instance was already absent. Final provider lists confirmed no `a14t13ml` provider on either gateway. Foreign resources were not changed.

The temporary sentinel and isolated configuration remain for independent verification. The worktree retains only this evidence, the blocker update, and sanitized `.audit` records. The T13 production enablement is restored. No full G-FAST, commit, or push occurred.
