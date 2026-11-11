# A14 Claude per-instance model settings

Implementation base: `4a1538771fe0cbd05f6511a7b05bfc85764684af`.

Branch: `wt/a14-claude-model`.

## Fix

Claude settings now carry the selected model for each instance. `render_claude_config` validates an explicit model with `AgentTuple::check_model`. `None` uses the tuple default. `claude_config_file`, `agent_config_file`, and the non-lead launch writer pass the same model through the existing per-instance configuration path.

The renderer emits only the top-level `model` and the existing tuple base URL. T02 has no `env` block. Settings contain no authentication entry, placeholder, model environment selector, or launch flag. Credential handling and tuple availability are unchanged.

The exact parsed settings are:

| Tuple | Settings |
|---|---|
| T02 | `{"model":"haiku"}` |
| T13 | `{"model":"kimi-for-coding","env":{"ANTHROPIC_BASE_URL":"https://api.kimi.ai/coding/"}}` |
| T14 | `{"model":"glm-5.3","env":{"ANTHROPIC_BASE_URL":"https://api.z.ai/api/anthropic"}}` |
| T15 | `{"model":"MiniMax-M3","env":{"ANTHROPIC_BASE_URL":"https://api.minimax.io/anthropic"}}` |
| T16 | `{"model":"mimo-v2.6-pro","env":{"ANTHROPIC_BASE_URL":"https://token-plan-sgp.xiaomimimo.com/anthropic"}}` |

## Approved tests

The four approved behavior groups have these public checks.

1. `claude_config_selects_the_exact_tuple_model_without_auth` checks explicit models, `None` defaults, exact JSON, the settings path, and mode `0600`. `t02_subscription_settings_carry_no_base_url_and_no_auth_env` covers T02. `claude_compatible_tuples_agree_on_auth_token_profile_and_settings` retains the endpoint and credential-profile controls. The assertions reject `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`, and `placeholder` in settings.
2. `claude_config_rejects_unknown_and_cross_tuple_models` checks `no-such-model` for every Claude tuple, `kimi-for-coding` on T14, and lowercase `minimax-m3` on T15. Each direct renderer error retains the original model, agent, and provider in `CatalogError::UnknownModel`. The same test checks that `plan_launch` returns `PlanError::TupleNotSupported` with `unknown model no-such-model`.
3. `t14_plan_launch_propagates_the_lead_model_to_claude_settings` checks the complete T14 settings document and mode `0600` in the public launch plan.
4. `t18_lead_and_t14_non_lead_write_the_claude_instance_model` runs a fake-gateway T18 Codex and T14 Claude launch. It checks the complete settings document and mode `0600` at `/sandbox/.jackin/homes/claude/.claude/settings.json`.

No test uses a live gateway or network.

## Old-code red evidence

The initial tests used the old public signatures so the old behavior could compile and run. The planner command was:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 nextest run -p jackin-session --test plan --locked --no-fail-fast --retries 0 -E 'test(/claude/)'
```

Nextest run `c527e027-d77c-4bf5-bdbf-c1c97d22e697` ran 8 tests with 4 passed and 4 failed. The exact-rendering, credential-profile, T02, and T14 planner checks failed. The left-hand settings lacked the required model. The log is `.audit/a14-claude-model/old-code-plan-red.log`.

The non-lead command selected only `t18_lead_and_t14_non_lead_write_the_claude_instance_model`. Nextest run `8524d1d1-2fde-4817-ae19-cce9eb903361` ran 1 test with 1 failure. The launch wrote only `ANTHROPIC_BASE_URL`, while the assertion required `glm-5.3` and the same base URL. The log is `.audit/a14-claude-model/old-code-non-lead-red.log`.

An earlier invocation incorrectly inserted `cargo` between `mbx +1.99.1` and `nextest`. It ran no tests and is not counted as evidence.

## Mutant evidence

Every mutant was applied to the fixed tree, run with `--locked --no-fail-fast --retries 0`, and then reverted. The logs are under `.audit/a14-claude-model/`.

| Mutant | Nextest run | Result | Discriminating assertion |
|---|---|---|---|
| Base-URL-only settings | `b8831ecc-1144-499d-b656-296643a1b57b` | 3 run, 3 failed | Exact renderer, T14 planner, and T18+T14 launch checks all lacked `model`. Log: `mutant-1-base-url-only-red.log`. |
| Hardcoded `kimi-for-coding` | `9808191d-75da-4192-a8ae-bd54d44275f9` | 2 run, 2 failed | T02 required `haiku`. The per-tuple check reached T14 and required `glm-5.3`. Log: `mutant-2-hardcoded-kimi-red.log`. |
| Dropped renderer validation | `eeb972c6-6873-4508-999e-04247b4b8287` | 1 run, 1 failed | `no-such-model` serialized instead of returning `CatalogError::UnknownModel`. Log: `mutant-3-dropped-validation-red.log`. |
| Altered endpoint | `0951665e-b444-4121-8888-f83405b2b005` | 1 run, 1 failed | T13 rendered `https://altered.invalid` instead of `https://api.kimi.ai/coding/`. Log: `mutant-4-altered-endpoint-red.log`. |
| Discarded planner model | `aa325f3a-ad0b-4a65-b382-1f3c8c413d51` | 2 run, 1 passed, 1 failed | The direct renderer control passed. The T14 planner wrote base-URL-only settings. Log: `mutant-5-discarded-planner-model-red.log`. |
| Discarded non-lead model | `c8b996ac-8180-4c6b-84eb-43f57be616a4` | 1 run, 1 failed | The T18+T14 launch wrote base-URL-only settings in the Claude home. Log: `mutant-6-discarded-non-lead-model-red.log`. |
| Lowercase rendered models | `1e5fb85b-ee9b-4b1b-b5aa-647579cc6ffe` | 1 run, 1 failed | T15 rendered `minimax-m3` instead of `MiniMax-M3`. Log: `mutant-7-lowercase-model-red.log`. |

