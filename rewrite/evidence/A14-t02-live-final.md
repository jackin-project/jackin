# A14 final T02 live proof through generic OAuth refresh

Date: 2026-11-10. Review commit: `94d880efefe94cc9915c9369d15a8cb6a3507394`. Worktree: `/tmp/jackin-a14-t02-final-oauth`. Branch: `wt/a14-t02-final-oauth`.

## Verdict

**FAIL**. The generic `OAuth2RefreshToken` path fixed the older static-token failure in the ordinary host-then-sandbox sequence. Gateway refresh and post-refresh inference also passed. The same explicit gateway refresh then revoked the host-side Claude session. Unmodified host Claude reported `loggedIn=false` and could not answer after the docker run or after the local import attempt.

The local import failed because no host login remained. No local sandbox ran. T02 must stay `BlockedVendor`. No SKIP result is counted as PASS.

## Vehicle and isolation

The only temporary product change set T02 from `TupleState::BlockedVendor` to `Supported` and changed its reason to `None` in `crates/jackin-core/src/agent.rs`. `AuthClass::BlockedVendor` remained unchanged. `mise exec -- mbx +1.99.1 build -p jackin` exited 0 in 50.85 seconds. The proof binary was `/tmp/jackin-a14-t02-final-oauth/target/debug/jackin`.

The source change was restored after the live runs. `git diff --exit-code -- crates/jackin-core/src/agent.rs` exited 0. Nothing was committed or pushed. G-FAST was not run, as ordered.

OpenShell reported 0.1.7. Both gateway doctor checks passed with health `serving` and pinned version 0.1.7. `jackin-docker` used the OrbStack builder on linux/arm64 with `DOCKER_CONTEXT=orbstack`. Docker BuildKit only built images. OpenShell created the sandbox and executed every sandbox command. No Docker, Podman, or Apple container command ran an agent, a sandbox, or a shell.

The isolated `XDG_CONFIG_HOME` was `/tmp/a14t02o-xdg`. Its OpenShell configuration named the existing mTLS gateways through links. The host jackin configuration was not changed. The disposable sentinel clone was `/tmp/a14t02o-sentinel` at `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`. Its only source edit named the role `a14t02o-sentinel`. Docker stayed disabled. `jackin role check` exited 0.

The docker run used workspace `a14t02o`, account `a14t02o-cd`, provider `jackin-a14t02o-cd-claude`, sandbox `jackin-a14t02o-0ap5`, and launch ID `a14t02odockerproof0000001`. The reserved local account name was `a14t02o-cl`. All names were unique to this run. Host Claude was the only credential source. No Anthropic API key, `op` reference, existing provider, or paid fallback supplied a credential.

## Host subscription before the runs

Unmodified host Claude 2.1.319 answered `A14T02O-HOST-BEFORE` with `is_error=false`. `modelUsage` reported canonical model `claude-haiku-5-5` and provider `firstParty`.

Before the imports, `claude auth status --json` reported `loggedIn=true`, `authMethod=claude.ai`, `subscriptionType=max`, `planTier=max_20x`, `storedClaudeAiLogin=true`, and `apiProvider=firstParty`. This is the subscription account used for the proof.

## Docker import and refresh metadata

The import used the current host-login command:

```text
jackin --gateway jackin-docker account add claude --name a14t02o-cd --agent claude --from-host-login
```

The provider used profile `claude` and credential key `CLAUDE_CODE_OAUTH_TOKEN`. `openshell provider refresh status` reported strategy `oauth2_refresh_token` and status `configured`. It showed an expiry and a next-refresh time. This proves that the new generic refresh path, not the older static-token path, held the imported grant.

## Image build and cached rebuild: PASS

The initial launch built role tag `jackin.invalid/role/a14t02o-sentinel:906a81ed10e57ccd` and derived tag `jackin.invalid/a14t02o-sentinel:906a81ed10e57ccd`. The derived image ID was `sha256:0022bc2203ede2da89531e880caad557136a7db7b349846cdf955179130dc06c`. The image installed Claude 2.1.311 from the checksum-pinned catalog artifact.

A relaunch with the same launch ID printed:

```text
image construct: cached
image derived: cached
image: skipped, the image is cached
provider: skipped, the provider exists
sandbox: skipped, the sandbox exists
upload: skipped, the project is uploaded
supervisor: skipped, Codex is running
hooks: skipped, Codex is running
start: skipped, Codex is running
```

