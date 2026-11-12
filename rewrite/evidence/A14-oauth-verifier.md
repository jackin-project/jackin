# A14 independent OAuth evidence verification

Date: 2026-11-09. Verified commit: `54485eb720dc93b205924ad6fcef6cb9ce73f8a6`. Worktree: `/tmp/jackin-a14-oauth-verify`. Branch: `wt/a14-oauth-verify`.

## Verdict

**PASS within the requested jackin-local scope.** An independent run reproduced the current T03 and T04 blocker conditions recorded in `rewrite/evidence/A14-oauth-final.md`. This verdict does not make either tuple supported. It does not reproduce the earlier `jackin-docker`, export, cache, detach, shell-persistence, Landlock, or second-sandbox claims.

The verified commit added only `rewrite/evidence/A14-oauth-final.md`. This verification adds only `rewrite/evidence/A14-oauth-verifier.md`. No product code, tests, decisions, plan, design, blocker, pull request, or push is part of the verdict.

## Vehicle and credential handling

OpenShell reported 0.1.7, matching `compatibility.lock.toml`. Host Grok reported 1.0.70. Host Muse reported 1.4.14. The sandbox Grok reported 1.0.77.

The run used an isolated `XDG_CONFIG_HOME` and the `a14ov` prefix for its account, role, workspace, launch, provider, and sandbox. The isolated OpenShell path referenced the existing mTLS registration. Host logins were the only agent credential sources. No `op` command or gateway-provider discovery supplied a credential.

The only temporary jackin source change set T03 and T04 to `TupleState::Supported` with no reason. A disposable sentinel worktree supplied the Grok instruction mapping. `mise exec -- mbx +1.99.1 build -p jackin` exited 0, and `jackin role check .` accepted the disposable role. The catalog was restored before G-FAST.

## T03 reproduction

At 2026-11-09T15:08:46Z, host `grok models` reported a `grok.com` login and default model `grok-4.7`. A host fixed prompt answered `HOST-PONG` before the launch. At 2026-11-09T15:20:08Z, while the owned sandbox existed, another host fixed prompt answered `HOST-PONG` with `stopReason=end_turn` and `modelUsage=grok-4.7-build`.

`jackin --gateway jackin-local account add grok --name a14ov-grok --from-host-login` exited 0 and created only `jackin-a14ov-grok-grok` for this account. The launch created `jackin-a14ov-w-4zoe`. The sandbox reported UID 1000, Grok 1.0.77, and supervisor state `running 1`. `/usr/local/bin/grok` was a child of `/bin/sh /usr/local/bin/jackin-start grok`.

A real PTY attach displayed `Do you trust the contents of this directory?` for `/sandbox/project`. This confirms that the supervised agent reached the trust prompt. It does not show that the agent reached the launch task.

The independent one-shot command was bounded inside the sandbox:

```text
timeout 120 grok -p "Answer exactly: PONG" --no-auto-update
```

It started at 2026-11-09T15:16:53Z, produced no stdout or stderr, and exited 124 after 120 seconds. The supervised Grok process and `running 1` state remained after the timeout and PTY attach.

The sandbox `grok models` command exited 0 and reported:

```text
You are using XAI_API_KEY.
Default model: grok-4.6
Available models: grok-4.6, grok-4.5
```

The rendered `/sandbox/.grok/config.toml` contained `default = "grok-4.7"`. This is a model-list and rendered-config mismatch only. The run does not identify a root cause, quota state, paid fallback, or billing attribution. Host CLI cost fields are not vendor invoices. No current vendor balance screen was obtained.

In-memory comparisons found zero host Grok session-key matches in the captured PTY, environment, `jackin inspect --json`, and model-list output. `XAI_API_KEY` held an `openshell:resolve:` broker reference. `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, and `META_API_KEY` were absent. The credential file checks found mode 0600 for both Grok files, a 30-byte `auth.json`, and one literal `placeholder` match. No credential value was printed or written to the evidence.

`openshell -g jackin-local provider refresh status jackin-a14ov-grok-grok` reported no refresh configuration. No second sandbox was launched. Refresh behavior, second-sandbox isolation, subscription consumption, and billing attribution remain unproven.

## T04 reproduction

At 2026-11-09T15:14:30Z, the current account command ran against `jackin-local`:

```text
jackin --gateway jackin-local account add muse --name a14ov-muse --from-host-login
```

It exited 1 after 30.09 seconds with `error: no credential is available for account a14ov-muse`. The current code gives the interactive Keychain read a 30-second timeout in `sync_muse_login`. An existence-only check for Keychain service `ai.meta.dev.credentials`, account `meta`, succeeded. That check printed no secret value.

The isolated account list remained empty. No `a14ov-muse` provider or sandbox appeared. The pre-existing `jackin-muse-live-muse` provider was not used or changed. No T04 launch was attempted after the failed fresh import. Host Muse answered `HOST-PONG` before the import and again at 2026-11-09T15:15:14Z after the failure.

The failed import supplied no Muse credential for a token comparison. Its output contained only the account name and the missing-credential error. This run confirms the fresh-import blocker. It does not reuse the stale gateway token, re-run the Landlock diagnosis, or prove any T04 sandbox requirement.

## Blocker corrections and scope limits

`rewrite/blockers/T03-grok.md` still names HTTP 402 quota exhaustion as the reason. Two independent host responses in this run contradict that stale reason. They do not prove a sandbox response or a billing state. The current reproduced blockers are the directory-trust stop and the bounded one-shot timeout.

`rewrite/blockers/T04-muse.md` still names the earlier Agent Definition filesystem error as the reason. The current account flow fails earlier at the interactive Keychain read. The earlier Landlock evidence remains historical and was not re-certified here.

The final evidence's current local T03 and T04 claims need no correction from this run. Its other gateway and lifecycle claims are outside this verification. Neither catalog tuple should flip from this evidence.

## Cleanup

The owned sandbox was purged. The owned account and provider were removed. The isolated role and workspace were removed. The disposable sentinel worktree and isolated configuration were deleted. The original sentinel checkout remained clean. No `a14ov` provider or sandbox remained. The foreign `jackin-a14t02b-995e` sandbox and all foreign providers were left unchanged.

The local image store retains `jackin.invalid/a14ov-sentinel:55ef880cac10c8e5`, `jackin.invalid/role/a14ov-sentinel:55ef880cac10c8e5`, and `jackin.invalid/construct:514892f751c0b402`. Jackin exposes no owned image-cleanup command. No manual Docker command bypassed the `jackin-image` boundary.

## G-FAST

`git diff --exit-code -- crates/jackin-core/src/agent.rs` exited 0 before the gate. The exact command ran from the worktree root:

```text
mise exec -- mbx +1.99.1 xtask ci --fast
```

It exited 0 in 554.004 seconds. The final summaries were:

```text
Summary [ 373.511s] 2156 tests run: 2156 passed, 0 skipped
advisories ok, bans ok, licenses ok, sources ok
shear/summary
  ✓ no issues found
Files with copyright information: 1211 / 1211
Files with license information: 1211 / 1211
Congratulations! Your project is compliant with version 3.3 of the REUSE Specification :-)
```
