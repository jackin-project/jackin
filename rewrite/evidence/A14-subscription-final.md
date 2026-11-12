# A14 final live proof for T02 and T05

Date: 2026-11-09. Base HEAD: `10f227145e7a1aabf93b7e293af7de4a64603c70`. Worktree: `/tmp/jackin-a14-subscription-final`, branch `wt/a14-subscription-final`.

## Verdicts

- T02, Claude Code with an Anthropic subscription: **FAIL**. One-shot Claude requests used the gateway-held subscription credential and answered on both gateways. The supervised interactive agent exited because the T02 policy refused `platform.claude.com`. The running-agent, shell-pane, and gateway-refresh requirements are unproven.
- T05, Antigravity with a Google subscription: **FAIL**. The pinned image built and `agy` ran without desktop keyring facilities. The fixed prompt failed on both gateways with `authentication required`. No live model response or subscription consumption occurred.
- Neither result supports a catalog flip. The proof-only T02/T05 state changes were reverted before G-FAST.

## Vehicle and gateways

The only product-code diff during the proof was four lines in `crates/jackin-core/src/agent.rs`. T02 and T05 changed from `TupleState::BlockedVendor` to `Supported`, and their reasons changed to `None`. `AuthClass::BlockedVendor` remained unchanged. `mise exec -- mbx +1.99.1 build -p jackin` passed. The binary was `/tmp/jackin-a14-subscription-final/target/debug/jackin`.

The role was a temporary clone of sentinel at `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`. Its uncommitted changes named the role `a14sub-sentinel`, pinned the existing catalog models, and added the agy instruction mapping. Docker was disabled. `jackin role check /tmp/jackin-a14-sentinel-final` exited 0. The temporary workspace was `a14sub`.

At 2026-11-09T13:03:13Z, `jackin doctor` passed on `jackin-docker` and `jackin-local`. Both reported OpenShell 0.1.7 and gateway health `serving`. `openshell --version` reported 0.1.7. `jackin-docker` used the OrbStack builder on linux/arm64. Every image build used `DOCKER_CONTEXT=orbstack`. No Docker, Podman, or Apple container command ran an agent or sandbox.

The runs were sequential. T02 finished and its accounts were removed before T05 began. The host agents were used only for authentication and vendor-account checks, never as a substitute for a sandbox response.

## T02 live evidence

### Import and provider

At 2026-11-09T13:04:04Z, unmodified host Claude 2.1.319 answered `PONG`. The `Claude Code-credentials` Keychain item existed. The host credential file was not read for this check.

The current import path succeeded on each gateway:

```text
jackin --gateway jackin-docker account add claude --name a14sub-cd --agent claude --from-host-login
jackin --gateway jackin-local  account add claude --name a14sub-cl --agent claude --from-host-login
```

`jackin-a14sub-cd-claude` existed on `jackin-docker`. `jackin-a14sub-cl-claude` existed on `jackin-local`. Each provider used profile `claude` and had one credential key. No secret appeared in command output.

### Image and cached rebuild

The primary launch used sandbox `jackin-a14sub-d9up` and launch ID `myzaiyy5neldj7vq7t26f2niiu`. The derived image was `sha256:0022bc2203ede2da89531e880caad557136a7db7b349846cdf955179130dc06c`, with Claude 2.1.311 from the catalog artifact. The initial launch printed `image derived: built`.

A relaunch with the same launch ID printed:

```text
image construct: cached
image derived: cached
image: skipped, the image is cached
sandbox: skipped, the sandbox exists
upload: skipped, the project is uploaded
```

The `jackin-local` launch also printed `image derived: cached` and created `jackin-a14sub-0rra`.

### Live response and subscription attribution

This command class ran in each sandbox with the catalog model:

```text
claude -p "Reply with exactly: PONG" --model haiku --output-format json
```

The docker result included:

```json
{"result":"PONG","models":["claude-haiku-5-5"],"is_error":false,"total_cost_usd":0.00019958}
```

The local result included:

```json
{"result":"PONG","models":["claude-haiku-5-5"],"is_error":false,"total_cost_usd":0.0008628900000000001}
```

The JSON cost fields are recorded without interpreting them as invoices. Vendor-visible `claude auth status --json` reported this account state after the docker run:

```json
{"loggedIn":true,"authMethod":"claude.ai","planTier":"max_20x","subscriptionType":"max","storedClaudeAiLogin":true,"apiProvider":"firstParty"}
```

