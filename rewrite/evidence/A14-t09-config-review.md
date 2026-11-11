# A14 T09 config review

## Verdict

PASS. The T09 correction is properly scoped, the configuration contracts are exact, and the red, green, and full-gate checks passed.

## Scope

Commit `78ab4be376d5cd717aa471079e4748019f24a03a` contains the corrected test expectations. The reviewed product correction is commit `fb0d396d6ebc57f8929c7f69c8066869d29add3c`. Its diff changes only `crates/jackin-core/src/agent.rs`.

The product diff makes two T09 changes. It sets `omp_base_url()` to `https://api.z.ai/api/anthropic` and adds `omp_model_api()` as `anthropic-messages`. It does not change T10, T11, T12, T21, or T22. The table-driven renderer and golden tests cover those tuples.

The [Z.AI model guide](https://docs.z.ai/devpack/latest-model) assigns `https://api.z.ai/api/anthropic` to Anthropic-compatible tools. It assigns `https://api.z.ai/api/coding/paas/v4` to OpenAI-compatible tools.

## Configuration contracts

`render_omp_models` produces one `zai` provider with this exact content and no model entry:

```yaml
providers:
  zai:
    baseUrl: https://api.z.ai/api/anthropic
    api: anthropic-messages
    apiKey: ZAI_API_KEY
```

`render_omp_config` produces this exact content with one placeholder:

```toml
providers:
  zai:
    baseUrl: https://api.z.ai/api/anthropic
    apiKey: placeholder
credentialEnv: ZAI_API_KEY
modelRoles:
  default: zai/glm-5.3
```

`omp_auth_file` remains unchanged. It writes `/sandbox/.omp/auth.json` with mode `0600` and this exact content:

```json
{"zai": {"type": "api_key", "key": "placeholder"}}
```

## Red and green proof

I temporarily restored the old T09 base URL and removed only the T09 `omp_model_api()` arm. I ran this command from the repository root:

```text
mise exec -- mbx +1.99.1 nextest run -p jackin-session --test plan --test launch omp --no-fail-fast
```

The old values produced 18 passes and these five failures in 23 tests:

- `plan omp::omp_config_sets_the_exact_plan_base_url_per_tuple`
- `plan omp_models::omp_config_and_auth_goldens_stay_unchanged_for_every_omp_tuple`
- `plan omp_models::omp_models_yaml_pins_one_provider_the_env_named_key_and_one_model_for_custom_tuples`
- `launch omp_models::an_omp_lead_launch_gets_the_lead_models_file_and_no_home_copy`
- `launch omp_models::a_non_lead_omp_instance_gets_the_lead_models_file_and_a_home_copy`

The assertion diffs showed the old base URL and the missing `api: anthropic-messages` line. The other 18 tests passed.

I restored the two T09 corrections and reran the same command. All 23 tests passed, with 185 tests skipped by the filter.

## G-FAST

I ran the full gate from the repository root with no timeout:

```text
mise exec -- mbx +1.99.1 xtask ci --fast
```

The first run stopped after 1,380 passes. `jackin-testkit::identity manager::jackin_account_form_shows_a_pending_check_until_the_provider_answers` returned `cannot execute binary file` after earlier tests in the same binary had passed. Nextest did not run the remaining 787 tests, so this was not a complete gate result.

An isolated rerun of that identity test passed. The full gate rerun exited 0. Nextest ran 2,168 tests with 2,168 passes and no skips. The `deny`, `shear`, `comments`, `layers`, `pins`, `containers`, `errors`, `baseline`, `screen`, `demo-gate`, and `reuse` steps also passed.
