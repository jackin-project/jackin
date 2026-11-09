# A14 final-head availability

Final base SHA: `ebfb26c9c3961ceba28e0cab84396c5530648b20`.

## Dispositions

| Tuple | State | Exact reason |
|---|---|---|
| T01 | Unavailable | `host Codex login expired; interactive re-login required` |
| T10 | Unavailable | `no Kimi Code membership key on this host` |
| T11 | Unavailable | `no MiniMax coding plan key on this host` |
| T12 | Unavailable | `no MiMo Token Plan key on this host` |
| T16 | Unavailable | `no MiMo Token Plan key on this host` |
| T20 | Unavailable | `no MiMo Token Plan key on this host` |

T09, T14, and T18 remain supported positive controls. T15 and T19 keep their existing dispositions. Auth classes, profiles, and models are unchanged. T09 keeps its live-proven `https://api.z.ai/api/anthropic` base URL and `anthropic-messages` API from commit `436781af9`.

## Host proof

On 2026-11-09 at about 13:12Z, the actual host command was:

```sh
codex exec --skip-git-repo-check "Reply with exactly: HOST-PONG"
```

The host ran codex-cli 0.167.0. It returned `token_expired` and repeated HTTP 401 responses during token refresh. No `HOST-PONG` response arrived. Interactive re-login is required.

Before and after the run, `~/.codex/auth.json` had mode 0600, size 4250 bytes, and mtime `2026-10-29 22:50:55` local time. A repeated run at 16:37Z returned the same error class and left those metadata unchanged.

The credential search found no replacement keys. `op whoami` failed with `account is not signed in`. The host omp store has no Kimi Code membership key, MiniMax coding plan key, or MiMo Token Plan key. Existing gateway providers are not an approved discovery source and supplied no support evidence.

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



## Full fast gate

`NEXTEST_TEST_THREADS=1 mise exec -- mbx +1.99.1 xtask ci --fast` exited 0. Nextest ran all 2207 tests: 2207 passed and 0 skipped. The `deny`, `shear`, `comments`, `layers`, `pins`, `containers`, `errors`, `baseline`, `screen`, `demo-gate`, and `reuse` steps then passed. The full console suite also passed 201 tests with 0 skipped.
