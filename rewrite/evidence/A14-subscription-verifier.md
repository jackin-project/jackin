# A14 subscription independent verifier

Date: 2026-11-09. Verified commit: `20621e9e2869c93db30f37f5577f0896944e7947`. Worktree: `/tmp/jackin-a14-subscription-verify`. Branch: `wt/a14-subscription-verify`.

## Verdict

**PASS**. The evidence commit adds only `rewrite/evidence/A14-subscription-final.md`. Fresh runs on `jackin-local` reproduce the minimal T02 and T05 failures and support the report's decision to keep both tuples blocked. This PASS concerns the evidence, not catalog support for T02 or T05.

No evidence statement reviewed here promotes either tuple to PASS, treats Claude's JSON cost fields as an invoice, declares bare `gemini-4-argon` invalid, or claims a gateway refresh race. The original report also keeps vendor quota and subscription consumption unproven where no response occurred.

This verification did not repeat the `jackin-docker` runs, vendor quota checks, a refresh race, or the full detach, shell, export, and purge matrix. Those scopes remain unverified here rather than PASS. The local T02 attach and purge checks are stated separately below.

## Vehicle and isolation

`openshell --version` reported 0.1.7. `jackin doctor --gateway jackin-local` passed, including gateway health `serving` and pinned version 0.1.7. The proof build used only the four temporary T02/T05 launch-state changes in `crates/jackin-core/src/agent.rs`. `mise exec -- mbx +1.99.1 build -p jackin` exited 0.

The temporary role came from sentinel commit `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`. The verifier used role `a14v-sentinel`, workspace `a14v`, accounts `a14v-claude` and `a14v-agy`, and sandboxes `jackin-a14v-urhq` and `jackin-a14v-y9yo`. The two derived image IDs matched the original report:

```text
T02 sha256:0022bc2203ede2da89531e880caad557136a7db7b349846cdf955179130dc06c
T05 sha256:7b6da349475aa4f9d6aa126acc9345a59e4ec2d0d70d2acb639412918a85b834
```

An initial BuildKit attempt with `DOCKER_CONTEXT=orbstack` stopped at the role build before sandbox creation because the active builder required `default`. Both successful launches used `DOCKER_CONTEXT=default`. Docker BuildKit only built the images. OpenShell ran both sandboxes and every sandbox command.

The foreign local sandbox `jackin-a14t02b-995e` was not inspected or changed. No existing account or provider was edited or removed.

## T02 reproduction

The host Keychain item existed before and after the run. Host Claude answered `HOST-T02-BEFORE` and `HOST-T02-AFTER`. The new `a14v-claude` import created provider `jackin-a14v-claude-claude` with profile `claude` and one credential key.

The one-shot sandbox request used the catalog model and succeeded:

```text
claude -p "Reply with exactly: SANDBOX-T02-PONG" --model haiku --output-format json
result=SANDBOX-T02-PONG
is_error=false
canonicalModel=claude-haiku-5-5
provider=firstParty
```

The ordinary supervised launch failed. `/sandbox/.jackin/run/state` contained `exited 1 1`. The process list had `jackin-start` but no Claude child. A real PTY attach replayed this failure and detached with exit 0:

```text
Welcome to Claude Code v2.1.311
Unable to connect to Anthropic services
Failed to connect to platform.claude.com: ECONNREFUSED
jackin-start: claude exited with code 1 (attempt 1)
```

A direct request to `https://platform.claude.com/` exited 7 with `Could not connect to server`. This reproduces the reported distinction between a working one-shot API request and a failing supervised Claude launch.

