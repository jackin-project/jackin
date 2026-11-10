# A15 supported-target E2E unit

Date: 2026-11-10. Base commit: `655a00e2f128093ca970e804c01850e8bfd25a30`. Jackin worktree: `/tmp/jackin-a15-supported-targets`, branch `wt/a15-supported-targets`. Sentinel worktree: `/tmp/jackin-a15-supported-targets-sentinel`, branch `wt/a15-supported-targets`. Sentinel base commit: `99ef6585d1c93b06bd5294c2188f75178f6af37a`.

## Implementation

- T01 remains the default. Agent-only `omp` selects T09, `claude` with provider `zai` selects T14, and provider-only `zai` selects T18. Every other supported catalog tuple without an adapter returns `AdapterUnavailable`. Invalid selections, models, credential classes, sources, provider hosts, and incomplete adapters fail before credential discovery or production effects.
- The adapter registry keeps immutable target identity and canonical SHA256 behavior specifications. T09 uses `https://api.z.ai/api/anthropic` with `anthropic-messages`. T14 uses the same Anthropic-compatible endpoint with `ANTHROPIC_AUTH_TOKEN`, and its exact prompts name `glm-5.3`. T18 uses `https://api.z.ai/api/v1`, requires an absent Codex auth file, and passes `--sandbox danger-full-access -c check_for_update_on_startup=false` to Codex exec. T01 uses those same Codex exec flags and remains the default target. T09, T14, and T18 use D29 `static_key` billing classification, never `api_key`.
- Credential sessions accept environment, 1Password, regular files up to 1 MiB, and the omp host store only for T09. Only non-T01 implicit sources can fall back to discovery; T01 preserves its original missing-environment error and never calls `op`. Discovery requires successful `op whoami` and exactly one item titled `Z.AI`, then reads its `credential` field. Failed `whoami`, absent items, and ambiguous items are setup errors with no gateway effect. Alternate source names stay outside private specs. Static-key setup uses synthetic environment descriptors for A1, FXR, and FXW, with independent invalid fixture keys. Non-T01 runs use no Codex client ID, OAuth refresh lock, Codex fixture login, or fresh host reply.
- Probe schema 3 requires actual provider and model metadata and a successful real tool invocation in `/sandbox/project`. The runtime credential SHA256 is outside the canonical target digest: the probe rejects the raw host key in standalone J08 and J09, while a distinct brokered token remains executable. J08 validates generic malformed JSON and auth-field classifications, nullable absent-file mode, exact configuration, declared brokered delivery, forbidden cross-lane variables, and reflection through the selected agent with an independent nonsecret placeholder. Z.AI provider checks require policy refusal because current evidence has no invalid-key response to declare.
- J11 and J12 require exactly one agent child whose `comm` matches the selected adapter before comparing process identity and terminals. Z.AI usage requires five-hour and weekly windows, permits monthly, and rejects credits, spend, paid fallbacks, undeclared windows, and fixture windows. Every account usage record and sandbox provider must name `zai`; FXR and FXW have independent wrong-reason and wrong-provider controls.
- Scenario constructs target-specific reports and external state. T01, T09, T14, and T18 each complete J00 through J17 and CLEAN with 19 PASS lines, zero FAIL, and zero SKIP. T01 reports `a1_host_login=unchanged`; the other targets report `not-applicable`.

The changes leave all catalog tuple states, `DESIGN.md`, `PLAN.md`, `decisions.tsv`, generated workflows, and dependency lockfiles unchanged. No live gateway, sandbox, container, or provider request ran.

## Red/green evidence

The independent reviewer still must inspect and mutate these results. This unit does not claim independent approval.

The exact old base rejected T09 before production:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 run -p jackin-xtask -- e2e --gateway jackin-local --role /tmp/jackin-a15-supported-targets-sentinel --agent omp --provider zai --credential env:ZAI_API_KEY
```

Result in detached worktree `/tmp/jackin-a15-old-code-red` at `655a00e2f`: exit 2, `xtask: tuple T09 has no complete E2E adapter`.

The fixed supported-target suite passed:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 nextest run -p jackin-xtask --test e2e --no-fail-fast j_supported::
```

