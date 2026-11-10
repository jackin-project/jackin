# A14 final-head live proof for T01 Codex with ChatGPT

Date: 2026-11-10. Run window: 04:20:31Z to 05:26:36Z. Base commit: `e275950ca3f5d82d17dfa49789f9582d3487088b`. Worktree: `/tmp/jackin-a14-t01-final`. Branch: `wt/a14-t01-final`.

## Verdict

T01 passes on `jackin-docker` and `jackin-local` with Codex CLI 0.167.0, provider `openai`, model `gpt-6.1-sol`, and a fresh ChatGPT login. Both gateways returned exact `PONG`, ran a real shell tool, returned exact `TOOL-OK`, and reported nonzero token usage. Credential, process, network, terminal, export, purge, refresh, and host-login checks passed.

This verdict does not promote the catalog. The committed catalog remains `Unavailable` until independent verification and a separate promotion change. No commit or push occurred. The proof-only catalog edit was restored. No G-FAST run occurred because production code did not change.

## Proof setup

The proof binary was `/tmp/jackin-a14-t01-final/target/debug/jackin`, built with this command:

```text
mise exec -- mbx +1.99.1 build -p jackin
```

The launcher fails closed while T01 is `Unavailable`. The only production-code diff during the proof changed the T01 tuple from `TupleState::Unavailable` to `TupleState::Supported` and changed its reason to `None` in `crates/jackin-core/src/agent.rs`. `git restore -- crates/jackin-core/src/agent.rs` removed that edit after the proof. `git diff --exit-code -- crates/jackin-core/src/agent.rs` exited 0.

`openshell --version` reported 0.1.7. `jackin doctor --gateway jackin-docker` and `jackin doctor --gateway jackin-local` passed the binary, gateway registration, gateway health, pinned version, and engine checks. OpenShell ran every sandbox and every sandbox command. Only Jackin's image path invoked Docker BuildKit, with `DOCKER_CONTEXT=default`. No Docker, Podman, or host fallback ran an agent, sandbox, or shell.

The proof used `XDG_CONFIG_HOME=/tmp/a14t01f-xdg`. Its OpenShell configuration used symbolic links to the existing gateway registry and TLS material. The Jackin configuration contained only the two gateway entries and the owned proof resources. The host Jackin configuration had zero `a14t01f` matches after cleanup.