The existing `Codex` wording came from the product output. The running agent was Claude.

## Sandbox user and supervised process: PASS

The sandbox reported `uid=1000(sandbox) gid=1000(sandbox) groups=1000(sandbox)` and `sudo=absent`. `/sandbox/.jackin/run/state` contained `running 1`.

The process check showed:

```text
PID  PPID  USER     COMMAND
64   1     sandbox  /bin/sh /usr/local/bin/jackin-start claude
373  64    sandbox  /usr/local/bin/claude
```

The same parent relationship remained after the host revocation and after a shell session. The interactive CLI stayed at its first-run theme screen. The process and replay checks do not claim that the supervised launch task completed.

## Exact model replies and real tool: PASS

Every one-shot request selected the configured `haiku` model. Each successful response reported canonical model `claude-haiku-5-5` and provider `firstParty`.

The initial sandbox request returned `A14T02O-DOCKER-INITIAL` with `is_error=false`. Host Claude then returned `A14T02O-HOST-AFTER-DOCKER-INITIAL`. The next sandbox request returned `A14T02O-DOCKER-AFTER-HOST` with `is_error=false`. This ordinary host-then-sandbox sequence did not reproduce the older `OAuth token revoked` failure.

A streaming request used the Bash tool exactly once. The captured tool result was `A14T02O-TOOL-OUTPUT`, and the final response was `A14T02O-DOCKER-TOOL` with `is_error=false`. This was a real tool call in the OpenShell sandbox.

## Placeholders and disclosure checks: PASS for captured state

An in-memory check parsed the host access and refresh fields from the `Claude Code-credentials` Keychain payload. It compared them with sandbox environment output, both Claude files, auth status, inspect output, captured stderr, and the proof artifacts. It printed only counts and booleans.

The docker check reported:

```text
host-token-fields=2
captured-host-credential-matches=0
artifact-host-credential-matches=0
expected-env-set=True
expected-env-openshell-reference=True
expected-env-literal-placeholder=False
expected-env-host-credential-match=False
credentials-host-credential-matches=0
```

`CLAUDE_CODE_OAUTH_TOKEN` held an OpenShell broker reference. `/sandbox/.claude/.credentials.json` had `claudeAiOauth.accessToken` set to the matching `openshell:resolve:env:CLAUDE_CODE_OAUTH_TOKEN` reference. Its `refreshToken` was empty, `expiresAt` was `4102444800000`, and its scopes were `user:profile` and `user:inference`. `/sandbox/.claude/settings.json` was `{}`. Both files had mode 0600 and owner `sandbox:sandbox`.

`ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `OPENAI_API_KEY`, and `ANTHROPIC_BASE_URL` were absent. Inspect listed only `jackin-a14t02o-cd-claude` and reported `closure_violations=[]`.

## Provider egress and non-agent denial: PASS for exercised paths

Successful Claude responses exercised `api.anthropic.com`. The successful gateway rotation exercised `platform.claude.com`. The attached profile contained only those two provider hosts.

Direct `curl` probes from the sandbox returned exit 7 and HTTP `000` for `https://api.anthropic.com/`, `https://platform.claude.com/`, and `https://example.com/`. The provider policy authorizes Claude, not arbitrary `curl` traffic. The non-provider destination was refused.

## Refresh and host-safety sequence: FAIL

No grants were rotated concurrently. The observed sequence was sequential:

1. The initial sandbox and host responses passed, followed by another passing sandbox response.
2. `openshell -g jackin-docker provider refresh rotate jackin-a14t02o-cd-claude --credential-key CLAUDE_CODE_OAUTH_TOKEN` reported `Rotation requested ... (refreshed)`.
3. Refresh status reported `refreshed` and `LAST_REFRESH=2026-11-10 06:16:34`.
4. The sandbox returned `A14T02O-DOCKER-POST-REFRESH` with `is_error=false` and canonical model `claude-haiku-5-5`.
5. The next unmodified host request exited 1 with this exact sanitized result:

```text
Failed to authenticate: OAuth session expired and could not be refreshed
```

