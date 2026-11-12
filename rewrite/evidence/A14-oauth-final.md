# A14 final live proof for T03 and T04

Date: 2026-11-09. Base HEAD: `0a4b4c6bb9c30afaf7e4629e1854ed706be973e7`. Worktree: `/tmp/jackin-a14-oauth-final`. Branch: `wt/a14-oauth-final`.

## Verdicts

- T03, Grok Build with xAI: **FAIL**. Host Grok 1.0.70 answered fixed prompts. The pinned sandbox Grok 1.0.77 did not answer on either gateway. Subscription billing attribution is also unproven. The independent image, cache, hygiene, terminal, export, and purge checks do not remove those blockers.
- T04, Muse Code with a Meta subscription: **FAIL**. Host Muse 1.4.14 answered `HOST-PONG`. Fresh imports through the current account flow failed on both gateways because the interactive Keychain secret read did not complete. No new provider or sandbox was created. The earlier Landlock evidence remains valid history, but this run adds no T04 sandbox pass.
- Neither result supports a catalog flip. The temporary T03 and T04 launch-state changes were reverted before G-FAST.

## Vehicle and isolation

OpenShell reported version 0.1.7. The primary gateway was `jackin-docker`, followed by `jackin-local`. The runs were sequential. No Docker, Podman, or Apple container command ran an agent, a sandbox, or a shell.

The only product-code diff during the proof was the temporary T03 and T04 change from `TupleState::Unavailable` to `Supported`, with the reasons changed to `None`. `mise exec -- mbx +1.99.1 build -p jackin` passed. The proof binary was `/tmp/jackin-a14-oauth-final/target/debug/jackin`.

The temporary sentinel clone started at `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`. Its uncommitted edits named the role `a14of-sentinel`, added the Grok and Muse instruction mappings, and added `/` to `read_only`. `jackin role check` passed. The landed launch planner also adds read-only `/` for Muse in `crates/jackin-session/src/plan.rs`. No edit weakened that rule or D10.

An isolated temporary `XDG_CONFIG_HOME` held the jackin configuration. Its OpenShell registry path referenced the existing registry so both mTLS gateways remained available. Host logins were the only credential sources. No `op` command or gateway-provider discovery supplied a credential. Account names, role names, workspaces, providers, and sandboxes owned by this run use the `a14of` prefix. Foreign resources were not changed.

## T03 live evidence

### Host behavior and imports

At 2026-11-09T14:20:58Z, `grok models` reported a valid `grok.com` login with default model `grok-4.7`. The next host request used this command:

```text
grok -p "Reply with exactly: PONG" --no-auto-update --output-format json
```

It exited 0 and returned `text=PONG`, `stopReason=end_turn`, one model call, and `modelUsage=grok-4.7-build`. This is a current response, not the old empty-result behavior and not a claim that the old HTTP 402 persists. A later host request returned `HOST-PONG` while the docker sandbox was present.

The current host-login imports succeeded separately on each gateway:

```text
jackin --gateway jackin-docker account add grok --name a14of-gd --from-host-login
jackin --gateway jackin-local account add grok --name a14of-gl --from-host-login
```

They created `jackin-a14of-gd-grok` and `jackin-a14of-gl-grok`, each with profile `grok` and one credential key. `openshell -g jackin-docker provider refresh status jackin-a14of-gd-grok` reported no refresh configurations. No gateway refresh or refresh race is claimed.

### Image build and cache

The docker launch created `jackin-a14of-6zxe` with launch ID `a14oft03dockerproof0000001`. It installed the pinned Grok 1.0.77 artifact with SHA256 `b30eee14a656124ef3c9da8577b5e783ef2cffabeb2b679c5c49633e77eed574`. The derived image was `sha256:fed94351c177b99181623267e6c66c68a355983b97b2f5a068f3607e3453d9e3`.

A relaunch with the same launch ID printed:

```text
image construct: cached
image derived: cached
image: skipped, the image is cached
sandbox: skipped, the sandbox exists
upload: skipped, the project is uploaded
```

