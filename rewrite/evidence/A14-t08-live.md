# A14 T08 native Kimi Code subscription live proof

Date: 2026-11-10. Base: `bafb12ec0c157df12ca0d497b0b22620a6e3c074`. Worktree: `/tmp/jackin-a14-t08-live`. Branch: `wt/a14-t08-live`.

## Verdict

T08 is **FAIL**. The actual configured region is global. Native OAuth import and gateway refresh passed on both gateways, but Kimi Code 2.5.2 rejected the rendered model configuration before inference on both gateways. No guest reply, successful guest tool call, post-refresh reply, or nonzero guest membership usage was obtained. The temporary `Supported` enablement was restored. T08 remains unavailable.

The independent image, process, containment, regional-policy, terminal, export, and purge checks passed. Those checks do not repair the failed inference gate.

## Proof vehicle and isolation

The proof binary was `/tmp/jackin-a14-t08-live/target/debug/jackin`. The only tracked product-code change during the live runs was T08 `TupleState::Unavailable` to `Supported` and its reason to `None` in `crates/jackin-core/src/agent.rs`. `mise exec -- mbx +1.99.1 build -p jackin` exited 0 in 89 seconds. No full G-FAST ran, as ordered.

OpenShell reported 0.1.7. `jackin doctor` passed binary, registration, health, pinned-version, and engine checks on `jackin-docker` and `jackin-local`. Docker BuildKit built images only through `jackin-image`. OpenShell ran every sandbox and guest command. No Docker, Podman, Apple container, or host-agent fallback ran a sandbox or agent.

The proof used `XDG_CONFIG_HOME=/tmp/a14t08-xdg`. Its OpenShell registry path referenced the existing gateway registrations. The host Jackin configuration was not edited. Its SHA-256 was `5f5b215bf27638cabb0a9e0d33942ad45117f9c791c4144d75d402115348e1c3` before and after the proof.

The temporary sentinel clone was `/tmp/jackin-a14-t08-live-sentinel` at `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its role name was `a14t08-sentinel`. It added only the Kimi instruction mapping with model `kimi-for-coding`. `jackin role check` exited 0. The original sentinel checkout stayed clean at the same commit.

No membership API key, 1Password item, static Kimi file, environment key, or existing gateway provider supplied the T08 credential. Both accounts used `--from-host-login`. The profile credential name `KIMI_API_KEY` is the OpenShell broker key name, not a membership-key source.

## Actual region and other-region disposition

The approved `read_kimi_host_login` path selected `kimi-global` on `jackin-docker` first. Provider `jackin-a14t08-kd-kimi` had one credential key and refresh strategy `oauth2_refresh_token`. The same native import on `jackin-local` selected `kimi-global` for `jackin-a14t08-kl-kimi`. This proves global region selection through the approved reader without printing a credential.

The selected global grant file `/Users/donbeave/.kimi-code/credentials/kimi-code-env-0e4f99c69cc27850.json` passed an existence-only check. The mainland grant file `/Users/donbeave/.kimi-code/credentials/kimi-code.json` returned exit 1 from `test -f`. No mainland native grant exists at the required path. Mainland China was not exercised, and no grant or configuration was invented for it.

## Global provider and refresh

The docker account was `a14t08-kd`. The local account was `a14t08-kl`. Each provider used the generic `OAuth2RefreshToken` path with the regional global profile and one brokered access credential.

Refresh evidence from the final status checks was:

| Gateway | Status | Expires at | Next refresh | Last refresh |
|---|---|---|---|---|
| `jackin-docker` | `refreshed` | `2026-11-10 09:11:45` | `2026-11-10 09:06:45` | `2026-11-10 08:56:45` |
| `jackin-local` | `refreshed` | `2026-11-10 09:08:24` | `2026-11-10 09:03:24` | `2026-11-10 08:53:24` |

The docker status had earlier advanced from `configured` to `refreshed` at `2026-11-10 08:45:46`. Both refresh records are live gateway observations. Post-refresh guest prompts still failed at model resolution before inference. Host Kimi answered `HOST-T08-AFTER` after the runs, so the refresh and sandbox lifecycle did not make the host login unusable.

## Image and provider-present relaunch

The docker sandbox was `jackin-a14t08-rff6` with launch ID `a14t08kimid01`. The local sandbox was `jackin-a14t08-09nw` with launch ID `a14t08kimil01`. Both used derived image `5392e31b70c40750f16ce881a04b122a202783aa5fe66f85c23a07ad9d5fe781` and Kimi Code 2.5.2 with the catalog SHA-256 `ac941c80c8b2b34451432323397a8e8c2e376948bc6d878fc7d2630be235b678`.

The initial docker launch printed `image derived: built`. Its same-launch relaunch printed:

```text
image construct: cached
image derived: cached
image: skipped, the image is cached
provider: skipped, the provider exists
sandbox: skipped, the sandbox exists
upload: skipped, the project is uploaded
```

The local launch used the cached derived image. Its same-launch relaunch reported the same cached, provider-present, existing-sandbox, and uploaded-project results.

## Process and exact guest files

Both sandboxes reported `uid=1000(sandbox) gid=1000(sandbox)` and no `sudo`. On docker, `kimi-code` PID 363 was the direct child of `/bin/sh /usr/local/bin/jackin-start kimi` PID 64. On local, `kimi-code` PID 685 was the direct child of `jackin-start` PID 371. Both supervisors reported `Running` with attempt 1.

The docker and local config files were byte-identical, mode `0600`, owner `sandbox`, and size 478 bytes:

```toml
default_model = "kimi-code/kimi-for-coding"