6. Host `claude auth status --json` then reported `loggedIn=false`, `authMethod=none`, and `loginExpiresAt=null`. The host subscription fields were absent.
7. The gateway credential still worked. A later sandbox request returned `A14T02O-DOCKER-AFTER-HOST-REVOCATION` with `is_error=false`.
8. After docker cleanup, the host request for `A14T02O-HOST-AFTER-DOCKER-CLEANUP` exited 1 with the same exact error. The `Claude Code-credentials` Keychain item still existed, but the host login remained unusable.

The explicit gateway rotation therefore passed on the gateway and failed the host-safety gate. No refresh race is needed to reproduce the logout. The run does not claim a vendor-internal cause beyond this observed sequence and exact error.

## Detach, reattach, and replay: PASS for the observed screen

Two real PTY attach sessions detached with exit 0. Both rendered `Welcome to Claude Code v2.1.311` and the same first-run theme screen. The second attach replayed that observed screen. No supervised-task response marker is claimed because the CLI remained at onboarding.

## Shell persistence: PASS

A real PTY shell session exited with code 0. The following process check still showed Claude PID 373 as a direct child of `jackin-start` PID 64. Opening and closing the shell did not stop the agent.

## Export, unchanged host state, and guarded purge: PASS

A sandbox-only commit created commit `146ce1f361e283959f9f9983bc229823073d3858`. Guarded purge first refused with exit 3:

```text
error: the work in sandbox jackin-a14t02o-0ap5 is not exported (uncommitted changes, detached HEAD 146ce1f361e283959f9f9983bc229823073d3858 not exported); run jackin export jackin-a14t02o-0ap5 or pass --force
```

Export published exactly:

```text
refs/heads/jackin/jackin-a14t02o-0ap5/HEAD  146ce1f361e283959f9f9983bc229823073d3858
```

The temporary host HEAD stayed `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`. Its diff SHA256 stayed `99006ff04069db6ff89027d9549861ca9211c081663ae89796aaa8373d37261c` before and after export. After the exported sandbox checkout was made clean, guarded purge deleted the sandbox without `--force`. The exported ref was removed during temporary-clone cleanup.

The original sentinel checkout stayed clean at `6722f8024623c76726ce0293fc7e93da47b5ee74`. Export did not change it.

## Subscription use and paid fallback

The docker responses used the only attached provider, the imported host Claude subscription, and `firstParty` model attribution. The host account was `max_20x` before import. No API key, alternate base URL, or other paid provider existed in the sandbox. This proves subscription-only use for the successful docker responses and no paid fallback in the tuple.

Sandbox `claude auth status --json` reported `loggedIn=true`, `authMethod=oauth_token`, and `apiProvider=firstParty`. It did not report a plan tier. JSON cost fields are not invoices. Vendor quota movement and invoice attribution were not measured.

## Local-second result: NOT RUN

After docker cleanup, the local import command ran:

```text
jackin --gateway jackin-local account add claude --name a14t02o-cl --agent claude --from-host-login
```

It exited 2 with:

```text
error: no claude host login is available
```

The isolated account list remained empty. No `a14t02o-cl` account, provider, or sandbox was created. A following host request for `A14T02O-HOST-AFTER-LOCAL-ATTEMPT` exited 1 with `Failed to authenticate: OAuth session expired and could not be refreshed`.

The local image, process, response, placeholder, egress, replay, shell, refresh, export, and purge checks were NOT RUN. They are not PASS.

## Cleanup

The owned docker sandbox and account were removed. Provider `jackin-a14t02o-cd-claude` was absent after account removal. The owned workspace and role were removed. No local account, provider, or sandbox required removal. Foreign providers and sandboxes were left unchanged.

The temporary exported ref, sentinel clone, isolated configuration, probe scripts, and captured output were removed. The original sentinel checkout and the host jackin configuration were not changed.

The owned role and derived image tags remain in the OrbStack builder. Jackin has no image-removal command in this review tree. No direct Docker removal command bypassed the `jackin-image` boundary. The shared construct image was not touched.

The host Claude login remains logged out. Cleanup did not restore or replace it. A new host login is required before another live proof.

## Remaining blockers

- Explicit gateway refresh makes the host subscription unusable. T02 fails the required host-safety gate.
- The local-second launch and all dependent local checks remain unproven because the host login was gone.
- A future proof needs a fresh host login and must repeat both gateway runs sequentially.
- Vendor quota movement and invoice attribution remain unmeasured.