The local launch created `jackin-a14of-1s4x` and also used the cached construct and derived images.

### Sandbox process and fixed prompts

Both sandboxes reported `grok 1.0.77 (50ca579fa996)`, `uid=1000(sandbox)`, and no `sudo`. On docker, `/usr/local/bin/grok` had PPID 64, where PID 64 was `/bin/sh /usr/local/bin/jackin-start grok`. The local process had the same parent relationship. The supervisor reported `running 1` on both gateways.

The fixed docker request used the catalog default and produced no stdout or stderr for more than ten minutes. The exec client was then stopped and only the one-shot Grok child was terminated. The supervised Grok process remained running. The local equivalent was bounded by `timeout 120` and ended with `fixed-prompt-exit=124`, again with no stdout or stderr. Neither result is a response, and neither is evidence of the old 402.

The sandbox `grok models` command exited 0 and reported:

```text
You are using XAI_API_KEY.
Default model: grok-4.6
Available models: grok-4.6, grok-4.5
```

The rendered `/sandbox/.grok/config.toml` instead set `default = "grok-4.7"`. The cause of the fixed-prompt hang is unproven. The model mismatch is a diagnostic fact, not a root-cause claim.

The supervised docker agent displayed `Do you trust the contents of this directory?` for `/sandbox/project`. It had not reached the launch task. This is a product defect for a separately reviewed code unit. No product code was patched in this proof.

### Placeholders and egress

On both gateways, an in-memory comparison found zero host-session-key matches in the captured environment and `jackin inspect --json` output. `XAI_API_KEY` was set to an `openshell:resolve:` broker reference and did not match the host session key. `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, and `META_API_KEY` were absent on docker.

`/sandbox/.grok/auth.json` had mode 0600, size 30 bytes, and one literal `placeholder` match. `/sandbox/.grok/config.toml` had mode 0600. The file checks covered these rendered files, not every file in the sandbox. No credential value appeared in the captured command output or evidence.

Inspect listed only the matching T03 provider and zero policy-closure violations on each gateway. Requests from the sandbox to `https://example.com` and `https://api.meta.ai` failed with curl exit 7 and HTTP `000`. The successful sandbox `grok models` request provides the positive provider-path observation. No other provider was attached.

### Detach, reattach, and shell persistence

Two real docker PTY attaches replayed the Grok trust screen and detached with exit 0. This proves replay of the observed screen, not a working authenticated session.

A shell pane exited 0. The following process check still showed Grok PID 372 as a child of `jackin-start` PID 64. Opening and closing the shell pane did not stop the agent.

### Export and guarded purge

A sandbox-only commit created an unexported branch tip. Guarded purge refused with exit 3:

```text
error: the work in sandbox jackin-a14of-6zxe is not exported (branch feature/openshell-native not exported); run jackin export jackin-a14of-6zxe or pass --force
```

Export published only this new ref in the temporary sentinel clone:

```text
refs/heads/jackin/jackin-a14of-6zxe/feature/openshell-native  9ddabc0b5abda0c8fbd3e18ae0c47634873fc2cd
```

The host HEAD remained `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`. The host diff hash remained `fbcd997a121635de81a9e30196ac3c1d69a20beb61421d857dbbcc7acabcc84e`. Guarded purge then deleted the sandbox without `--force`.

Local guarded purge refused the uncommitted role edits with exit 3. Only that owned local sandbox was then purged with `--force`.

### Billing and unrun checks

The host JSON responses included `total_cost_usd` values. Those CLI fields are not invoices and do not establish which balance paid. The run did not obtain a current vendor Auto Top-up or prepaid-balance screen. `rewrite/research/agent-provider.md` records an earlier operator confirmation of Auto Top-up off and prepaid balance zero, but that history is not a fresh observation from this run.

No sandbox fixed prompt returned a response. Therefore subscription consumption, model billing attribution, and the absence of paid fallback remain unproven. Second-sandbox isolation was not run because the required sandbox response did not occur. Host concurrency passed only for the host login remaining usable while a sandbox existed. Refresh and second-sandbox isolation are not PASS.

