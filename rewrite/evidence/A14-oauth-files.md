# A14 per-instance OAuth files

Date: 2026-11-10. Base HEAD: `17aacafcfa08300200bea25a8d5562268efaaf04`. Worktree: `/tmp/jackin-a14-oauth-files`. Branch: `wt/a14-oauth-files`.

## Scope and status

This unit applies the reviewed per-instance diff from `/tmp/jackin-a14-instance-files`, excluding `.audit`, and closes the independent OAuth review's blocking T02 and T03 findings. Every hunk applied cleanly. The older patch's T03 `XAI_API_KEY` mapping was the only semantic conflict. It now resolves to `GROK_SESSION_TOKEN` in `crates/jackin-core/src/agent.rs`.

T02 remains `AuthClass::BlockedVendor` and `TupleState::BlockedVendor` with reason `vendor clarification pending`. T03 remains `TupleState::Unavailable` with reason `not yet proven on a real gateway`. The `grok --trust` launch arguments and T09's `anthropic-messages` rendering remain unchanged. No live support is claimed.

## Implementation

The public `plan_static_files` seam in `crates/jackin-session/src/plan.rs` plans every static and credential file for one instance. `plan_launch` collects the files for the lead home and every secondary home. `crates/jackin-session/src/launch/run.rs` writes that complete set generically. The old agent-specific secondary config, OMP auth, OMP models, Claude config, and Muse trust write branches are gone.

T02 plans `{}` settings plus a native `.credentials.json` object at mode `0600` in lead and secondary Claude homes. Its exact object has `claudeAiOauth.accessToken` set to `openshell:resolve:env:CLAUDE_CODE_OAUTH_TOKEN`, an empty `refreshToken`, numeric `expiresAt` value `4102444800000`, and scopes `user:profile` and `user:inference`. It has no host refresh token, host expiry, API-key field, or extra client field. Static Claude tuples T13 through T16 retain their empty credential objects.

T03 plans only `.grok/config.toml` at mode `0600` in lead and secondary Grok homes. Generic `.grok/auth.json` planning and the public `grok_auth_file` helper are removed. The active TOML keeps `cli.auto_update = false` and the instance model. Its `[auth]` table sets `disable_api_key_auth = true` and this exact helper command:

```sh
jq -n -e --arg access_token "${GROK_SESSION_TOKEN-}" '$access_token | select(test("^openshell:resolve:[^\\r\\n]+$")) | {access_token: ., issuer: "https://auth.x.ai"}'
```

The helper emits exactly `access_token` and `issuer`. It emits no refresh token, expiry, API key, or client-driven token URL. Missing, empty, literal `placeholder`, raw, malformed, bare-prefix, and multiline values fail with empty stdout. Launch reuses the typed `PlaceholderMissing` and `PlaceholderInvalid` reader for native OAuth tuples before writing credential files. The reader also rejects the bare `openshell:resolve:` prefix.

## Reproducible red evidence

These runs are local reproducible evidence for later independent review. They are not an independent verdict. Every wrong-direction mutation was reverted immediately after its red run.

The incomplete implementation failed all four new T02 and T03 tests. T02 rendered generic `CLAUDE_CODE_OAUTH_TOKEN` JSON. Grok had no `[auth]` table and planned generic `auth.json` beside its config. The helper test failed because no helper existed. Logs are `/tmp/a14-oauth-files-old-red.log` and `/tmp/a14-oauth-files-old-red-all.log`.

```sh
rtk env NEXTEST_TEST_THREADS=2 CARGO_NET_OFFLINE=true mise exec -- mbx +1.99.1 nextest run --no-fail-fast -p jackin-session --test plan -E 'test(grok::) or test(t02_native_placeholder_is_exact_in_lead_and_secondary_homes)'
```

The wrong-direction runs produced these exact results:

- `/tmp/a14-oauth-files-red-helper-accepts-invalid.log`: removing helper validation emitted `access_token: ""` and exited zero for empty input.
- `/tmp/a14-oauth-files-red-api-key-enabled.log`: setting `disable_api_key_auth = false` produced left `Some(false)`, right `Some(true)`.
- `/tmp/a14-oauth-files-red-t02-refresh-leak.log`: inserting a synthetic refresh token changed the exact native object from `refreshToken: ""` to `refreshToken: "FAKE-HOST-REFRESH"`.
- `/tmp/a14-oauth-files-red-xai-alias.log`: restoring the older alias produced left `Some("XAI_API_KEY")`, right `Some("GROK_SESSION_TOKEN")`.
- `/tmp/a14-oauth-files-red-empty-resolve-prefix.log`: accepting the bare resolve prefix let the launch reach `Ready` instead of returning `PlaceholderInvalid`.

## Fixed green and static checks

The restored four-test run passed 4 of 4 tests in `/tmp/a14-oauth-files-final-four-green.log`. The helper-only restored run passed 1 of 1 in `/tmp/a14-oauth-files-green-helper-restored.log`. The bare-prefix restored run passed 1 of 1 in `/tmp/a14-oauth-files-green-empty-resolve-prefix.log`.

The focused hermetic control run passed 321 of 321 tests with nine filtered out in `/tmp/a14-oauth-files-focused-green.log`. The filter excludes only the previously documented ZAI URL mismatch in `omp_models` and `omp_config_sets_the_exact_plan_base_url_per_tuple`.

```sh
rtk env NEXTEST_TEST_THREADS=2 CARGO_NET_OFFLINE=true mise exec -- mbx +1.99.1 nextest run --no-fail-fast -p jackin-core --test agent_table --test catalog --test agent_instances -p jackin-session --test plan --test launch --test accounts -p jackin-openshell --test tuple_policy -E 'not test(omp_models) and not test(omp_config_sets_the_exact_plan_base_url_per_tuple)'
```

The new tests cover exact lead and secondary paths, modes, parsed file contents, no generic Grok auth file, no API-key fallback, unchanged availability, exact helper output, malformed helper input, and typed launch refusal. Existing static launch tests cover generic consumption for launchable tuples. The helper test executes `/bin/sh` and `jq` locally without a gateway or network.

These static checks passed:

```sh
rtk mise exec -- mbx +1.99.1 fmt --all -- --check
rtk mise exec -- mbx +1.99.1 clippy -p jackin-core -p jackin-session --all-targets -- -D warnings
rtk git diff HEAD --check
```

Clippy output is `/tmp/a14-oauth-files-clippy-final.log`. Full G-FAST was not run on this intermediate dependency stack. No commit or push was made. No live network, gateway, Docker, credential, or agent was used.

## Remaining gates

- An independent reviewer must reproduce the old-code and wrong-direction reds and fixed greens before retaining the new tests under the repository protocol.
- The coordinated integrated stack must run its final gate, including full G-FAST. Native T02 and T03 launch writes cannot be exercised while their availability states remain unchanged, so the approved public planner provides their hermetic lead and secondary coverage.
- T02 still needs live vendor clarification, refresh and rotation, native placeholder compatibility, host safety, two-gateway isolation, replay, export, process-state, and billing evidence.
- T03 still needs real synthetic and subscription `xtask e2e` proof of helper login, headless rerun, gateway refresh with a stable placeholder, effective `grok-4.7`, trusted supervised launch, bounded prompts, subscription attribution, host usability, and second-sandbox isolation.
- The known ZAI URL expectation mismatch remains outside this unit. Its tests were filtered rather than changed.
