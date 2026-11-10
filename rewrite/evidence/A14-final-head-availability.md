# A14 final-head availability

Verified implementation head: `e275950ca3f5d82d17dfa49789f9582d3487088b`.

## Dispositions

| Tuple | State | Exact reason |
|---|---|---|
| T01 | Unavailable | `final live proof pending after host re-login` |
| T10 | Unavailable | `final live proof pending with Kimi plan key` |
| T11 | Unavailable | `final live proof pending with MiniMax plan key` |
| T12 | Unavailable | `final live proof pending with MiMo Token Plan key` |
| T16 | Unavailable | `final live proof pending with MiMo Token Plan key` |
| T20 | Unavailable | `final live proof pending with MiMo Token Plan key` |
| T21 | Unavailable | `final live proof pending with Anthropic API key` |

T09, T14, and T18 remain supported positive controls. T15 and T19 keep their existing exact-model proof-pending dispositions. T22 remains unavailable because no OpenAI API key exists in an approved source. Auth classes, profiles, and models are unchanged. T09 keeps its live-proven `https://api.z.ai/api/anthropic` base URL and `anthropic-messages` API from commit `436781af9`.

## Host proof

The earlier 2026-11-09 host check ran Codex CLI 0.167.0 and returned `token_expired` with repeated HTTP 401 refresh responses. The host `~/.codex/auth.json` stayed unchanged at mode 0600, size 4250 bytes, and mtime `2026-10-29 22:50:55` local time. This was the evidence for the initial availability correction, not a current claim that the login is still expired.

The operator subsequently completed interactive re-login. On 2026-11-10, `codex --version` reported `codex-cli 0.167.0` and `codex login status` reported `Logged in using ChatGPT`. No credential value was read or recorded. T01 remains unavailable until its complete final-head live proof.

Authorized 1Password access subsequently supplied the Kimi, MiniMax, MiMo, and Anthropic keys. They were exported to private host files with mode 0600 and registered through the supported account path on both jackin-docker and jackin-local. Only the sources and registration state are recorded. Existing provider inventory was not used as a credential-discovery source or as support evidence. No OpenAI API key was found.

## Old-head failures

The new checks ran before the six rows changed. Each failure showed the expected disagreement.

- All 18 planner refusal cases failed. Six lead `plan_launch` cases, six secondary `plan_launch` cases, and six secondary `plan_multi` cases planned successfully or reached a different error. The T09/T14/T18 control and the T18 `plan_launch` control passed.
- All six CLI cases failed. T01 launched successfully. T10, T11, T12, T16, and T20 reached account selection instead of the required tuple refusal.
- All six console offer cases reported `Supported`. The T01 real launch completed. T20 reached `unknown model gpt-6.1-sol` instead of the required availability refusal.
- The T01 catalog assertion reported `Supported` instead of `Unavailable`. The manifest accepted `gpt-6.1-sol` instead of returning `ManifestError::InvalidValue` with key `agents.codex.model`.

## Fixed-head passes

The six rows changed to `Unavailable` with the exact reasons above.

- Planner: 20 tests passed, including all 18 refusal cases and both positive controls.
- CLI refusals: 6 passed. The full CLI suite passed 172 tests with 0 skipped.
- Console offers and real T01/T20 refusals: 8 passed. The full console launch suite passed 50 tests with 0 skipped.
- Core: 176 tests passed with 0 skipped.
- Planner and multi-layout: 122 tests passed with 0 skipped.
- Launch: 110 tests passed with 0 skipped.
- Accounts: 75 tests passed with 0 skipped.
- xtask: 227 tests passed with 0 skipped.

Pure rendering remains independent of launch availability. The exported Codex renderer proves T01 trust-only configuration and T20 `web_search = "disabled"`. Generic launch and model cases use T18 with provider `zai` and model `glm-5.3`. Unknown models remain catalog errors before planning wraps them as `PlanError::TupleNotSupported`.

Account import, synchronization, renewal, removal, locking, and usage tests keep explicit ChatGPT fixtures. Generic launch, provider, stale-launch, account-selection, and executor fixtures use T18/Z.AI. `LaunchRequest::new` keeps its production Codex/OpenAI defaults.

The e2e setup registers a Z.AI account for supported launches. J03, J14, J15, and J17 pass explicit provider, model, and account flags. The A1 ChatGPT account remains for account lifecycle, expired-login negatives, and usage. No test was skipped or ignored.


