# A14 T08 native Kimi Code subscription live proof after the model-key fix

Date: 2026-11-10. Base: `9e231545cb692e80153b2fa6aa8e7f67eb17c81c`. Worktree: `/tmp/jackin-a14-t08-model-live`. Branch: `wt/a14-t08-model-live`.

## Verdict

T08 is **PASS** for the requested one-shot live proof recorded here. Native Kimi Code 2.5.2 answered exact prompts and completed a real tool call on `jackin-docker` and `jackin-local`. Explicit `kimi-code/kimi-for-coding` requests worked before and after gateway refresh. The earlier `Model "kimi-code/kimi-for-coding" is not configured in config.toml` failure did not recur.

The proof used the native global subscription only. Placeholders, process supervision, cached rebuilds, regional egress, terminal behavior, export, host invariance, and guarded purge passed on both gateways. No paid fallback ran.

The catalog remains `Unavailable` because the temporary proof enablement was restored as ordered. Production promotion still needs the permanent catalog change, a repair for the unrelated clean-head compile error, and the normal review and gate sequence.

## Promotion status: PENDING, not promoted

This record is the live proof of the one-shot lane only. It is **not** a promotion, and the T08 tuple stays `TupleState::Unavailable` on `main` with `rewrite/blockers/T08-kimi-code.md` present.

The promotion is held pending the supplementary legs, which this proof does not cover:

- supervised interactive inference to the exact reply marker,
- live secondary-home Kimi execution,
- vendor-side usage attribution with no overage.

`rewrite/decisions.tsv:510-511` records that hold and the quota failure that stopped the supplementary attempt, and `rewrite/decisions.tsv:514` records the restoration of the blocker while the tuple stays unavailable. `rewrite/evidence/A14-t08-supplementary.md:16-28` records the same legs as still unproven and states that T08 stays `Unavailable` until the Kimi weekly window resets.

So: the promotion is **not** in effect. The only promotion-coupled product change in the source branch was a temporary proof-time flip that was restored, and the permanent catalog promotion lands only on a full pass of the supplementary legs.

## Proof vehicle and isolation

The proof binary was `/tmp/jackin-a14-t08-model-live/target/debug/jackin`. The base contains `571cb32ed773f3dcad459522a516e271b2c069bf`, which renders the model table as `[models."kimi-code/{model}"]`.

The requested head had an unrelated compile error. `crates/jackin-session/src/build.rs` imported the absent `validate_tuple_selection` function. The first `mise exec -- mbx +1.99.1 build -p jackin` exited 101. A temporary proof-only correction changed that import and call to the existing `lead_tuple` function and made `lead_tuple` `pub(crate)`. The next build exposed the missing visibility, and the completed correction built successfully in 10.95 seconds. Both compile corrections were restored after the proof.

The only other temporary product-code change was T08 `TupleState::Unavailable` to `Supported` and its reason to `None`, and it was restored after the proof. No test was added or changed. No full G-FAST ran, as ordered.

OpenShell reported 0.1.7. `jackin doctor` passed binary, registration, health, pinned-version, and engine checks on both gateways. Docker BuildKit built images only through `jackin-image`. OpenShell ran every sandbox and guest command. No Docker, Podman, Apple container, or host-agent fallback ran a sandbox or guest agent.

The proof used `XDG_CONFIG_HOME=/tmp/a14t08m-xdg` with an OpenShell registry symlink to the existing gateway registrations. The host Jackin configuration was not edited. Its SHA-256 was `5f5b215bf27638cabb0a9e0d33942ad45117f9c791c4144d75d402115348e1c3` before and after the proof.

