# A14 native implementation

## Scope

The worktree is `/tmp/jackin-a14-native-implementation` on branch `wt/a14-native-implementation`, based on `17aacafcfa08300200bea25a8d5562268efaaf04`.

The tracked diffs from `/tmp/jackin-a14-instance-files` and `/tmp/jackin-a14-native-subscription` applied without textual conflicts. The integration removed the older static T07/T08 behavior, retained the generic `ProviderMaterial::OAuth2RefreshToken` path, and kept the approved native readers. No `.audit` file was changed. Dangling module declarations from excluded untracked test files were removed. The independently reviewed native reader tests were added back explicitly.

No commit or push was created. No full G-FAST, live network, live gateway, Docker, or credential operation was run. No new independent review or approval is claimed.

## Behavior

- `Account::check_credential_eligibility` accepts native host logins only for T02-T08. T06-T08 require native host logins and reject environment, file, 1Password, and OMP host sources. Native host logins cannot serve T09-T20. Existing static sources still serve T09-T20.
- Account edits validate every touched account before provider deletion, provider publication, or config commit. Direct sync validates in `sync_login`. Launch planning validates the lead and every secondary instance. Launch execution validates the current account and native metadata before every `ProviderState::Present` return.
- Kimi launch metadata is read and compared twice before the provider-state call. Provider publication compares it again before gateway effects. Accounts in different regions have independent metadata and providers.
- T03 uses `GROK_SESSION_TOKEN` and the existing Grok session OAuth grant. The T03 `XAI_API_KEY` override is gone.
- T06 remains unavailable with `AuthClass::OAuth` and the exact reason `no stable Linux artifact`. Valid native material returns `NativeRefreshUnsupported` with `AudienceUnsupported` before gateway effects.
- T07 remains unavailable with `AuthClass::OAuth` and the reason `native login has no refresh grant`. A valid `type = "api"` record returns `NativeRefreshUnsupported` with `NoRefreshGrant` before gateway effects. No Token Plan fallback exists.
- T08 remains unavailable with `AuthClass::OAuth` and the reason `native refresh and final live proof pending`. It was not marked Supported.
- T09 still uses `https://api.z.ai/api/anthropic` and `anthropic-messages`. Stale test expectations were corrected to the existing production behavior.

## Kimi regional grants and guest files

The mainland China profile is `kimi`. It uses `https://api.kimi.com/coding/v1` and `https://auth.kimi.com/api/oauth/token`. The global profile is `kimi-global`. It uses `https://api.kimi.ai/coding/v1` and `https://auth.kimi.ai/api/oauth/token`.

Both profiles use generic OAuth2 refresh material with client ID `17e5f671-d194-4dfb-9706-5516cb48c098`. Gateway expiry is the host `expires_at` converted from seconds to milliseconds with checked arithmetic. Zero host expiry is invalid and never reaches the gateway. Imported scope is never copied into gateway refresh material or either refresh profile. Refresh tokens stay in gateway material.

Guest config uses `default_model = "kimi-code/kimi-for-coding"` and only the `managed:kimi-code` provider. The provider has `type = "kimi"`, the regional base URL, an empty `api_key`, and file-backed OAuth storage. The mainland key is `oauth/kimi-code`. The global key is `oauth/kimi-code-env-0e4f99c69cc27850`. Only global config writes `oauth_host = "https://auth.kimi.ai"`. The model has `max_context_size = 262144` and the exact capabilities `thinking`, `always_thinking`, `image_in`, `video_in`, and `tool_use`.

Guest auth contains the validated `openshell:resolve:` access handle, an empty refresh token, preserved scope, `Bearer`, and numeric zero `expires_at` and `expires_in`. The mainland file is `credentials/kimi-code.json`. The global file is `credentials/kimi-code-env-0e4f99c69cc27850.json`. Both use mode `0600`. `plan_kimi_files` places the same files under `/sandbox/.kimi-code` for the lead and `/sandbox/.jackin/homes/kimi/.kimi-code` for the secondary.