The sandbox ran as uid 1000 with no `sudo`. `CLAUDE_CODE_OAUTH_TOKEN` was set. `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, and `OPENAI_API_KEY` were unset. The credentials file and settings file had mode 0600. The credentials file contained one literal placeholder.

An in-memory comparison decoded two host token fields and found zero host-token matches in the sandbox environment, credentials file, or `jackin inspect --json` output. No credential value entered the report or command output.

The verifier sandbox reported `loggedIn=true`, `authMethod=oauth_token`, and `apiProvider=firstParty` from `claude auth status --json`. It did not report a plan tier. A separate host status check reported the `max` subscription. The original report's sandbox `claude.ai` and plan-tier fields remain unverified by this run. This metadata difference does not change the reproduced supervised failure or the fact that the imported provider was the only credential source configured for the launch.

The T02 profile has no refresh configuration. No gateway refresh or refresh race was run or claimed. No vendor invoice or quota movement was measured. The successful response is consistent with use of the imported host subscription, and no paid fallback environment key was present, but this run does not add independent billing evidence.

Guarded purge refused the T02 sandbox because the temporary role edits arrived as uncommitted work. Export also refused that uncommitted work. The verifier then used `--force` only on its own `jackin-a14v-urhq` sandbox. The host-side temporary role retained those edits until cleanup. This verifier does not claim the original report's clean local export path.

## T05 reproduction

The host Keychain item existed before and after the run. Host agy answered `HOST-T05-BEFORE` and `HOST-T05-AFTER`. The new `a14v-agy` import created provider `jackin-a14v-agy-agy` with profile `agy` and one credential key.

The supervised process ran without desktop keyring facilities:

```text
PID  PPID  USER     COMMAND
372  1     sandbox  /bin/sh /usr/local/bin/jackin-start agy
678  372   sandbox  /usr/local/bin/agy
```

The sandbox ran as uid 1000. `dbus-daemon`, `busctl`, `secret-tool`, and `sudo` were absent. `/run/dbus` was absent, and `DBUS_SESSION_BUS_ADDRESS` was unset.

The fixed prompt used the bare catalog model and exited 1:

```text
agy -p "Reply with exactly: SANDBOX-T05-PONG" --model gemini-4-argon
Error: authentication required. Run 'agy' to log in, then retry.
error: authentication failed or timed out
```

Thus agy starts under `jackin-start` without a desktop keyring, but the gateway placeholder and provider profile do not authenticate it. No model response or subscription consumption occurred.

The host model list contained `gemini-4-argon-medium`, `gemini-4-argon-high`, and `gemini-4-argon-low`. It did not contain bare `gemini-4-argon`. Authentication failed before model selection, so the bare catalog model remains unproven, not invalid.

`GOOGLE_API_KEY` was set. `GEMINI_API_KEY`, `ANTHROPIC_API_KEY`, and `OPENAI_API_KEY` were unset. Inspect reported only `jackin-a14v-agy-agy` and zero policy-closure violations. After agy started, `settings.json` had mode 0600 and no literal placeholder.

The final in-memory comparison decoded the Keychain payload with the same prefix and base64url format as `credentials.rs`. It found three host token fields and zero matches in the sandbox environment, settings file, or inspect output. No credential value entered the report or command output.

Guarded purge deleted the clean T05 project sandbox without `--force`. No authenticated agy session, model response, vendor quota result, or billing result is claimed.

## Evidence and cleanup audit

The original report's central blocker claims are accurate against the fresh local runs. T02 has a working one-shot request but no running supervised Claude agent. T05 has a running supervised agy process but no authenticated session. Neither result supports a catalog flip.

The original report does not claim T05 subscription consumption. Its T02 cost fields are explicitly not invoices. This verifier adds no vendor billing or quota claim for either tuple. It also adds no refresh claim and no bare-model validity claim.

After cleanup, the local account, provider, role, and workspace lists contained no `a14v` or `a14sub` entries. The default Docker context contained zero image tags with either prefix. The original report's local cleanup state is consistent with these observations. Its historical docker cleanup was not rechecked.

All verifier sandboxes, accounts, providers, the workspace, the role, four owned image tags, and two temporary sentinel worktrees were removed. The original sentinel checkout stayed clean at `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`. The foreign `jackin-a14t02b-995e` sandbox remained present and `Ready`. Both host logins still answered after cleanup.

## G-FAST

The temporary catalog flips were restored before the gate. `git diff --exit-code -- crates/jackin-core/src/agent.rs` exited 0. The required command ran from the verifier worktree root without a command-time cap:

```text
mise exec -- mbx +1.99.1 xtask ci --fast
```

It exited 0 in 440.918 seconds. The final summaries were:

```text
Summary [ 262.696s] 2156 tests run: 2156 passed, 0 skipped
advisories ok, bans ok, licenses ok, sources ok
shear/summary
  ✓ no issues found
Files with copyright information: 1209 / 1209
Files with license information: 1209 / 1209
Congratulations! Your project is compliant with version 3.3 of the REUSE Specification :-)
```