The temporary sentinel worktree was `/tmp/jackin-a14-t08-model-live-sentinel` at `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its role name was `a14t08m-sentinel`. It added only the Kimi instruction mapping with model `kimi-for-coding`. `jackin role check` exited 0. The original sentinel checkout remained clean at the same commit.

## Credential discovery and region

Host native Kimi login discovery came first. Host Kimi 2.5.2 answered exact `HOST-T08-MODEL-BEFORE` before the sandbox runs and exact `HOST-T08-MODEL-AFTER` after them. These host replies prove login usability only.

The host `omp` store was checked second without exporting credentials. Its `auth_credentials` rows contained only provider `zai`, with `api_key` and `oauth` credential types. It had no Kimi row. The native Kimi login was already available, so no 1Password lookup was needed and `op` was not invoked.

Both accounts used `--from-host-login`. The approved `read_kimi_host_login` path reported `Global` in the sanitized containment helper. Providers `jackin-a14t08mkd-kimi` and `jackin-a14t08mkl-kimi` had type `kimi-global`, one `KIMI_API_KEY` broker key, and refresh strategy `oauth2_refresh_token`. That key name is the OpenShell broker name, not a membership-key source.

The selected global grant file `/Users/donbeave/.kimi-code/credentials/kimi-code-env-0e4f99c69cc27850.json` passed an existence-only check. `/Users/donbeave/.kimi-code/credentials/kimi-code.json` did not exist. Mainland China was not exercised, and no grant or configuration was invented for it.

No membership API key, static Kimi file, environment key, 1Password item, `omp` credential, or existing gateway provider supplied the T08 credential.

## Images and cached relaunch

The docker sandbox was `jackin-a14t08m-uvgw` with launch ID `a14t08mkimd01`. The local sandbox was `jackin-a14t08m-4tf6` with launch ID `a14t08mkiml01`. Both used Kimi Code 2.5.2 with the catalog SHA-256 `ac941c80c8b2b34451432323397a8e8c2e376948bc6d878fc7d2630be235b678`.

The docker build used role tag `jackin.invalid/role/a14t08m-sentinel:d940d2caf3c00c05` and derived tag `jackin.invalid/a14t08m-sentinel:d940d2caf3c00c05`. The derived image manifest was `546cf9a8d1f630b5b1f1f343aecc65b8aaa6d838be437028f0a63152ffb40a8b`.

The first docker launch printed `image derived: built`. Its same-launch relaunch printed:

```text
image construct: cached
image derived: cached
image: skipped, the image is cached
provider: skipped, the provider exists
sandbox: skipped, the sandbox exists
upload: skipped, the project is uploaded
```

The local launch used the cached derived image. Its same-launch relaunch printed the same cached, provider-present, existing-sandbox, and uploaded-project results.

## Exact inference and tool calls

The fixed request `kimi -p "Reply with exactly: PONG" --output-format stream-json` returned this assistant record on each gateway:

```json
{"role":"assistant","content":"PONG"}
```

The explicit model requests and post-refresh requests produced these exact assistant records:

| Gateway | Request model | Exact reply |
|---|---|---|
| `jackin-docker` | `kimi-code/kimi-for-coding` | `KIMI-FOR-CODING-LIVE` |
| `jackin-docker` after refresh | `kimi-code/kimi-for-coding` | `POST-REFRESH-T08-DOCKER` |
| `jackin-local` | `kimi-code/kimi-for-coding` | `KIMI-FOR-CODING-LOCAL` |
| `jackin-local` after refresh | `kimi-code/kimi-for-coding` | `POST-REFRESH-T08-LOCAL` |

The docker tool request ran `printf TOOL-T08-DOCKER`. Stream JSON contained the `Bash` tool call, a tool result with content `TOOL-T08-DOCKER`, and final assistant content `TOOL-T08-DONE`. The local request ran `printf TOOL-T08-LOCAL`, returned that tool content, and ended with `TOOL-T08-LOCAL-DONE`.

Kimi session `usage.record` records named model `kimi-code/kimi-for-coding` and nonzero input and output tokens. The docker explicit-model and tool sessions recorded 21, 28, and 62 output tokens. The local corresponding sessions recorded 26, 47, and 61 output tokens. These are guest client usage records, not a separate vendor billing report.

`jackin usage --json` still returned `unavailable` with reason `no_credentials` for the native host-login account. This usage-reporter limitation did not prevent guest inference or gateway refresh, and no host response was substituted for guest usage.

## Refresh

The final sanitized refresh observations were:

| Gateway | Status | Expires at UTC | Next refresh UTC | Last refresh UTC |
|---|---|---|---|---|
| `jackin-docker` | `refreshed` | `2026-11-10 10:17:45` | `2026-11-10 10:12:45` | `2026-11-10 10:02:45` |
| `jackin-local` | `refreshed` | `2026-11-10 10:25:23` | `2026-11-10 10:20:23` | `2026-11-10 10:10:23` |

The exact post-refresh replies in the previous table occurred after these observations.

## Process and exact guest files

Both sandboxes reported `uid=1000(sandbox) gid=1000(sandbox)`. `sudo -n true` failed with exit 127 because `sudo` was absent.

On docker, `kimi-code` PID 373 was the direct child of `/bin/sh /usr/local/bin/jackin-start kimi` PID 64. On local, `kimi-code` PID 680 was the direct child of `jackin-start` PID 372. The same docker PID and parent remained after a shell pane. The same local PID and parent remained after its shell pane. Both supervisors reported `Running` with attempt 1.

The docker and local config files were byte-identical, mode `0600`, owner `sandbox`, and size 488 bytes:

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

[models."kimi-code/kimi-for-coding"]
provider = "managed:kimi-code"
model = "kimi-for-coding"
max_context_size = 262144
capabilities = ["thinking", "always_thinking", "image_in", "video_in", "tool_use"]
```

The auth file was `/sandbox/.kimi-code/credentials/kimi-code-env-0e4f99c69cc27850.json`, mode `0600`, owner `sandbox`, and size 210 bytes on each gateway. Sanitized `jq` checks established six fields. `access_token` was an `openshell:resolve:` handle of length 100. `refresh_token` was empty. `expires_at` and `expires_in` were numeric zero. `scope` was `kimi-code`. `token_type` was `Bearer`. `KIMI_API_KEY` was also an `openshell:resolve:` placeholder, and `KIMI_CODE_HOME` was unset.

