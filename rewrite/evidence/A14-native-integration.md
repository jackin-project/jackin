# A14 native integration

Date: 2026-11-10. Base: `d9411a2d72a3710a28675072adef1bcc64c07093`. Worktree: `/tmp/jackin-a14-native-integrated`. Branch: `wt/a14-native-integrated`.

## Inputs and review

The integration first applied the complete 57-file `git diff HEAD --binary` from `/tmp/jackin-a14-oauth-integrated` at base `6d22dc1015340facc54fb9c13d4cb9a531996f37`. The captured snapshot had 2,689 insertions and 549 deletions. Its SHA256 was `7ba6e78e838f2f04068a12ba09f2d7bc0cba33393b8c8ef69b2616bd318dacf6`.

The integration then applied the complete 46-file `git diff HEAD --binary` from `/tmp/jackin-a14-native-implementation` at base `17aacafcfa08300200bea25a8d5562268efaaf04`. That snapshot had 3,934 insertions and 561 deletions. Its SHA256 was `42de71ac1ae483be8c1f3a4a7154795dd579cb1be6446d07b967255926925910`, matching the required value.

The existing verdicts are `REVIEW_PASS` in `/tmp/a14-oauth-independent-review.md` and `/tmp/a14-native-implementation-review.md`. The native verdict includes all six regional-closure and credential-collision controls. This integration claims no new independent review. A fresh integrated review follows this unit.

## Conflict resolution

The OAuth snapshot applied without unresolved conflicts. The native snapshot produced 31 conflict regions in these 18 files:

- `crates/jackin-core/src/agent.rs` and `crates/jackin-core/tests/agent_instances.rs`.
- `crates/jackin-session/src/accounts.rs`, `files.rs`, `launch/run.rs`, `lib.rs`, and `plan.rs`.
- `crates/jackin-session/tests/accounts/stale_launch.rs` and `sync.rs`.
- `crates/jackin-session/tests/launch/credentials.rs`, `multi_instance.rs`, `omp_models.rs`, and `provider.rs`.
- `crates/jackin-session/tests/plan/accounts.rs`, `codex.rs`, and `files.rs`.
- `crates/jackin-tui/src/launch.rs` and `crates/jackin/tests/cli/load.rs`.

The resolutions retain generic `ProviderMaterial::OAuth2RefreshToken`, exact per-instance static files, the exact T02 placeholder, Grok external-session config, and `GROK_SESSION_TOKEN`. They add both regional Kimi profiles, native eligibility, regional policy closure, and T08 `KIMI_API_KEY` collision detection. The shared placeholder parser rejects bare prefixes, raw values, non-UTF8 values, and multiline values. Account sync combines native metadata checks with exact-provider rollback.

The planner keeps generic static files for static tuples and separate `plan_kimi_files` output for native Kimi. T06, T07, and T08 have no static-file fallback. Native host logins cannot serve static tuples, and static sources cannot serve T06 through T08. Role bindings, A11b exact-provider cleanup, baseline APIs, corrected records, and T09 rendering remain intact.

Fixture reconciliation preserves the current head's availability. Tests use supported ZAI and multi-agent fixtures instead of unavailable T01 launches. The native eligibility positive control uses supported OMP/ZAI because OMP/Kimi remains unavailable. The provider rollback test uses Claude/ZAI plus Codex/ZAI to avoid the T09/T18 environment collision while preserving seeded OMP providers. The two native placeholder launch tests that required supported T01 were not retained. Existing T01 tests prove refusal before credential reads. A11b OmpHost ambient-isolation tests remain present.

No tuple state changed. T02 remains `BlockedVendor` with `vendor clarification pending`. T03 remains `Unavailable` with `not yet proven on a real gateway`. T06 remains unavailable with `no stable Linux artifact`. T07 remains unavailable with `native login has no refresh grant`. T08 remains unavailable with `native refresh and final live proof pending`. T09 remains supported with `https://api.z.ai/api/anthropic` and `anthropic-messages`.

## Verification

Formatting and `git diff HEAD --check` passed. Focused hermetic nextest runs used two test threads and `CARGO_NET_OFFLINE=true` through `mise exec -- mbx +1.99.1`.

- `jackin-core`: 188 passed, 0 skipped.
- `jackin-session --test accounts --test plan --test launch`: 349 passed, 0 skipped.
- `jackin-openshell --test gateway --test tuple_policy`: 107 passed, 0 skipped.
- `jackin-tui --test launch --test console`: 280 passed, 0 skipped.
- `jackin --test cli`: 180 passed, 0 skipped.

The focused runs passed 1,104 tests with zero skipped. A later focused run also passed all 17 native reader and subscription tests after the Display-format correction.

The required root gate was:

```sh
rtk env NEXTEST_TEST_THREADS=2 mise exec -- mbx +1.99.1 xtask ci --fast
```

It exited 0 in 1,327.084 seconds. Nextest ran 2,306 tests in 1,237.446 seconds, with 2,306 passed and 0 skipped. The run passed fmt, clippy, nextest, deny, shear, comments, layers, pins, containers, errors, baseline, screen, demo-gate, and reuse. REUSE reported 1,247 compliant files. The fast gate skipped the docs build as defined by the repository gate.

No live gateway, Docker, real credential, or live agent was used. No live support is claimed. No `.audit` file is included.

## Remaining live gates

T02 still needs vendor clarification, refresh and rotation, native placeholder compatibility, host safety, two-gateway isolation, replay, export, process-state, and billing evidence.

T03 still needs real synthetic and subscription `xtask e2e` proof of helper login, headless rerun, gateway refresh with a stable placeholder, effective `grok-4.7`, trusted supervised launch, bounded prompts, subscription attribution, host usability, and second-sandbox isolation.

T08 still needs mainland China and global live gateway proofs for refresh, regional policy, exact guest files, provider-present relaunch, and lead and secondary homes. T08 remains unavailable pending that proof.