## Mutation and restoration proof

The per-tuple mutation pass used the same public checks. Restoring each tuple to `Supported` made its planner, CLI, and console refusal tests fail. Replacing each exact reason with `incorrect mutation reason` made those tests fail again. The T01 `Supported` mutation also made the manifest accept `gpt-6.1-sol`. Wrong reason text did not change the manifest's correct refusal.

Removing the lead availability check made five lead cases fail. T01 still failed closed through the shared secondary validator. Removing the secondary check in `validate_instances` made all 12 secondary `plan_launch` and `plan_multi` cases fail. All product code was then restored. The restored availability matrix passed 20 tests, the core suite passed 176 tests, and xtask passed 227 tests, all with 0 skipped.

The local mutation log is `.audit/a14-availability-mutations.log`. Earlier zero-test and filter-error attempts are not counted as proof.

## T09 renderer correction proof

The availability branch started from a base that predated commit `436781af9`. The correction restores `omp_base_url` to `https://api.z.ai/api/anthropic` and `omp_model_api` to `anthropic-messages`. The planner and launch fixtures assert the same values. No availability disposition changed.

The focused check selected three planner tests and two launch tests. The fixed control passed all 5 tests in nextest run `4ddc3d53-cc20-4961-877c-beaf94ec0ac1`.

```sh
rtk mise exec -- mbx +1.99.1 nextest run -p jackin-session --test plan --test launch -E 'test(omp_config_sets_the_exact_plan_base_url_per_tuple) | test(omp_models_yaml_pins_one_provider_the_env_named_key_and_one_model_for_custom_tuples) | test(omp_config_and_auth_goldens_stay_unchanged_for_every_omp_tuple) | test(a_non_lead_omp_instance_gets_the_lead_models_file_and_a_home_copy) | test(an_omp_lead_launch_gets_the_lead_models_file_and_no_home_copy)'
```

Restoring only the retired `https://api.z.ai/api/coding/paas/v4` URL made all 5 tests fail in nextest run `d2e76dd7-9dbb-4823-b4a2-a11b3205aecd`. The planner assertions reported the retired URL on the left and the Anthropic URL on the right. Both launch assertions reported the same URL disagreement in `models.yml`.

With the correct URL restored, removing only the T09 `omp_model_api` mapping made 3 tests fail in nextest run `d6c8913b-a364-4111-970f-be662f8fffb0`. The planner YAML assertion reported `None` instead of `Some("anthropic-messages")`. Both launch assertions reported the missing `api: anthropic-messages` line. The two config-only tests passed because they do not render `models.yml`.

Restoring the T09 API mapping made all 5 tests pass in nextest run `314af0d0-4ceb-47b8-9819-e859ebaacd3e`. The worktree retains this fixed state.

## Direct-review account-selection restoration

Direct diff review found that `resolve_choices` replaced an explicitly named account with the default when the name was missing or belonged to another provider. The restoration removes that fallback. `Some(id)` again preserves exactly the parsed `AccountName`. Only `None` uses `config.select_account` and `default_account`. No new error or fallback was added.

The existing controls passed after the restoration. The console account-selection run passed 30 tests. The core account-selection run passed 2 tests, including unknown-account and wrong-provider refusals. The planner selection control passed 1 test. No test was added or changed for this restoration.

## Integrated baseline migration and final fast gate

The integrated implementation is commits `344472d4a3b78cc24cff00e0a96aac0e52599268` and `e275950ca3f5d82d17dfa49789f9582d3487088b`. The follow-up corrected the remaining picker fixtures, moved degraded power errors into typed domain-owned variants, and recorded the two baseline `windows-sys` duplicate survivors.

The final launch suite passed 57 tests, the console suite passed 219 tests, the core catalog control passed 9 tests, and the availability matrix passed 20 tests. The complete command was:

```sh
NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 xtask ci --fast
```

It exited 0. Nextest ran all 2,234 tests with 2,234 passed and 0 skipped. Fmt, Clippy, deny, shear, comments, layers, pins, containers, errors, baseline, screen, demo-gate, and reuse also passed. The exact log is `.audit/a14-api-migration-ci-fast-final.log`.

Independent integrated review and final-head live proofs remain separate gates. This fast gate does not promote any unavailable tuple.