Result: 13 tests run, 13 passed, 0 failed, nextest run `c3ddbeda-59b0-4478-b72b-2dfd2fed16b2`.

Focused mutants produced these behavioral reds and fixed greens:

- Ignoring tool success and exit status made T09's answer-without-tool case incorrectly pass J07. Restoring the checks passed `j_supported::credentialed_exec_proves_identity_answer_and_real_tool_use`, one test, zero failures.
- Accepting arbitrary provider HTTP responses made an undeclared T09 HTTP 401 incorrectly pass J09. Restoring exact response handling passed `j_supported::j09_provider_closure_and_response_classification_are_exact`, one test, zero failures.
- Returning the complete parent environment from `CredentialSession::selected_host_environment` exposed a sibling `KIMI_API_KEY`. Restoring the one-variable projection passed `environment_session_exposes_only_the_selected_secret_at_the_host_boundary`, one test, zero failures.

Commands and run IDs are in `.audit/a15-supported-targets.tsv`. Compile-only mutant attempts are not counted as behavioral evidence.

## Gates

Full hermetic E2E binary:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 nextest run -p jackin-xtask --test e2e --no-fail-fast
```

Result: 119 tests run, 119 passed, 0 skipped in 286.625 seconds, nextest run `1fdd33da-eb5a-4df9-9f83-b073cc8452cd`. The final supported-target subset also passed all 13 tests with nextest run `c3ddbeda-59b0-4478-b72b-2dfd2fed16b2`.

Sentinel probe suites:

```sh
rtk mise exec -- sh probe/tests/j08.sh
rtk mise exec -- sh probe/tests/egress.sh
```

Results: 285 J08 cases with zero failures and 165 egress cases with zero failures. The new cases prove brokered execution for every Z.AI target, raw-key rejection in standalone J08 and J09, malformed T09 and T18 auth handling, and policy-only provider closure.

Complete sentinel gate in `/tmp/jackin-a15-supported-targets-sentinel`:

```sh
rtk mise run test
```

Result: exit 0 in 832.695 seconds. Gitleaks, staged gitleaks cases, filter syntax, shellcheck, shfmt, hadolint, J08, setup hooks, and egress passed.

Role check in the same worktree, with jackin built by `rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 build -p jackin`:

```sh
rtk /tmp/jackin-a15-supported-targets/target/debug/jackin role check .
```

Result: exit 0 in 0.405 seconds.

Full jackin fast gate from the repository root:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 xtask ci --fast
```

Result: exit 0 in 1,847.694 seconds. Nextest ran 2,215 tests in 1,599.025 seconds, with 2,215 passed and 0 skipped. Fmt, clippy, deny, shear, comments, layers, pins, containers, errors, baseline, screen, demo-gate, and reuse also passed. The fast gate skipped the docs build as defined by the repository gate.

`git diff --check` passed. Local mutation scripts, logs, and `.audit` trails are not staged.

## Independent and live gates that remain

A different independent reviewer must witness the retained tests fail on broken code and pass on fixed code. The reviewer must also check the discriminating mutant matrix, including T18 versus T01 behavior, exact T14 and T18 prompts, incomplete adapters, pre-setup ordering, non-T01-only discovery, raw-host-key versus brokered-token handling in J08 and J09, malformed auth, T14 delivery, independent `other` classifications, fixture independence, wrong `comm` and duplicate children in J11 and J12, process ancestry, actual provider and model, real tool execution, absent T18 auth, provider closure, independent FXR and FXW reason and provider faults, Z.AI usage, no-skip handling, target selectors, and digest isolation.

After that review, default T01 and explicitly selected T09, T14, and T18 each need a real-gateway proof on the committed jackin and sentinel heads. Each record must include the exact head, tuple, adapter ID, canonical digest, all 19 PASS lines, CLEAN PASS, zero FAIL and SKIP, secret-free output, actual provider and model, successful real tool execution, expected usage and auth billing, and target-specific credential delivery. T01 also needs distinct fresh host replies and `a1_host_login=unchanged`.

The Z.AI provider probes require policy refusal until a separate live capture supplies an exact invalid-key status and closed body classification. Missing credentials, incomplete adapter evidence, provider defects, and unsupported usage remain FAIL or setup errors, never accepted SKIP results.