The temporary sentinel clone was `/tmp/jackin-a14t01f-sentinel` at `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its uncommitted role edit named the role `a14t01f-sentinel` and pinned `gpt-6.1-sol`. Docker capability remained disabled. `jackin role check /tmp/jackin-a14t01f-sentinel` exited 0. The original sentinel checkout stayed clean at the same commit.

Both accounts used the supported `codex-login` source and the fresh host Codex home:

```text
jackin --gateway jackin-docker account add codex --name a14t01f-cd --chatgpt-from /Users/donbeave/.codex
jackin --gateway jackin-local account add codex --name a14t01f-cl --chatgpt-from /Users/donbeave/.codex
```

The imports created `jackin-a14t01f-cd-codex` and `jackin-a14t01f-cl-codex`. Account listing recorded `codex-login` for both accounts. No credential value appeared in command output.

## Secret handling

The host Codex login was the only credential source. No 1Password command ran. `OPENAI_API_KEY` was unset for imports, launches, and host checks. No paid-key environment variable supplied a fallback.

Temporary capture programs read the host token fields in memory. They compared those values with sandbox environment output, readable text files up to 4 MiB under `/sandbox`, `jackin inspect --json`, and captured stderr. The programs printed only byte counts, match counts, placeholder booleans, and variable lengths. They did not write raw environment output, auth files, inspect output, or stderr to disk. No raw key was passed as an argument. Model and terminal output passed through the same in-memory host-token check before display.

The sandbox had no `python3`. The first file-scan attempt exited 127 without scanning files. The capture program then used the available `find`, `grep`, `cat`, and `jq` tools. The successful scan results below supersede that proof-tool setup failure.

## Docker proof

### Image and cached rebuild

The launch command was:

```text
DOCKER_CONTEXT=default jackin load a14t01f-sentinel /tmp/jackin-a14t01f-sentinel --agent codex --provider codex --account a14t01f-cd --gateway jackin-docker --model gpt-6.1-sol --launch-id a14t01fcodexd01 --task "Reply with exactly: LAUNCH-READY and take no other action."
```

The initial launch printed `image role: built` and `image derived: built`. The role tag was `jackin.invalid/role/a14t01f-sentinel:f820751af4e83ce5`. The derived image was `sha256:0523330587b3c8fb45d5f4a4f945bf089c5a3347ae816e56a066350a99ae1bed`.

A second launch with the same launch ID printed `image construct: cached`, `image derived: cached`, `image: skipped, the image is cached`, `sandbox: skipped, the sandbox exists`, and `upload: skipped, the project is uploaded`.

The sandbox was `jackin-a14t01f-qz47`. The account was `a14t01f-cd`. The provider was `jackin-a14t01f-cd-codex`. The image label was `0523330587b3c8fb45d5f4a4f945bf08`.

### Process and credential state

The sandbox reported `uid=1000(sandbox)`, `sudo-absent`, and `codex-cli 0.167.0`. The Codex binary SHA256 was `2fe94d848540ff2a6868c7ebcc494126a974aa409711676d14fa0379e49e6143`.

Codex PID 390 was the direct child of `jackin-start codex` PID 64. The supervisor state was `running 1`. `codex login status` reported `Logged in using ChatGPT`.

`/sandbox/.codex/auth.json` and `config.toml` had mode 0600. The config contained `model = "gpt-6.1-sol"`. The auth file had `auth_mode=chatgptAuthTokens` and `OPENAI_API_KEY=null`. The access token was an `openshell:resolve:` placeholder. The refresh token was empty. The ID token was the catalog's alg-none JWT-shaped value with a `.placeholder` signature. The account ID was a nonsecret literal.

The environment contained only broker placeholders for `CODEX_AUTH_ACCESS_TOKEN` and `CODEX_AUTH_ACCOUNT_ID`. Their lengths were 111 and 64. `CODEX_AUTH_REFRESH_TOKEN`, `CODEX_AUTH_ID_TOKEN`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, and `ANTHROPIC_AUTH_TOKEN` were unset.

The final scan covered 815 bytes of environment output, 3,441,053 bytes of readable sandbox text, and 565 bytes of inspect JSON. It found zero host-token matches and zero stderr matches. Inspect listed only `jackin-a14t01f-cd-codex`, a loaded policy, and an empty `closure_violations` array.

### Fixed response, tool use, and subscription attribution

The fixed request used this command shape:

```text
openshell -g jackin-docker sandbox exec jackin-a14t01f-qz47 -- sh -c 'cd /sandbox/project && codex exec --sandbox danger-full-access -c check_for_update_on_startup=false -m gpt-6.1-sol "Reply with exactly: PONG"'
```

It returned exit 0 and exact `PONG`. Codex reported `model: gpt-6.1-sol`, `provider: openai`, session `01a1c3b8-0135-77f3-90ed-d7c069ca7bd7`, and 3,334 tokens used.

The second request asked Codex to run `ls -A`, verify success, and reply with exactly `TOOL-OK`. Codex invoked `/bin/bash -lc 'ls -A'` in `/sandbox/project`. The command succeeded and returned the real sentinel tree, including `AGENTS.md`, `Dockerfile`, `instructions`, `jackin.role.toml`, `probe`, and `tests`. Codex returned exact `TOOL-OK`, session `01a1c3b8-5227-7532-b8c3-d82cb450b9bb`, and 3,784 tokens used.

The only attached provider held the imported ChatGPT login. The auth file and CLI both identified ChatGPT authentication. No API key or alternate paid provider was configured. The nonzero Codex usage therefore records subscription-backed use without a paid fallback. These token counts are CLI usage metadata, not invoices.

### Provider-only egress

Requests from `curl` to `https://api.openai.com/v1/models`, `https://auth.openai.com/`, `https://example.com/`, and `https://api.anthropic.com/` each exited 7 with HTTP `000`. Codex reached OpenAI and received successful responses. The non-agent binary had only placeholder environment values and received no provider connection or usable credential.

### Refresh and host concurrency

The provider had an `oauth2_refresh_token` configuration for `CODEX_AUTH_ACCESS_TOKEN`. The initial refresh completed at 04:25:46Z. This command forced a gateway-owned rotation:

```text
openshell -g jackin-docker provider refresh rotate jackin-a14t01f-cd-codex --credential-key CODEX_AUTH_ACCESS_TOKEN
```

