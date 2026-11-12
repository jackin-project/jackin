# A14 T02 live proof after the platform policy correction

Date: 2026-11-09. Review base: `28f7e0751b71b5068103415bdfd53ff61c2c969b`. Product correction: `8f4a4bfd31db1f2fd98d5dfc8c23d73ab3d3743c`. Worktree: `/tmp/jackin-a14-t02-live-fixed`. Branch: `wt/a14-t02-live-fixed`.

## Verdict

**FAIL**. The `platform.claude.com` correction fixes the previously recorded supervised-start failure. Claude stayed running under `jackin-start`, and a fixed one-shot prompt answered with the catalog model. The complete live requirements still do not pass.

A new live failure condition stopped the proof. After host Claude answered, the next sandbox request failed with HTTP 401 and `OAuth token revoked`. No product patch or retry followed. The `jackin-local` re-check, export, and some terminal checks were not completed. T02 must not flip to Supported on this evidence.

## Vehicle and isolation

The only temporary product change was the T02 tuple state in `crates/jackin-core/src/agent.rs`, from `BlockedVendor` to `Supported`, with the reason changed to `None`. `AuthClass` remained unchanged. The change was reverted before G-FAST. `mise exec -- mbx +1.99.1 build -p jackin` exited 0.

The temporary sentinel clone came from `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`. Its only change renamed the role to `a14t02f-sentinel`. Docker stayed disabled. `jackin role check /tmp/a14t02f-sentinel` exited 0.

The run used workspace `a14t02f`, account `a14t02f-cd`, provider `jackin-a14t02f-cd-claude`, sandbox `jackin-a14t02f-2qa7`, and launch ID `jp3ohskbip2yylq5vnyantaaid`. A temporary `XDG_CONFIG_HOME` held only this lane's jackin configuration and non-secret OpenShell gateway metadata. Read-only links named the existing gateway mTLS directories so OpenShell could authenticate. No credential value was printed, copied into a file, or placed in argv.

Both gateway doctor checks passed before launch. Each reported OpenShell 0.1.7, gateway health `serving`, and pinned version 0.1.7. `openshell --version` reported 0.1.7. `jackin-docker` used the OrbStack builder on linux/arm64 with `DOCKER_CONTEXT=orbstack`. Docker BuildKit only built images. OpenShell ran the sandbox and every sandbox command.

## Requirement results

### Image build and cache: PASS

The initial docker launch printed `image role: built` and `image derived: built`. The role tag was `jackin.invalid/role/a14t02f-sentinel:cc80ec0e6ff6db64`. The derived tag was `jackin.invalid/a14t02f-sentinel:cc80ec0e6ff6db64`, with image ID `sha256:0022bc2203ede2da89531e880caad557136a7db7b349846cdf955179130dc06c`. The derived image installed Claude 2.1.311 from the catalog artifact.

A relaunch with the same launch ID printed:

```text
image construct: cached
image derived: cached
image: skipped, the image is cached
sandbox: skipped, the sandbox exists
upload: skipped, the project is uploaded
supervisor: skipped, Codex is running
hooks: skipped, Codex is running
start: skipped, Codex is running
```

The word `Codex` in these supervisor messages is the product's existing output. This launch ran Claude.

### Sandbox user and supervised agent: PASS before the stop condition

The sandbox reported `uid=1000(sandbox) gid=1000(sandbox) groups=1000(sandbox)` and `sudo=absent`. The process list showed:

```text
PID  PPID  USER     COMMAND
64   1     sandbox  /bin/sh /usr/local/bin/jackin-start claude
372  64    sandbox  /usr/local/bin/claude
```

`/sandbox/.jackin/run/state` contained `running 1`. This closes the earlier condition where Claude exited after `platform.claude.com` was refused.

The fixed docker prompt was:

```text
claude -p "Reply with exactly: A14T02F-DOCKER-PONG" --model haiku --output-format json
```

It returned `result=A14T02F-DOCKER-PONG`, `is_error=false`, and canonical model `claude-haiku-5-5`. `modelUsage` named provider `firstParty`. The response's JSON cost field is not treated as an invoice.

### Placeholders and host-key exclusion: PASS for captured state

An in-memory check parsed two host token fields from the `Claude Code-credentials` Keychain payload. It compared those values with captured sandbox environment output, both Claude auth files, `jackin inspect --json` stdout, and all captured command stderr. It printed only counts and booleans:

```text
host-token-fields=2
environment-host-credential-matches=0
auth-files-host-credential-matches=0
inspect-host-credential-matches=0
stderr-host-credential-matches=0
expected-env-set=True
expected-env-literal-placeholder=False
expected-env-host-credential-match=False
credentials-literal-placeholder-count=1
ANTHROPIC_API_KEY-set=False
ANTHROPIC_AUTH_TOKEN-set=False
OPENAI_API_KEY-set=False
ANTHROPIC_BASE_URL-set=False
```

`CLAUDE_CODE_OAUTH_TOKEN` held an OpenShell broker reference, not the literal word `placeholder` and not a host token. `/sandbox/.claude/.credentials.json` contained one literal placeholder. Both that file and `/sandbox/.claude/settings.json` had mode 0600 and were owned by `sandbox:sandbox`.

### Provider-only egress and policy closure: PASS for the exercised paths

`jackin inspect --json` listed only `jackin-a14t02f-cd-claude`, reported `closure_violations=[]`, and showed the supervisor running. The successful API response exercised `api.anthropic.com`. Supervised Claude stayed running with the corrected profile instead of hitting the prior `platform.claude.com` refusal.