## T04 live evidence

### Host login and failed fresh imports

At 2026-11-09T14:43:49Z, unmodified host Muse 1.4.14 answered `HOST-PONG`. The host metadata reported `providers.meta.storage=keychain` and `providers.meta.subs_active=True`. An existence-only check for Keychain service `ai.meta.dev.credentials`, account `meta`, exited 0. No secret value was read or printed by these checks.

The current account flow was attempted first on docker and then on local:

```text
jackin --gateway jackin-docker account add muse --name a14of-md --from-host-login
jackin --gateway jackin-local account add muse --name a14of-ml --from-host-login
```

Each command took about 30 seconds and exited 1 with:

```text
error: no credential is available for account a14of-md
error: no credential is available for account a14of-ml
```

The implementation gives the Keychain secret read a 30-second timeout in `sync_muse_login`. The interactive approval did not complete. Neither command created a provider. The host answered `HOST-PONG` again after both failures, with the same non-secret metadata.

The existing `jackin-muse-live-muse` provider on `jackin-local` is foreign and stale-history state. This proof did not attach, refresh, replace, or delete it. No sandbox launched from the stale token.

### Unproven requirements

The fresh import failure stopped both launches before image build. This run therefore proves none of the T04 sandbox requirements: image build and cache, uid and no sudo, child-of-`jackin-start`, fixed prompt, placeholders, provider-only egress, detach and replay, shell persistence, export, guarded purge, refresh and host concurrency, or subscription payment. These are unproven, not SKIP or PASS.

The temporary role check accepted read-only `/`. The prior causal Landlock proof remains in `A14-muse-strace.md`, and the prior live transition past the Agent Definition error remains in `A14-t21-t04.md`. This run does not downgrade or re-certify those results. The current blocker is the inability to import a fresh host Keychain credential.

## Remaining defects and blockers

- T03 fixed one-shot prompts hang on both gateways. The supervised agent also waits at a directory-trust prompt. These behaviors need a separately reviewed product-code unit before another support disposition.
- T03 has no proven subscription billing attribution, no gateway refresh configuration, and no second-sandbox isolation proof.
- T04 needs a completed interactive Keychain approval or a separately reviewed credential-lifecycle change before a fresh end-to-end proof. The earlier stale-token rejection must not be represented as a current response failure because this run did not use that token.

## Cleanup

Both owned T03 sandboxes were purged. Accounts `a14of-gd` and `a14of-gl` and their providers were removed. No T04 account, provider, or sandbox was created. The exported T03 ref was deleted from the temporary sentinel clone. The original sentinel checkout remained clean at `1e9431e4e98895ac616bef42fe2f61d1a9c68a20`.

The derived tag `jackin.invalid/a14of-sentinel:6fbacfb03d92eb3a` and role tag `jackin.invalid/role/a14of-sentinel:6fbacfb03d92eb3a` remain in the local image store. Jackin exposes no owned image-cleanup command, and this proof did not bypass the jackin-image Docker boundary with a manual Docker command.

Foreign sandboxes and providers were left alone. The remaining docker sandboxes are `jackin-jackin-rqw3`, `jackin-jackin-3yuk`, and `jackin-arch-pr-dqq7`. The remaining local sandbox is `jackin-a14t02b-995e`.

## G-FAST

The temporary catalog flips were restored before the gate. `git diff --exit-code -- crates/jackin-core/src/agent.rs` exited 0. The required command ran from the worktree root without a command-time cap:

```text
mise exec -- mbx +1.99.1 xtask ci --fast
```

It exited 0 in 562.319 seconds. The final summaries were:

```text
Summary [ 302.412s] 2156 tests run: 2156 passed (1 slow), 0 skipped
advisories ok, bans ok, licenses ok, sources ok
shear/summary
  ✓ no issues found
Files with copyright information: 1212 / 1212
Files with license information: 1212 / 1212
Congratulations! Your project is compliant with version 3.3 of the REUSE Specification :-)
```