[providers."managed:kimi-code"]
type = "kimi"
base_url = "https://api.kimi.ai/coding/v1"
api_key = ""

[providers."managed:kimi-code".oauth]
storage = "file"
key = "oauth/kimi-code-env-0e4f99c69cc27850"
oauth_host = "https://auth.kimi.ai"

[models."kimi-for-coding"]
provider = "managed:kimi-code"
model = "kimi-for-coding"
max_context_size = 262144
capabilities = ["thinking", "always_thinking", "image_in", "video_in", "tool_use"]
```

The auth file was `/sandbox/.kimi-code/credentials/kimi-code-env-0e4f99c69cc27850.json`, mode `0600`, owner `sandbox`, and size 210 bytes on each gateway. Its exact stable contract was six fields. `access_token` held a distinct `openshell:resolve:` handle. `refresh_token` was the empty string. `expires_at` and `expires_in` were numeric zero. `scope` was `kimi-code`. `token_type` was `Bearer`. No refresh grant or host expiry entered either guest.

`kimi doctor config` accepted this file. `kimi provider list` reported `managed:kimi-code type=kimi models=1 source=oauth` and default model `kimi-code/kimi-for-coding`. Runtime prompt behavior did not accept that same configuration.

## Model-resolution failure on both gateways

The fixed docker request was `kimi -p "Reply with exactly: PONG" --output-format stream-json` from `/sandbox/project`. It exited 1 before inference:

```text
error: failed to run prompt: Model "kimi-code/kimi-for-coding" is not configured in config.toml.
```

The local fixed request produced the same error and exit 1. Two docker-only diagnostics preserved the files and established that explicit selection did not repair the contract:

```text
--model kimi-for-coding
error: failed to run prompt: model kimi-code/kimi-for-coding does not resolve to a configured provider

--model managed:kimi-code/kimi-for-coding
error: failed to run prompt: Model "managed:kimi-code/kimi-for-coding" is not configured in config.toml.
```

No `PONG` or `kimi-for-coding` response occurred. A real tool request could not run because every model request failed before inference. No successful tool result exists. The post-refresh reply gate fails with the same exact model-resolution error.

The supervised agent also displayed `Trust this folder?` for `/sandbox/project` during replay. The direct-child and persistence checks pass, but no successful supervised task is claimed.

## Membership usage and paid fallback

`jackin usage --json` could not read the native host-login material. It returned an `unavailable` outcome with reason `no_credentials` for both `a14t08-kd` and `a14t08-kl`. This usage reporter result is not a sandbox membership measurement.

No guest reached inference. Therefore nonzero guest membership usage and subscription billing attribution are **unproven**. No client token estimate or host response is substituted for that missing evidence.

Each sandbox had only its matching global Kimi provider. The guest config had only `managed:kimi-code`, an empty `api_key`, and file-backed OAuth. The broker value was an OpenShell handle, not a membership key. No alternate paid provider or static-key fallback was configured, and no paid fallback occurred during the failed requests. This structural result does not establish subscription consumption.

## Secret containment and regional policy

A temporary helper used `read_kimi_host_login` and compared the current host access and refresh grants with captured guest files, environment, project state, and inspect output in memory. It printed only counts and booleans. The helper was deleted after the checks.

| Gateway | Guest bytes | Inspect bytes | Guest access matches | Guest refresh matches | Inspect access matches | Inspect refresh matches |
|---|---:|---:|---:|---:|---:|---:|
| `jackin-docker` | 1,311,876 | 558 | 0 | 0 | 0 | 0 |
| `jackin-local` | 1,292,286 | 558 | 0 | 0 | 0 | 0 |

Inspect listed only `jackin-a14t08-kd-kimi` on docker and `jackin-a14t08-kl-kimi` on local. Both had a loaded policy and zero closure violations. Direct non-agent `curl` to `https://api.kimi.ai/coding/v1/models` failed with exit 7 and HTTP `000` on both gateways. `https://example.com` failed the same way. Non-agent denial passes. Positive provider-only agent egress is unproven because no agent request reached inference.

## Replay and shell persistence

Two docker PTY attaches and one local PTY attach detached cleanly with exit 0. They replayed the folder-trust screen. Replay passes, but it does not show an authenticated working session.