The rotation reported `refreshed` at 04:33:15Z. The next sandbox request returned exact `POST-REFRESH-PONG` on `gpt-6.1-sol`, session `01a1c3bb-71b9-75c0-87cc-d96c953581de`, with 3,342 tokens used. While that sandbox was present, the host returned exact `HOST-T01-DURING`, thread `01a1c3bb-86cb-72d3-8308-a58f67b9344c`, with 26,137 input tokens and 10 output tokens.

### Detach, reattach, and shell persistence

Two real PTY attaches replayed the Codex 0.167.0 screen with `gpt-6.1-sol`. Both sent Ctrl-P Ctrl-Q and exited 0. The capture reported zero host-token matches.

A shell pane printed `A14T01F-SHELL-OK`, returned uid 1000 and `/sandbox/project`, and exited 0. Codex PID 390 remained the direct child of `jackin-start` PID 64. The supervisor remained `running 1`.

### Export and guarded purge

Sandbox commit `4359c862d5e307f655d0d1ffab3f6967405b3f2d` created unexported work. Guarded purge refused with exit 3 and reported the unexported branch and uncommitted role edit.

The first export created only `refs/heads/jackin/jackin-a14t01f-qz47/feature/openshell-native` at that commit. It warned that the role edit remained unexported. A second guarded purge also refused with exit 3. The proof then committed the remaining role edit in the sandbox, producing `db08234ebdf493bd0127d52d46ccec8ea31c9049`. The next export updated the same ref to that commit. Guarded purge deleted the sandbox without `--force`.

The temporary host HEAD stayed `6722f8024623c76726ce0293fc7e93da47b5ee74`. Its index SHA256 stayed `7522b0b900d727c62950392fe3506c998229ac377ef0c8e6b8f228cd1ef39060`. Its staged diff stayed empty. Its working diff SHA256 stayed `f9f8e3d6e48f6c8e77301e34e7767fb10ef377a313cfc55c2511e5b677fd87f6`.

## Local proof

### Cached image and process state

The local launch used account `a14t01f-cl`, launch ID `a14t01fcodexl01`, and the same model and task options. It created `jackin-a14t01f-dozp`. The initial launch printed `image derived: cached` and used the same derived image. A same-launch reload also skipped the existing sandbox and uploaded project.

The sandbox reported uid 1000, no sudo, Codex 0.167.0, and the same binary SHA256. Codex PID 697 was the direct child of `jackin-start` PID 373. The supervisor state was `running 1`. `codex login status` reported ChatGPT.

The auth and config files had mode 0600 and the same model and placeholder properties as docker. The access and account-ID environment values were broker placeholders of lengths 111 and 65. The other credential variables were unset.

The final scan covered 816 bytes of environment output, 3,441,110 bytes of readable sandbox text, and 565 bytes of inspect JSON. It found zero host-token matches and zero stderr matches. Inspect listed only `jackin-a14t01f-cl-codex`, a loaded policy, and no closure violations.

### Fixed response, tool use, and egress

The fixed request returned exact `PONG`, `model: gpt-6.1-sol`, `provider: openai`, session `01a1c3be-1e84-76d2-a092-ef479dbb57c6`, and 3,334 tokens used.

The tool request ran `/bin/bash -lc 'ls -A'` successfully, returned the real sentinel tree, and finished with exact `TOOL-OK`. It reported session `01a1c3be-2fff-7f32-85be-842da5c820b2` and 6,981 tokens used.

The same four `curl` requests each exited 7 with HTTP `000`. Only the local ChatGPT provider was attached. No paid fallback was configured.

### Forced rotation and host usability

Independent review found that the original evidence lacked a forced local rotation. The follow-up used a fresh `codex-login` import named `a14t01fr-cl`, role `a14t01fr-sentinel`, workspace `a14t01fr`, and launch ID `a14t01frlocal01`. It created the new local sandbox `jackin-a14t01f-z8zu` at the same base and sentinel commits. The proof-only T01 catalog edit was restored afterward.

The follow-up sandbox reported Codex 0.167.0, `Logged in using ChatGPT`, and `model = "gpt-6.1-sol"`. Codex PID 704 was the direct child of `jackin-start` PID 373. The auth and config files had mode 0600. The access token was a broker placeholder, the refresh token was empty, the ID token had the alg-none `.placeholder` signature, and `OPENAI_API_KEY` was null. Inspect listed only `jackin-a14t01fr-cl-codex` and no closure violations.

This command forced gateway-owned rotation on `jackin-local`:

