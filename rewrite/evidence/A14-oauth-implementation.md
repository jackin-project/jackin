# A14 shared T02 and T03 OAuth implementation

Date: 2026-11-10. Base HEAD: `7bd1b7036fe771139b885243f89afe949f7510ae`. Worktree: `/tmp/jackin-a14-grok-session`. Branch: `wt/a14-grok-session`.

## Scope and status

This unit implements the shared OAuth refresh model, complete Claude and Grok host imports, account publication, exact T02 and T03 profiles, and the Grok session alias and trusted launch arguments. It does not change per-instance credential planning, `plan.rs`, `files.rs`, `launch/run.rs`, or `attach.rs`.

T02 remains `AuthClass::BlockedVendor` and `TupleState::BlockedVendor` with reason `vendor clarification pending`. T03 remains `TupleState::Unavailable` with reason `not yet proven on a real gateway`. No live support or live refresh is claimed.

The new tests have local fixed-green results. Independent old-code and wrong-direction reds and fixed greens remain pending. This unit does not claim independent test-retention approval.

## Approved plans

The approved reviewer outputs are preserved verbatim:

- `.audit/a14-oauth-t03-review.log`
- `.audit/a14-oauth-t03-final-approval.log`
- `.audit/a14-oauth-t02-approval.log`

No per-instance placeholder tests or other tests outside the approved scope were added.

## Implementation

`ProviderMaterial::OAuth2RefreshToken` in `crates/jackin-openshell/src/gateway.rs` holds the access token, refresh token, client ID, scopes, optional epoch-millisecond expiry, and additional provider credentials. ChatGPT publication converts its real account ID into `CODEX_AUTH_ACCOUNT_ID` through `additional_credentials`. Static records no longer need a dummy ChatGPT account ID.

The gRPC writer in `crates/jackin-openshell/src/grpc/provider.rs` places only the access token and explicit additional credentials in provider credentials. Refresh material contains `client_id`, secret `refresh_token`, and optional space-joined `scope`. The profile owns the token URL. Empty optional values produce no empty entries. Expiry converts to exact protobuf seconds and nanoseconds. Updates replace removed credentials and refresh state. OAuth-to-static conversion deletes refresh state, static-to-OAuth conversion recreates it, and provider deletion removes both records. Errors redact access and refresh tokens.

`crates/jackin-session/src/credentials.rs` validates complete grants before publication:

- Claude file and Keychain imports require nonblank access and refresh tokens, a positive integer epoch-millisecond expiry, and a nonempty ordered scope array with nonblank members. The public client ID is `9d1c250a-e61b-44d9-88ed-5944d1962f5e`.
- Grok imports require nonblank `key`, `refresh_token`, and `oidc_client_id`. The entry must have `oidc_issuer` equal to `https://auth.x.ai` and `auth_mode` equal to `session` or `oidc`. Multiple matching entries are ambiguous and fail. Errors and debug output do not contain token values.

`Session::sync_account` uses one publication helper for static and OAuth material. Invalid host grants publish no profile or provider. The shared test fake retains scopes, expiry, additional credentials, and secret-free debug output.

The T02 profile uses `https://platform.claude.com/v1/oauth/token`, required nonsecret `client_id`, required secret `refresh_token`, and a 300-second refresh margin. It retains only `api.anthropic.com` and `platform.claude.com` and injects only `CLAUDE_CODE_OAUTH_TOKEN`.

The T03 profile uses `https://auth.x.ai/oauth2/token` with the same material and refresh-margin shape. It injects only `GROK_SESSION_TOKEN` and allows only `cli-chat-proxy.grok.com` and `auth.x.ai`. The Grok spec launches `grok --trust`, retains model `grok-4.7`, and removes `XAI_API_KEY`, `x.ai`, and `api.x.ai` from runtime credential and host declarations. Neither profile sets `stable_placeholder` or profile-wide grant scopes.

## Verification

The baseline targeted command passed 193 tests. The same command after implementation passed 199 tests. Exact outputs are in `.audit/a14-oauth-baseline-nextest.log` and `.audit/a14-oauth-targeted-nextest.log`.

```sh
rtk mise exec -- mbx +1.99.1 nextest run -p jackin-core --test agent_table --test catalog -p jackin-session --test accounts -p jackin-openshell --test gateway --test tuple_policy
```

The new tests cover complete file and Keychain imports, malformed and incomplete grants, no-publication failures, ChatGPT account-ID preservation and updates, generic fake state, exact gRPC material and expiry, A-to-B updates, repeated convergence, OAuth/static conversions, deletion, retry redaction, exact profiles, launch arguments, and unchanged availability.

The full G-FAST command ran to the nextest stage. Rustfmt and clippy passed. Nextest stopped after 952 tests with 950 passes and two failures in `launch/omp_models.rs`. The failures expect `https://api.z.ai/api/coding/paas/v4`, while `HEAD` and the current T09 renderer produce `https://api.z.ai/api/anthropic`. Those tests and that renderer are outside this unit and unchanged. The exact output is in `.audit/a14-oauth-g-fast.log`.

```sh
rtk mise exec -- mbx +1.99.1 xtask ci --fast
```

Separate planner controls found three more assertions from the same pre-existing ZAI mismatch. The final planner run passed 94 tests with the `omp_models` group and `omp_config_sets_the_exact_plan_base_url_per_tuple` filtered out. The complete `jackin-start` suite passed 16 tests. Exact outputs are in `.audit/a14-oauth-planner-nextest.log` and `.audit/a14-oauth-image-nextest.log`.

The other CI steps passed. Their exact output is in `.audit/a14-oauth-remaining-gates.log`.

```sh
rtk mise exec -- mbx +1.99.1 xtask ci --only deny,shear,comments,layers,pins,containers,errors,baseline,screen,demo-gate,reuse
```

All test fixtures are synthetic. No real credential, live agent, container, or live gateway was accessed.

## Remaining work

- An independent reviewer must observe the approved old-code and wrong-direction reds and fixed greens before the new tests are retained under the repository protocol.
- The coordinated all-agent unit must consume the tuple and profile changes in per-instance planning and execution, including the excluded planner, launch, and attach files.
- The pre-existing ZAI URL expectation mismatch needs a separate owner. This unit does not alter OMP behavior.
- Live T02 and T03 refresh, rotation, placeholder stability, runtime compatibility, isolation, billing, and host-safety gates remain open. Availability must not change until those gates pass.