A shell pane on each gateway returned uid 1000 and `/sandbox/project`, then exited 0. The docker `kimi-code` PID 363 remained a direct child of PID 64. The local `kimi-code` PID 685 remained a direct child of PID 371. Shell persistence passes.

## Lead and secondary homes

Both live runs exercised the lead home at `/sandbox/.kimi-code`. A live secondary Kimi instance requires a different supported lead agent because a launch rejects repeated agent types. The minimal supported lead considered was Claude/Z.AI. Its expected approved plan-key file `/tmp/jackin-plan-keys/zai` returned exit 1 from `test -f`. No unrelated credential was imported to manufacture that dimension.

The existing focused planner test covered the secondary path `/sandbox/.jackin/homes/kimi/.kimi-code`:

```text
mise exec -- mbx +1.99.1 nextest run -p jackin-session --test plan -E 'test(kimi_files_relocate_to_lead_and_secondary_homes)'
PASS native::kimi_files_relocate_to_lead_and_secondary_homes
```

Nextest ran one test with one pass and reported 135 skipped by the filter. Only the focused planner behavior is a pass. Live secondary-home placement remains unproven and is not promoted from this hermetic result.

## Export and guarded purge

Initial guarded purge refused each sandbox with exit 3 for `uncommitted changes`. Sandbox-only commits then created clean unexported branch tips. No host repository commit or push occurred.

A second guarded purge refused each sandbox with exit 3 for `branch feature/openshell-native not exported`. Export created exactly these refs:

```text
refs/heads/jackin/jackin-a14t08-rff6/feature/openshell-native e5eabe1bfb4461dc221ab1f3ea84a2da24600a7e
refs/heads/jackin/jackin-a14t08-09nw/feature/openshell-native 0c49d50d411aaaf2ecafcdf43ca91ae983cbbfe6
```

The temporary host HEAD stayed `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its tracked-diff SHA-256 stayed `26a9a973b55af8e1757c4aad8e27582ca2c62347495eddf8fbc971ee98be98da`, and its status remained one modified role manifest plus the new Kimi instruction file. Guarded purge then deleted both sandboxes without `--force`.

## Host usability correction and result

A host-only diagnostic with bare `--model kimi-for-coding` failed because that alias is not configured in the host config. Subsequent host checks left model selection at the configured native default. Host Kimi 2.5.2 answered exact `HOST-T08-BEFORE` before the sandbox runs and exact `HOST-T08-AFTER` after them. These host responses prove login usability only. They are not guest inference or membership-attribution evidence.

## Cleanup and restoration

Both exported refs were deleted. Accounts `a14t08-kd` and `a14t08-kl`, their providers, workspace `a14t08`, and role `a14t08-sentinel` were removed. No owned sandbox remains. The temporary auth-scan source was deleted.

`git restore -- crates/jackin-core/src/agent.rs` restored T08 to `Unavailable` with reason `native refresh and final live proof pending`. `git diff --exit-code -- crates/jackin-core/src/agent.rs` exited 0. The blocker update records the new live failure without changing the catalog.

The unique role and derived image tags remain in the BuildKit cache. Jackin has no owned image-removal command, and no direct Docker removal bypassed the `jackin-image` boundary.

A concurrent docker sandbox `jackin-a14t13m-als5` and provider `jackin-a14t13ml-claude-kimi-claude` appeared during this proof. They and every other foreign resource were left untouched. No full G-FAST, host commit, or push occurred.

## Required dispositions

| Requirement | Result |
|---|---|
| Actual region on docker, then local | PASS, global through the approved native reader |
| Other region | NOT RUN, mainland grant file absent |
| Cached rebuild and provider-present relaunch | PASS on both gateways |
| uid 1000 and no sudo | PASS on both gateways |
| Supervised native Kimi direct child | PASS on both gateways |
| Exact guest config and auth placeholders | PASS for the rendered stable contract |
| Gateway refresh | PASS on both gateways |
| Fixed `PONG` reply with `kimi-for-coding` | FAIL, model resolution rejected before inference |
| Real successful tool | FAIL, no model request could run a tool |
| Post-refresh reply | FAIL, same model-resolution error |
| Nonzero membership usage | UNPROVEN, no guest inference occurred |
| No host-key disclosure | PASS, zero matches in all captured classes |
| Regional closure and non-agent denial | PASS |
| Positive provider-only agent egress | UNPROVEN, no agent inference occurred |
| Replay and shell persistence | PASS, replay shows the trust screen |
| Lead home | PASS live on both gateways |
| Secondary home | PASS only in the focused hermetic planner test, live UNPROVEN |
| Exact export and unchanged host state | PASS |
| Guarded purge | PASS |
| Host login after runs | PASS, exact `HOST-T08-AFTER` |
| No paid fallback | PASS for configured sources, but no subscription billing claim |