The only OpenShell provider attached to each sandbox was the matching T02 provider. `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, and `OPENAI_API_KEY` were absent from both sandbox environments. No alternate base URL or paid provider was configured. The responses therefore used the imported Claude.ai subscription credential. No paid fallback was available in the tuple.

### Placeholder and secret checks

An in-memory comparison parsed the host Keychain access and refresh tokens, then compared them with captured sandbox environment output, auth-file contents, and `jackin inspect --json` stdout and stderr. It printed only counts and booleans. Both gateways reported:

```text
environment: host-credential-matches=0
auth-files: host-credential-matches=0
inspect: host-credential-matches=0
expected-env=CLAUDE_CODE_OAUTH_TOKEN:set=True,literal-placeholder=False,host-credential-match=False
literal-placeholder-count=1
closure-violations=0
```

`CLAUDE_CODE_OAUTH_TOKEN` held an OpenShell broker reference, not the host token. `/sandbox/.claude/.credentials.json` contained the literal placeholder, had mode 0600, and was owned by sandbox. `/sandbox/.claude/settings.json` had mode 0600. No host credential appeared in the captured files, environment, inspect output, or stderr.

Both sandboxes ran as `uid=1000(sandbox)` with no `sudo`. Requests to `platform.claude.com` and `example.com` from the docker sandbox failed with `Could not connect to server`. Policy closure reported zero violations.

### Exact supervised-agent defect

The smallest failing condition is an ordinary supervised T02 launch with the current `t02-claude.yaml` profile. The profile permits only `api.anthropic.com`. Interactive Claude 2.1.311 also requests `platform.claude.com`.

On `jackin-docker`, `/sandbox/.jackin/run/state` contained `exited 1 1`. `ps` showed `jackin-start` but no Claude child. Two PTY attach sessions replayed:

```text
Welcome to Claude Code v2.1.311
Unable to connect to Anthropic services
Failed to connect to platform.claude.com: ECONNREFUSED
jackin-start: claude exited with code 1 (attempt 1)
```

The second replay included attempt 2 after the cached relaunch. Both attaches detached with exit 0. A shell pane exited with code 0 and left `jackin-start` running, but no supervised Claude process existed. The child-of-`jackin-start` and running-agent requirements fail. Detach and replay work, but they replay the failure.

`jackin-local` also contained `exited 1 1` after the same launch shape. Its one-shot request succeeded independently.

### Host-login and refresh checks

At 2026-11-09T13:12:01Z, with the docker sandbox present, host Claude answered `HOST-PONG`. The next sandbox request answered `SANDBOX-PONG`. Host `claude auth status` still reported the `max` subscription, and the Keychain item remained present. Host Claude answered `PONG` again after the local sandbox was removed.

The command `openshell -g jackin-docker provider refresh status jackin-a14sub-cd-claude` returned:

```text
No refresh configurations found for provider 'jackin-a14sub-cd-claude'.
```

The run proves that sandbox consumption did not log out the host. No gateway refresh occurred, so a refresh race is unproven.

### Export and purge

A new sandbox commit made the guard condition. Guarded purge failed with exit 3:

```text
error: the work in sandbox jackin-a14sub-d9up is not exported (branch feature/openshell-native not exported); run jackin export jackin-a14sub-d9up or pass --force
```

`jackin export` published only this ref in the temporary clone:

```text
refs/heads/jackin/jackin-a14sub-d9up/feature/openshell-native  0cc77fa5d7003f333bc28773f73b0ce76955b60c
```

The host HEAD stayed `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`. Its pre-existing proof-only role edits were unchanged. Guarded purge then deleted the sandbox. The local sandbox had no new work and purged successfully without `--force`.

## T05 live evidence

### Import and image

At 2026-11-09T13:15:00Z, unmodified host agy 1.5.3 answered `PONG`. Keychain service `gemini`, account `antigravity`, existed. The current imports succeeded:

```text
jackin --gateway jackin-docker account add antigravity --name a14sub-gd --from-host-login
jackin --gateway jackin-local  account add antigravity --name a14sub-gl --from-host-login
```

`jackin-a14sub-gd-agy` and `jackin-a14sub-gl-agy` existed on their respective gateways. Each used profile `agy` and had one credential key.

The primary launch created `jackin-a14sub-j13h` with launch ID `5gkrvlfy4z4plzwheno4mj4hoq`. The image built agy `1.5.3-6604829099950080` from the pinned arm64 tarball with SHA256 `2d50060c7b1327995e5c81ec58829c91c2564ebb9fec165d3f9f1325ec188675`. The derived image was `sha256:7b6da349475aa4f9d6aa126acc9345a59e4ec2d0d70d2acb639412918a85b834`.

A same-launch relaunch printed `image derived: cached`, `image: skipped, the image is cached`, and `sandbox: skipped, the sandbox exists`. The `jackin-local` launch also used the cached image and created `jackin-a14sub-5vei`.

### No-keyring result and exact authentication defect

On both gateways, the sandbox had no `dbus-daemon`, `busctl`, or `secret-tool`. `/run/dbus` was absent, and `DBUS_SESSION_BUS_ADDRESS` was unset. Both sandboxes ran as uid 1000 with no `sudo`.

The docker process list showed:

```text
PID  PPID  USER     COMMAND
64   1     sandbox  jackin-start
363  64    sandbox  agy
```

The local process list showed the same parent relationship with agy PID 668 and `jackin-start` PID 373. The supervised agy process therefore ran without a desktop keyring.

The fixed prompt used only the catalog model:

```text
openshell -g jackin-docker sandbox exec jackin-a14sub-j13h -- sh -c 'cd /sandbox/project && agy -p "Reply with exactly: PONG" --model gemini-4-argon'
```

It exited 1 on docker and on local with this exact sanitized output:

```text
Error: authentication required. Run 'agy' to log in, then retry.
error: authentication failed or timed out
```

Two docker PTY attaches replayed the Antigravity welcome screen with `You are currently not signed in` and login choices. No interactive login was attempted. A shell pane exited 0, and agy PID 363 remained a child of `jackin-start` PID 64. Detach, replay, and shell persistence passed. The agent remained at a login screen rather than a working session.

The smallest failing condition is agy 1.5.3 with the gateway-brokered `GOOGLE_API_KEY` placeholder and no host keyring. The CLI asks for a new login before a model request. The current provider profile and placeholder settings do not authenticate the subscription. The host `agy models` output listed `gemini-4-argon-medium`, `gemini-4-argon-high`, and `gemini-4-argon-low`, but not bare `gemini-4-argon`. Authentication failed before model selection, so the catalog model remains unproven rather than declared invalid.

### Credential and egress checks

The same in-memory host-token comparison reported zero matches in environment, auth-file contents, and inspect output on both gateways. `GOOGLE_API_KEY` held an OpenShell broker reference, not the host token. `GEMINI_API_KEY`, `ANTHROPIC_API_KEY`, and `OPENAI_API_KEY` were absent. The files did not contain the broker reference. Policy closure had zero violations.

After agy rewrote `settings.json`, the file had mode 0600 and contained neither a literal placeholder nor a host or broker credential. This is the observed post-start state, not a claim about the pre-start writer. No real credential appeared in captured files, environment, inspect output, or stderr.

`example.com` and `generativelanguage.googleapis.com` were refused on docker. The only attached provider was the matching T05 provider. No model response occurred, so this run provides no evidence of subscription consumption. No paid fallback occurred. Vendor-visible quota movement and model attribution are unproven.

Host agy answered `PONG` at 2026-11-09T13:18:55Z after the docker checks and again after local cleanup. The host Keychain item remained present.

### Export and purge

A new sandbox commit caused guarded purge to refuse with exit 3 and `branch feature/openshell-native not exported`. Export published:

```text
refs/heads/jackin/jackin-a14sub-j13h/feature/openshell-native  3767fd82eb3b867a9ab3d5de422c4c5c2967cf96
```

The temporary host HEAD and working tree did not change. Guarded purge then deleted the docker sandbox. The local sandbox had no new work and purged without `--force`.

## Unproven requirements

T02 still lacks a running supervised agent, a successful shell-pane agent check, and a real gateway-refresh race check. T05 still lacks an authenticated agy session, a fixed-prompt response, catalog-model confirmation, subscription consumption, and vendor quota evidence. These failures are not SKIP or PASS.

## Cleanup

All four owned sandboxes were removed. Accounts `a14sub-cd`, `a14sub-cl`, `a14sub-gd`, and `a14sub-gl` and their providers were removed. The `a14sub` workspace and `a14sub-sentinel` role were removed. The four owned role and derived image tags were untagged. The temporary sentinel clone was deleted.

The original sentinel checkout stayed clean at `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`. Foreign accounts, providers, images, and sandboxes were not changed. The remaining docker sandboxes are `jackin-jackin-rqw3`, `jackin-jackin-3yuk`, and `jackin-arch-pr-dqq7`. The remaining local sandbox is `jackin-a14t02b-995e`. All are foreign to this run.

`crates/jackin-core/src/agent.rs` was restored with `git restore`. Before this evidence file, `git status --short` was empty.

## G-FAST

An earlier G-FAST process hit the 300-second tool timeout during nextest. That partial run was not a gate result. The complete rerun used the required command from the worktree root:

```text
mise exec -- mbx +1.99.1 xtask ci --fast
```

It exited 0 in 232.665 seconds. Its last lines were:

```text
Summary [ 205.859s] 2156 tests run: 2156 passed, 0 skipped
xtask: deny
xtask: shear
xtask: comments
xtask: layers
xtask: pins
xtask: containers
xtask: errors
xtask: baseline
xtask: screen
xtask: demo-gate
xtask: reuse
advisories ok, bans ok, licenses ok, sources ok
shear/summary
  ✓ no issues found
# SUMMARY
* Bad licenses: 0
* Deprecated licenses: 0
* Licenses without file extension: 0
* Missing licenses: 0
* Unused licenses: 0
* Used licenses: Apache-2.0, MIT
* Read errors: 0
* Invalid SPDX License Expressions: 0
* Files with copyright information: 1209 / 1209
* Files with license information: 1209 / 1209
Congratulations! Your project is compliant with version 3.3 of the REUSE Specification :-)
```
