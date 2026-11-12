# A14 final-head Muse Code subscription re-check

Date: 2026-11-10. Base HEAD: `d9411a2d72a3710a28675072adef1bcc64c07093`. Worktree: `/tmp/jackin-a14-muse-final`. Branch: `wt/a14-muse-final`.

## Verdict

T04 remains **unavailable**. The existing provider on `jackin-local` does not permit a live Muse session. Muse rejects the gateway-held credential before a reply or tool call. A launch on `jackin-docker` requires a fresh host-login import because that gateway has no Muse provider. The interactive Keychain read times out at the provider step.

The two failures have different conditions. Neither supports a catalog flip or any SKIP-to-PASS conversion.

## Vehicle and credential handling

OpenShell reported 0.1.7, matching `compatibility.lock.toml`. Host Muse reported `Muse Code 1.4.14 (1.4.14-R7309.1)`. No Docker, Podman, Apple container, or host agent command ran a workload. Image construction used only the jackin launch path.

The only jackin source change during the proof set T04 to `TupleState::Supported` with no reason. This command exited 0:

```text
mise exec -- mbx +1.99.1 build -p jackin
```

The proof binary was `/tmp/jackin-a14-muse-final/target/debug/jackin`. A disposable sentinel worktree started at `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its uncommitted edits named the role `a14mf-sentinel` and added the Muse instruction mapping. `jackin role check .` exited 0. The launch planner supplied read-only `/` for Muse.

An isolated `XDG_CONFIG_HOME` contained the two existing gateway entries, the registered `muse-live` host-login account, the `a14mf-sentinel` role, and the `a14mf` workspace. Its OpenShell path referenced the existing mTLS registration. An isolated `XDG_CACHE_HOME` held the account locks. No command wrote the host jackin configuration.

The local launch used the existing `jackin-muse-live-muse` provider through the provider-present path. No API key replaced the subscription login. No fresh import ran on local. Docker genuinely required the host-login import because no Muse provider was present there. No manual Keychain read or other workaround bypassed that requirement.

## Host login before the launches

The selected non-secret host metadata reported:

```json
{
  "storage": "keychain",
  "subs_active": true,
  "api_base_url": "https://api.meta.ai/v1"
}
```

The metadata file had mode 0600, size 771 bytes, and mtime `2026-11-09T19:44:35+0700`. An existence-only Keychain check for service `ai.meta.dev.credentials`, account `meta`, exited 0. No secret value was read or printed by these checks.

`muse usage --json` completed without a model turn. It returned `allowance=null` and `reason=server_sent_no_usage`. This response did not provide a usage baseline or a consumption delta.

## jackin-local fails at the existing credential

At `2026-11-10T05:41:23Z`, the launch used account `muse-live`, model `muse-spark-1.2`, and launch ID `a14mft04localproof00000001`. It built the Muse 1.4.14 image, printed `provider: skipped, the provider exists`, and created sandbox `jackin-a14mf-to5i`. The derived image ID was `sha256:5b25264593f6959925f48d9e34607738274c07f3819cbca0f89becc3b50f3d33`.

The sandbox reported `uid=1000(sandbox)` and Muse 1.4.14. `command -v sudo` produced no path. The process list showed `/bin/sh /usr/local/bin/jackin-start muse`, but no running Muse child. `jackin inspect --json` listed only `jackin-muse-live-muse`, no policy-closure violations, and supervisor state `Exited` with code 1.

At `2026-11-10T05:42:10Z`, the exact-model one-shot request exited 1 immediately with:

```text
failed to fetch model catalog: authentication failed: your API key from META_API_KEY was rejected — update or unset it
```

The request returned no `A14MF-PONG` and made no tool call. The supervised launch therefore fails the eligibility check. `openshell provider refresh status jackin-muse-live-muse` reported no refresh configuration. This run did not refresh, replace, or delete the existing provider.

## jackin-docker fails at the required fresh import

At `2026-11-10T05:42:29Z`, the equivalent launch used launch ID `a14mft04dockerproof0000001`. The construct and derived images were cached. The launch then failed at the provider step:

```text
error: launch a14mft04dockerproof0000001 failed at step provider: the provider for account muse-live is unavailable: no credential is available for account muse-live
real 31.08
```

The 30-second host-login Keychain read in `sync_muse_login` did not complete. No interactive approval bypass was attempted. The docker provider inventory still had no Muse provider after the failure, and the sandbox inventory had no `a14mf` resource.

## Unproven requirements

The failed eligibility checks stopped the complete two-gateway proof. The following requirements remain unproven:

- cached rebuild on both gateways
- a running direct Muse child of `jackin-start`
- an exact `muse-spark-1.2` live reply and a real tool call
- placeholder files and non-disclosure comparisons
- provider-only egress and denial of non-agent destinations
- detach, replay, and shell persistence
- exact export with unchanged host state
- successful guarded purge after export
- subscription consumption and the absence of paid fallback

The local UID and missing-`sudo` observations do not prove those requirements on both gateways. Docker's cached image output does not prove a cached rebuild on local. The local supervisor exit does not prove direct-child process persistence.

Guarded purge of `jackin-a14mf-to5i` refused with exit 3 because the workspace had uncommitted changes. The sandbox and disposable role had the same HEAD, the same modified `jackin.role.toml`, and the same untracked `instructions/muse.md`. No agent commit or extra file was present. Cleanup then used `--force` for this owned proof sandbox. This is not a successful guarded-purge proof.

## Host login afterward and host state

`muse usage --json` again completed without a model turn and returned `server_sent_no_usage`. The selected host metadata still reported Keychain storage and an active subscription. The existence-only Keychain check still exited 0. These checks show host login usability after the gateway failures, but they do not replace a model-turn proof.

The host Muse metadata file remained mode 0600, but its size changed to 773 bytes and its mtime changed to `2026-11-10T12:43:37+0700`. Unchanged host Muse file state is therefore not claimed. The host jackin configuration SHA256 remained `5f5b215bf27638cabb0a9e0d33942ad45117f9c791c4144d75d402115348e1c3` before and after the launches. The original sentinel checkout remained clean at `6722f8024623c76726ce0293fc7e93da47b5ee74`.

No usage allowance delta is available because the server supplied no allowance in either response. The failed model-catalog request cannot prove the absence of paid fallback.

## Cleanup and restoration

The owned local sandbox was deleted. Both gateway sandbox inventories contain no `a14mf` resource. Docker created no Muse provider. The pre-existing local provider remains. Foreign providers and sandboxes were not changed.

The T04 source edit was restored. `git diff --exit-code -- crates/jackin-core/src/agent.rs` exited 0. A second `mise exec -- mbx +1.99.1 build -p jackin` rebuilt the restored catalog and exited 0. No full G-FAST run, commit, or push was requested or performed.

The local image store retains the `a14mf-sentinel` role and derived tags created by the launch. Jackin exposes no owned image-cleanup command, so no manual container-engine command removed them.