```text
openshell -g jackin-local provider refresh rotate jackin-a14t01fr-cl-codex --credential-key CODEX_AUTH_ACCESS_TOKEN
```

At 05:22:50Z, the rotation reported `refreshed` for `CODEX_AUTH_ACCESS_TOKEN`. The next sandbox request returned exact `LOCAL-POST-REFRESH-PONG`, `model: gpt-6.1-sol`, `provider: openai`, session `01a1c3e8-deab-7e73-802c-d22872ac1233`, and 3,346 tokens used.

While `jackin-a14t01f-z8zu` remained active, the fresh host request returned exact `HOST-T01FR-DURING`, thread `01a1c3e8-f91e-7aa1-b7de-85b368b426b7`, with 26,138 input tokens and 11 output tokens. Before rotation, after that host reply, and after cleanup, the host auth file retained mode 0600, size 4,373 bytes, and mtime `Nov 10 08:57:58 2026` local time.

The final post-rotation scan covered 816 bytes of environment output, 3,325,397 bytes of readable sandbox text, and 568 bytes of inspect JSON. It found zero host-token matches and zero stderr matches. The access and account-ID environment values were broker placeholders of lengths 111 and 65. The forbidden credential variables remained unset.

### Detach, shell, export, and purge

Two PTY attaches replayed Codex 0.167.0 with `gpt-6.1-sol` and detached with exit 0. A shell pane printed `A14T01F-SHELL-OK`, returned uid 1000 and `/sandbox/project`, and exited 0. Codex PID 697 remained the direct child of `jackin-start` PID 373.

Sandbox commit `975908203e5a1cf24c26ae359b2e697c3f531e55` included the new proof file and the role configuration. Guarded purge refused with exit 3 and `branch feature/openshell-native not exported`. Export created only `refs/heads/jackin/jackin-a14t01f-dozp/feature/openshell-native` at that commit. Guarded purge then deleted the sandbox without `--force`.

The temporary host HEAD, index SHA256, staged diff, and working diff SHA256 remained unchanged.

## Host-login safety

The host used Codex CLI 0.167.0 and `Logged in using ChatGPT` for every check. Each host request selected `gpt-6.1-sol` and unset `OPENAI_API_KEY`.

| Check | Exact reply | Thread | Input tokens | Output tokens |
|---|---|---|---:|---:|
| Before | `HOST-T01-BEFORE` | `01a1c3af-c886-71f1-8194-427748ed39f6` | 26,374 | 10 |
| During docker | `HOST-T01-DURING` | `01a1c3bb-86cb-72d3-8308-a58f67b9344c` | 26,137 | 10 |
| After the original runs | `HOST-T01-AFTER` | `01a1c3c0-0918-75b3-a507-9e8c3176aaaf` | 26,137 | 10 |
| During the local follow-up | `HOST-T01FR-DURING` | `01a1c3e8-f91e-7aa1-b7de-85b368b426b7` | 26,138 | 11 |

Before and after, `/Users/donbeave/.codex/auth.json` had mode 0600, size 4,373 bytes, and mtime `Nov 10 08:57:58 2026` local time. The sandbox runs and gateway refreshes did not change that metadata. The host login remained usable.

## Cleanup

Both original owned sandboxes were removed. Both exported refs were deleted. Accounts `a14t01f-cd` and `a14t01f-cl` and their providers were removed. The `a14t01f` workspace and `a14t01f-sentinel` role were removed. The original temporary sentinel clone, XDG directory, and capture programs were deleted.

The follow-up sandbox `jackin-a14t01f-z8zu` was removed with `jackin purge --force` without repeating the export lifecycle. Account `a14t01fr-cl` and its provider were removed. The `a14t01fr` workspace and `a14t01fr-sentinel` role were removed. The follow-up temporary sentinel clone, XDG directory, and capture programs were deleted. No follow-up export ref existed.

The final provider checks reported zero owned providers on both gateways. The final sandbox inventories contained no `a14t01f` or `a14t01fr` sandbox. Foreign sandboxes and providers were not changed, including foreign resources created by other runs during this proof.

The role and derived images remain in the BuildKit cache under the unique `a14t01f-sentinel` and `a14t01fr-sentinel` tags. Jackin has no image-removal command. Direct Docker removal was not used because only `jackin-image` may start Docker.

## Remaining issue

No live proof check failed. The local rotation evidence gap is closed. Independent re-review and a separate catalog promotion remain. The evidence and blocker update are uncommitted, and the pushed catalog remains unavailable.