For the two propagation mutants, passing `None` would not discriminate the fix. T14's only supported model is also its default. The temporary planner and launch mutations therefore reproduced the actual old behavior by writing a base-URL-only document only at those layers. The renderer stayed fixed.

The first hardcoded-model and non-lead mutation attempts failed compilation because of denied lint warnings. Those attempts are not counted. The table records the later behavioral runs.

## Fixed green evidence

The six focused checks passed in nextest run `838940f4-d60a-481b-af29-c8a036bb98af`. The run covered exact settings, validation, T14 planning, and T18+T14 non-lead execution. Its summary was 6 tests run, 6 passed, and 231 skipped. The log is `.audit/a14-claude-model/fixed-focused-green.log`.

The complete planner and launch integration suites used this command:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 nextest run -p jackin-session --test plan --test launch --locked --no-fail-fast --retries 0
```

Nextest run `6db301ea-25d1-4266-a044-fe8059f665ad` ran 237 tests with 237 passed and 0 skipped. The log is `.audit/a14-claude-model/focused-suites-green.log`.

After the final T02 path assertion was added, the six focused checks passed again in nextest run `58e6d2f9-0e92-493a-865a-37dc09a4437b`. The log is `.audit/a14-claude-model/final-focused-green.log`.

## Full fast gate

The required command is:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 xtask ci --fast
```

The gate exited 0. Nextest run `94fede5d-f1f3-4be7-95b0-14a78b263cd9` ran 2,254 tests with 2,254 passed and 0 skipped. Fmt, Clippy, deny, shear, comments, layers, pins, containers, errors, baseline, screen, demo-gate, and reuse also passed. The log is `.audit/a14-claude-model/ci-fast-green.log`.

An earlier gate stopped in Clippy because the validation test used `expect_err`. The final test asserts the error variant directly. That log is `.audit/a14-claude-model/ci-fast-clippy-red.log`. A later run reached 2,064 passing tests before the tool's 20-minute task limit stopped it. No project check failed in that run. Its log is `.audit/a14-claude-model/ci-fast-timeout.log`. The completed run above is the gate result.

## Remaining fresh T13 supervised proof

The hermetic checks do not promote T13. A fresh supervised proof remains required on both `jackin-docker` and `jackin-local` with the fixed binary and a unique launch on each gateway.

Each gateway proof must satisfy all of these conditions:

- Render exactly the T13 model and base URL in settings, with no authentication entry.
- Deliver `ANTHROPIC_AUTH_TOKEN` without `ANTHROPIC_API_KEY`. Do not expose the token value.
- Launch interactive Claude Code 2.1.311 without manual `/model`, manual settings, `ANTHROPIC_MODEL`, or a print-mode substitute.
- Obtain exact `T13-PTY-OK` after the real `ls -A /sandbox/project` tool call.
- Capture `/status` showing exact `kimi-for-coding`. An interactive banner with the exact model is acceptable only where `/status` cannot be captured reliably. `Default`, `claude-opus-5-5`, and `Opus 5.5` are failures.
- Show fresh-launch and final supervisor state `running 1`, with no restart and no `running 2`.
- Keep Claude as a direct child of `jackin-start` through detach and reattach, replay, and shell entry and exit.
- Retain the provider-only egress, secret-absence, plan-usage and no-paid-fallback, cached reload, export, and guarded-purge checks.

Missing exact-model evidence, an exact reply, or `running 1` is a failure. No live proof ran in this worktree. T13 through T16 retain their existing availability until each tuple completes its own required live proof.

A different independent reviewer must inspect the final diff and the red and green logs before the live proof. That review is pending.