`kimi provider list` reported only `managed:kimi-code type=kimi models=1 source=oauth` and default model `kimi-code/kimi-for-coding`.

## Secret containment and provider-only egress

A temporary Rust helper used the approved `read_kimi_host_login` reader. It compared the current host access and refresh grants with captured guest files, environment, and inspect output in memory. It printed only region, byte counts, and booleans. Its source and binary were deleted after the checks.

| Gateway | Guest bytes | Environment bytes | Inspect bytes | Guest access matches | Guest refresh matches | Environment matches | Inspect matches |
|---|---:|---:|---:|---|---|---|---|
| `jackin-docker` | 1,884,160 | 704 | 211 | 0 | 0 | 0 | 0 |
| `jackin-local` | 1,843,200 | 704 | 211 | 0 | 0 | 0 | 0 |

Each sandbox had only its matching global Kimi provider. `jackin inspect` reported that provider, a loaded policy, and no other account or provider. The successful agent replies prove positive provider egress. Direct non-agent `curl` to `https://api.kimi.ai/coding/v1/models` failed with exit 7 and HTTP `000` on both gateways. `https://example.com` failed the same way. A non-agent binary received no usable credential or provider route.

The guest config had only `managed:kimi-code`, an empty `api_key`, and file-backed OAuth. No alternate paid provider or static-key fallback was configured or used.

## Replay and shell persistence

Two docker PTY attaches detached cleanly. The first replayed the folder-trust screen. After trust was accepted, the second attach showed the Kimi session UI and detached cleanly. Two local PTY attaches replayed the folder-trust screen and detached cleanly. Replay does not constitute the exact inference proof, which came from the direct guest requests above.

A shell pane on each gateway returned uid 1000 and `/sandbox/project`, then exited 0. The supervised `kimi-code` PID and its `jackin-start` parent remained unchanged.

## Export, host invariance, and guarded purge

Initial guarded purge refused each sandbox with exit 3 for `uncommitted changes`. Sandbox-only commits then created clean unexported branch tips. No host repository commit or push occurred.

A second guarded purge refused each sandbox with exit 3 for `branch feature/openshell-native not exported`. Export created exactly these refs:

```text
refs/heads/jackin/jackin-a14t08m-uvgw/feature/openshell-native 6a592792fd10c0b98b725a2942772a15f96643df
refs/heads/jackin/jackin-a14t08m-4tf6/feature/openshell-native 5168e80a6dae057d15f79ed35046af026e6939f8
```

Before and after each export, the temporary host HEAD stayed `6722f8024623c76726ce0293fc7e93da47b5ee74`. The tracked-diff SHA-256 stayed `5fc012138f0634f9f3acafd1ef8415e6b8e6a98c11f8fdca78a250ee8ae1f998`. The cached-diff SHA-256 stayed `01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b`. The status SHA-256 stayed `8955bea8d70a5544bed4e889d01d5462820ec9b7283586b82280b856d0aea73f`.

Guarded purge then deleted both sandboxes without `--force`.

## Cleanup and restoration

Both exported refs were deleted. Accounts `a14t08mkd` and `a14t08mkl`, their providers, the imported `kimi-global` profiles, OpenShell workspace `a14t08m` on both gateways, and role `a14t08m-sentinel` were removed. No owned sandbox, provider, profile, workspace, or exported ref remains. Foreign resources were left untouched.

The temporary XDG directory and sentinel worktree were removed. `git restore -- crates/jackin-core/src/agent.rs crates/jackin-session/src/build.rs crates/jackin-session/src/plan.rs` restored T08 and the compile correction. The unique role and derived image tags remain in the BuildKit cache. No direct Docker removal bypassed the `jackin-image` boundary.

No full G-FAST, host commit, push, or merge occurred.

## Required dispositions

| Requirement | Result |
|---|---|
| Actual global region on docker, then local | PASS |
| Mainland grant absence | CONFIRMED, not run |
| Cached rebuild and provider-present relaunch | PASS on both gateways |
| uid 1000 and no sudo | PASS on both gateways |
| Native Kimi direct child of `jackin-start` | PASS on both gateways |
| Exact qualified model config and placeholders | PASS on both gateways |
| Fixed `PONG` and explicit `kimi-for-coding` replies | PASS on both gateways |
| Real successful tool call | PASS on both gateways |
| Gateway refresh and post-refresh reply | PASS on both gateways |
| Guest token records for the exact model | PASS, client records only |
| Host-key disclosure checks | PASS, zero matches |
| Provider-only agent egress and non-agent denial | PASS |
| Detach, reattach, and shell persistence | PASS |
| Exact export and unchanged host state | PASS |
| Guarded purge before and after export | PASS |
| Host login after runs | PASS |
| No paid fallback | PASS |
| Production catalog promotion | PENDING, temporary enablement restored; held pending the supplementary legs of `rewrite/evidence/A14-t08-supplementary.md:16-28` and `rewrite/decisions.tsv:510-511,514` |
| Clean requested-head build | PENDING, unrelated `validate_tuple_selection` compile repair needed |