A direct `curl` probe from the sandbox returned connection exit 7 and HTTP `000` for `https://api.anthropic.com/`, `https://platform.claude.com/`, and `https://example.com/`. The provider profile authorizes Claude, not arbitrary `curl` traffic. Thus the new platform rule did not open general sandbox egress, and the non-provider destination was refused. Positive platform access is attributed to the supervised Claude process, not to the refused `curl` probe.

### Detach, reattach, and replay: NOT PROVEN

Two ordinary PTY attach sessions detached with exit 0. A later supervised-prompt probe also detached with exit 0, but it observed neither the response marker nor a reliable replay marker. A separate attach-inspection probe hung and was terminated before cleanup. These observations prove detach, not replay. No reattach replay PASS is claimed.

### Shell pane leaves Claude running: NOT PROVEN

A real PTY shell session exited with code 0. The later 401 stop condition occurred before a direct post-shell process-tree check. The shell exit alone does not prove that Claude remained running, so this requirement remains unproven.

### Export and host tree: NOT RUN

The stop condition occurred before an export. No export ref or successful export is claimed. The original sentinel checkout remained clean at `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`. The temporary clone retained only its intentional role-name change until deletion.

### Guarded purge refusal: PASS

Guarded purge failed with exit 3:

```text
error: the work in sandbox jackin-a14t02f-2qa7 is not exported (uncommitted changes); run jackin export jackin-a14t02f-2qa7 or pass --force
```

Only after that refusal did cleanup use `--force` on this run's own sandbox.

### Subscription payment and no paid fallback: PASS for the first response, later revoked

The successful first response used the only attached OpenShell provider, the imported host Claude login, and `firstParty` model attribution. No Anthropic API key, Anthropic auth token, OpenAI API key, alternate base URL, or other paid provider was configured in the sandbox. `claude auth status --json` reported `loggedIn=true`, `authMethod=oauth_token`, and `apiProvider=firstParty`. It did not report a plan tier in this run.

This proves that the first response consumed the imported subscription credential without a paid fallback in the tuple. It is not vendor quota or invoice evidence. The later 401 means credential continuity failed after the host check.

## Exact stop condition and host-after result

The observed sequence was:

1. The initial docker sandbox request returned `A14T02F-DOCKER-PONG` with the catalog model.
2. At `2026-11-09T15:21:10Z`, unmodified host Claude answered `A14T02F-HOST-AFTER-DOCKER` while the sandbox was present.
3. The next sandbox request, using the same catalog model and output shape, exited 1. Its JSON contained `terminal_reason=api_error`, `api_error_status=401`, `is_error=true`, and this exact sanitized result:

```text
Failed to authenticate: OAuth token revoked. Please log in again or contact your administrator.
```

4. Host Claude later answered `A14T02F-HOST-FINAL`. After sandbox cleanup it answered `A14T02F-HOST-AFTER-CLEANUP`. Existence-only checks found the `Claude Code-credentials` Keychain item after the sandbox failure and after cleanup.
5. `openshell -g jackin-docker provider refresh status jackin-a14t02f-cd-claude` returned:

```text
No refresh configurations found for provider 'jackin-a14t02f-cd-claude'.
```

The profile has no refresh configuration, and this run issued no gateway refresh command. The evidence does not establish a gateway refresh race or the vendor-side reason for revocation. The host-after check proves that the host login still answered after sandbox use and after the sandbox 401, while the imported sandbox credential was rejected in the observed host-then-sandbox sequence.

## jackin-local

**NOT RUN**. The new 401 condition stopped the proof before the local account import or sandbox launch. No local T02 result is claimed. The pre-existing foreign local sandbox `jackin-a14t02b-995e` was not inspected or changed.

## Cleanup

Guarded purge refused the docker sandbox as recorded above. Forced purge then deleted only `jackin-a14t02f-2qa7`. Account removal deleted `a14t02f-cd` and its provider. The isolated account, workspace, and role lists were empty after removal. The temporary sentinel clone, temporary XDG configuration, and probe scripts were deleted.

The remaining docker sandboxes were `jackin-jackin-rqw3`, `jackin-jackin-3yuk`, and `jackin-arch-pr-dqq7`. The remaining local sandbox was `jackin-a14t02b-995e`. All are foreign to this run and were left untouched. The remaining docker providers were `jackin-jackin-docker-validation-codex` and `jackin-codex-local-codex`; neither belongs to this run.

The two owned image tags remain on the OrbStack builder. Jackin has no image-removal command in this review tree, and the repository boundary reserves direct Docker process starts to `jackin-image`. No direct Docker removal command was run. The shared construct image was not touched.

The original sentinel checkout stayed clean at its starting HEAD. The host Claude login and Keychain item remained usable after cleanup.

## Remaining unproven requirements

- The `jackin-local` re-check did not run.
- Reattach replay did not produce a passing marker.
- A direct post-shell Claude process check did not run.
- Export to `refs/heads/jackin/<sandbox>/<branch>` did not run.
- Credential continuity after the host check failed; the revocation cause and any recovery path are unknown.
- Vendor quota movement and invoice attribution were not measured.
- No gateway refresh race was run or claimed because the profile has no refresh configuration.

## G-FAST

The temporary T02 state flip was restored before the gate. `git diff --exit-code -- crates/jackin-core/src/agent.rs` exited 0. The required command ran from the worktree root:

```text
mise exec -- mbx +1.99.1 xtask ci --fast
```

It exited 0 in 568.315 seconds. Nextest reported:

```text
Summary [ 312.628s] 2158 tests run: 2158 passed, 0 skipped
```

The `fmt`, `clippy`, `nextest`, `deny`, `shear`, `comments`, `layers`, `pins`, `containers`, `errors`, `baseline`, `screen`, `demo-gate`, and `reuse` steps passed. REUSE reported 1212 files with copyright information and 1212 files with license information. The fast gate skipped only the docs build as designed.