Codex and Kimi share strict placeholder parsing. Empty, non-UTF8, multiline, repeated-line-ending, wrong-prefix, and raw secret-shaped outputs fail before auth files or upload markers are written. Kimi placeholders for all instances are validated before any Kimi config or auth file is written.

CLI and console native discovery return `HostLogin` only after the matching native reader succeeds. Failed Kimi, MiniMax, or MiMo discovery never becomes `CredentialSource::File`.

## Red and green evidence

The first native subscription run used the public session API before the native implementation. The command was:

```
mise exec -- mbx +1.99.1 nextest run -p jackin-session --test accounts native_subscription
```

It ran 6 tests with 1 pass and 5 failures. Static native sources were accepted, regional OAuth publication was missing, both typed native refresh refusals were missing, and an invalid provider-present source change was not rejected.

After implementation, the same 6 tests passed. The 11 independently reviewed native reader tests also passed.

Three wrong-direction mutations were reverted after focused red runs:

- Allowing static sources for T06-T08 made `t06_through_t08_accept_only_host_logins` fail. The fixed test passed.
- Copying imported scope into gateway material made `kimi_native_logins_publish_regional_oauth_without_importing_scope` fail. The fixed test passed.
- Copying the access handle into guest `refresh_token` made `kimi_auth_preserves_scope_but_contains_no_refresh_grant_or_host_expiry` fail. The fixed test passed.

The final hermetic runs produced these results:

| Command target | Result |
| --- | --- |
| `nextest run -p jackin-core` | 184 passed |
| `nextest run -p jackin-session --test accounts` | 92 passed |
| `nextest run -p jackin-session --test plan` | 104 passed |
| `nextest run -p jackin-session --test launch` | 110 passed |
| `nextest run -p jackin --test cli account_add::` | 8 passed |
| `nextest run -p jackin --test cli load::` | 8 passed |
| `nextest run -p jackin-openshell --test tuple_policy` | 6 passed |
| `nextest run -p jackin-tui --test launch` | 41 passed |
| `nextest run -p jackin-tui --test console sources::` | 19 passed |

Every test command ran through `mise exec -- mbx +1.99.1 nextest run` from the worktree root. No `cargo test` command was used.

## Static checks

These checks passed from the worktree root:

- `mise exec -- mbx +1.99.1 fmt --all -- --check`
- `mise exec -- mbx +1.99.1 clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `git diff --check`
- `mise exec -- reuse lint`

## Review follow-up

The independent review at `/tmp/a14-native-implementation-review.md` found two omissions. New public regression tests reproduced both before any fix:

- `credential_environment_collisions_are_rejected_with_tuple_identity` accepted T08+T10. The new cases also cover T08+T17 with exact `KIMI_API_KEY`, `T08`, and second-tuple identity. The command exited 100 with `expected a credential collision`.
- `t08_global_role_rules_matching_single_instance_provider_hosts_are_rejected` accepted a role rule for `api.kimi.ai`. The command exited 100 with `Ok(ClosedPolicy(...))`.
- `t08_global_role_rules_matching_multi_instance_provider_hosts_are_rejected` accepted the same global API overlap. The command exited 100 with `Ok(ClosedPolicy(...))`.

The policy tests also cover `auth.kimi.ai` in both closure paths. `closure_profiles` now parses both T08 regional profiles for single-tuple and multi-tuple closure. T08 now exposes `KIMI_API_KEY` through `AgentTuple::credential_env`, so the existing public constructor rejects both collisions.

The identical three commands passed after the fixes. No availability flip, private launch construction, or live operation was used.

The follow-up suites passed with 184 core tests, 137 OpenShell tests, 633 session tests, and 8 CLI load tests. Formatting, workspace Clippy with `-D warnings`, `git diff HEAD --check`, and REUSE also passed. All 1239 files remain license-compliant.

## Remaining gates

The base branch still needs the integrated full G-FAST run and independent review. This intermediate dependency stack did not receive either gate.

CN and global launches still need final live gateway proofs for refresh, regional policy, exact guest files, provider-present relaunch, and lead and secondary homes. T08 remains unavailable pending that proof. No actual T08 launch or live refresh was performed here, so this document makes no Supported or live claim.
