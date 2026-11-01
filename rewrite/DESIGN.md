# Target design of jackin on OpenShell

This document is dated 2026-11-02 and was corrected from the P0 evidence in task P0.4. Its status is approved for execution. The branch is `feature/openshell-native` in all five repositories (PLAN.md lists them).

This document records the target design and the decisions behind it. It is a reference. [PLAN.md](PLAN.md) tells you how to build the design. The reports in [research/](research/) are evidence. If a report disagrees with this document, this document is correct.

## Product statement

Jackin is developer experience. It is a Rust CLI and a terminal console that run AI coding agents in NVIDIA OpenShell sandboxes. Jackin builds the role images. OpenShell is the only execution engine. Jackin does not run containers and does not enforce isolation. OpenShell runs the containers and enforces the isolation.

The first and only agent is OpenAI Codex. Jackin adds other agents later, after the Codex path is proven end to end.

## Reference baseline

The pre-rewrite code is a reference, not a base. Its last commit is `470fc5d6f5eae8621867d49b6937838dbfb2cec9` on `main` (2026-10-29). To read an old file, run `git show 470fc5d6:<path>`. Do not keep old code as a precaution. Do not write a migration, an importer, or a compatibility path.

## Decisions

Each decision has an ID. Cite the ID in commits, briefs, and reviews. To change a decision, write a new row in [decisions.tsv](decisions.tsv) and edit this section in the same commit.

### D01. OpenShell is the only execution engine, and BuildKit is the only image builder

- Every agent, sandbox, and shell runs in OpenShell.
- Jackin never runs a container with Docker, Podman, or Apple `container`. An agent can run containers in its sandbox only through the Docker capability (D24). Jackin code never calls `docker run`, `docker create`, `docker start`, `docker exec`, or a Docker API client.
- Jackin uses Docker BuildKit only to build images, through the `docker buildx` CLI with an explicit builder. Only the `jackin-image` crate starts `docker`.
- `deny.toml` bans `bollard` and every Docker client crate.
- The container step of `xtask ci` fails if code starts `podman` or `container`. It fails if code starts `docker` outside `jackin-image`. It fails if code starts a `docker` subcommand other than `buildx build`, `buildx create`, `buildx inspect`, `image inspect`, and `push`.

### D02. Use the latest stable version of every tool

Use the latest stable release of every tool and every dependency. Do not use a prerelease. Task P0.1 sets the pins. Tasks P8.0 and P13.0 look again for newer stable releases. Each newer release gets its own task. That task runs the affected live proofs again. The pins on 2026-11-02 are in this table:

| Tool | Version | Source |
|---|---|---|
| Rust | 1.99.1 | mise `rust`, with tool option `mr_boxington = true` |
| mr-boxington (`mbx`) | 1.29.1 | mise `mr-boxington` |
| OpenShell | 0.1.6 (`ac7f615cd12d5cc49cb16eb1f5f10ffeb1e90f47`, 2026-10-27) | Homebrew `openshell`, git tag `v0.1.6` |
| Codex CLI | 0.166.0 (2026-10-29) | `@openai/codex` |
| cargo-nextest | 0.9.150 | mise aqua |

The operator set the global toolchain with this command:

```sh
mise use --global --tool-option mr_boxington=true rust mr-boxington
```

The repository `mise.toml` and `rust-toolchain.toml` must agree with these pins. Run every cargo command through `mise exec -- mbx`. Record the OpenShell pin in `compatibility.lock.toml` at the repository root.

### D03. Talk to OpenShell through the SDK, and use the CLI only for file transfer

- Use the `openshell-sdk` crate as a git dependency at the pinned tag. Use it for sandboxes, labels, exec, and terminal sessions. Use its raw tonic clients for OpenShell providers and policy.
- Run a command in a sandbox with `ExecSandboxInteractive`. A hook run and a file write use no PTY and send their data on stdin. A shell uses a PTY.
- Attach to the sandbox main process in the jackin process: `CreateSshSession`, then `ForwardTcp` with an `SshRelayTarget`, then a `russh` client that requests a PTY and the `openshell-main` subsystem. Pin the host key to the `host_key_fingerprint` of the `CreateSshSession` response. Create one session token for each attachment. One token allows 3 streams, and one sandbox allows 20.
- `jackin attach`, `jackin shell`, and the console panes use these transports. Jackin does not hand the terminal to the `openshell` CLI.
- Use the `openshell` CLI for three things only: `sandbox upload`, `sandbox download`, and `gateway list -o json`. Upload and download run over the system `ssh` binary, and the filtered upload runs the host `git`. Run each CLI call with stdin from `/dev/null` and a hard timeout. P0.2 saw one upload call hang before it reached the gateway.
- Pass the gateway, the OpenShell workspace, and the sandbox name explicitly on every call. Never change the active gateway of the operator.
- A jackin workspace is not an OpenShell workspace. The jackin gateway config names the OpenShell workspace that jackin uses.
- Jackin gets the gateway endpoint from `openshell gateway list -o json`. For mTLS, jackin reads the `ca.crt`, `tls.crt`, and `tls.key` files of that gateway entry. Jackin does not write the registry.
- Only `jackin-openshell` depends on `openshell-sdk`, `tonic`, and `russh`. The SDK does not re-export tonic.

Evidence: `evidence/P0.6-panes.md` (transport research), `evidence/P0.2-gateway.md` (Gaps G1).

### D04. Codex is the only agent

`jackin-core` defines `enum Agent { Codex }` and a `const` table of `AgentSpec` rows. A row holds the binary path, the state directory, the OpenShell provider profile, the network policy rules, and the launch command. The Codex launch command is `codex --sandbox danger-full-access -c check_for_update_on_startup=false`. The OpenShell sandbox already isolates Codex, and the Codex sandbox fails inside it. To add an agent, add one variant and one row. Every `match` on `Agent` must stay exhaustive.

### D05. Jackin builds the role image

A role repository contains:

- `Dockerfile`. Its final stage starts `FROM construct`.
- `jackin.role.toml`. This is the manifest (D11).
- `policy.yaml`. This is a native OpenShell policy.

Jackin builds the image for each launch that needs one:

1. Jackin validates the role `Dockerfile`. The final stage must start `FROM construct`.
2. Jackin builds the construct image (D06), or takes it from the cache.
3. Jackin builds the role image with `docker buildx build`. It uses a builder with the `docker` driver, because a `docker-container` builder cannot read a local `docker-image://` context. It passes `--provenance=false --sbom=false --load` and the platform of the gateway. It gives the construct image to the build as the named build context `construct`.
4. Jackin derives the final image from its own embedded derived `Dockerfile`, with the role image as the named build context `role`. The build context holds only `jackin-start` and the role hook files (D11). As root, the derived layer:
   - installs `codex` and `codex-code-mode-host` at the D02 version from the pinned Codex package archive, selected by `TARGETARCH`, with the checksum of each platform from the `AgentSpec` row
   - writes `/etc/codex/config.toml` with `check_for_update_on_startup = false`
   - runs `codex --version` with `CODEX_HOME` in a temporary directory and deletes the directory in the same `RUN`
   - installs `jackin-start` at `/usr/local/bin/jackin-start`
   - installs the hooks at `/usr/local/lib/jackin/hooks/setup` and `/usr/local/lib/jackin/hooks/preflight`, owned by root, mode 0755
   - sets the user to `sandbox`
5. Jackin delivers the final image to the gateway (D23).

The image identity is the Docker image ID. The cache key is a hash of the role commit, the `Dockerfile` digest, the construct digest, the Codex version, the platform, the Docker capability, and the digests of `jackin-start` and each hook file. The image tag is the cache key. When a local image with that tag exists, jackin does not build again. `--rebuild` ignores the cache. Jackin streams the build output to the CLI and to the console.

Evidence: `evidence/P0.3-codex.md`, `evidence/P7.1-image.md`.

### D06. Jackin owns the construct base image

`jackin-image` embeds `images/construct/Dockerfile`. Jackin builds the construct image locally and caches it by digest. The base image needs no registry. The image has these properties:

- It starts from the OpenShell default sandbox image `nvcr.io/nvidia/base/ubuntu:24.04`, pinned by digest.
- It removes the user `ubuntu` and creates the user `sandbox` with uid and gid 1000 and home `/sandbox`. The `vm` driver takes the runtime user from `/etc/passwd`, not from the image `USER`.
- It clears every setuid and setgid bit. The build fails when `sudo`, a firewall tool, or a container tool is on `PATH`.
- It contains `git`, `curl`, `ca-certificates`, `jq`, `bash`, and the other shared shell tools that roles need. Export needs `git` (D08).
- Jackin puts each of its files under `/usr/local`, because the default Landlock policy denies `/opt`.

When the Docker capability is enabled, the derived layer adds the Docker client at `/usr/local/bin/docker` and its `buildx` and `compose` plugins. It never adds an engine (D24). The cache key includes the capability.

The `AgentSpec` row holds the Codex binary paths, the install archive URL template, and the checksum of each platform. The derived layer and the OpenShell provider profile read the same row, so the profile and the image cannot differ. The profile `binaries` list names only `/usr/local/bin/codex`.

Evidence: `evidence/P0.2-gateway.md`, `evidence/P0.3-codex.md`, `evidence/P7.1-image.md`.

### D07. One sandbox for each launch, and labels are the only index

- `jackin load` creates one OpenShell sandbox for one workspace, role, and account.
- The sandbox name is `jackin-<workspace>-<random suffix>`.
- Labels on the sandbox hold the workspace, role, agent, account, image digest, and launch ID.
- Jackin keeps no local instance index and no database. To find sandboxes, jackin lists them by label.
- A retry with the same launch ID finds the existing sandbox and continues from its observed state. It does not create a second sandbox.

### D08. Upload the project and export work as a git bundle

- Jackin uploads the project with two CLI calls, with the project root as the working directory:
  1. `openshell sandbox upload <sandbox> . /sandbox/project`. It respects `.gitignore`, uploads untracked files that are not ignored, and omits `.git`.
  2. `openshell sandbox upload <sandbox> .git /sandbox/project --no-git-ignore`. It adds `.git`.
- A path other than `.` lands in `/sandbox/project/<basename>`. The first call needs `git` and a work tree on the host.
- To retrieve work, `jackin export` runs `git bundle create` in `/sandbox/project` through exec, downloads the bundle from `/sandbox/.jackin/`, compares the sha256 digests, and fetches each branch into the host repository as `refs/heads/jackin/<sandbox-name>/<branch>`.
- Work is exported when the sandbox has no uncommitted change and each branch tip exists under `refs/heads/jackin/<sandbox-name>/` on the host. A sandbox that jackin cannot inspect counts as not exported.
- Jackin never writes to the host working tree.
- Jackin has no bind mounts and no sync daemon.

Evidence: `evidence/P0.2-gateway.md` step 4.

### D09. jackin-start supervises Codex and never exits by itself

OpenShell 0.1.6 does not reach a sandbox after its main process exits. Exit code 0 gives the phase `Completed`, and another code gives `Error`. Both phases refuse exec and attach. In `Error`, `stop` fails and `start` stays in `Starting`. So the main process must stay alive, or unexported work is trapped (ADR-010).

- The main process is `/usr/local/bin/jackin-start <Codex launch command>`, with a kept PTY. The derived layer installs it. Jackin passes the launch command of the `AgentSpec` row as the arguments, and `JACKIN_LAUNCH_ID` in the environment.
- At each start, `jackin-start` removes its old state and writes `waiting` to `/sandbox/.jackin/run/state`. When jackin writes a new attempt ID to `/sandbox/.jackin/run/start`, `jackin-start` writes `running <attempt>` and runs Codex as its child in `/sandbox/project`. When Codex exits, `jackin-start` writes `exited <attempt> <code>`, prints one line to the PTY, and waits for the next attempt ID.
- `jackin-start` writes only `state`, and jackin writes only `start`. Each write is a temporary file and a rename. The attempt ID fences a stale request.
- `jackin-start` runs no hooks. Jackin runs them through exec (D11).
- `jackin-start` traps `SIGINT`, `SIGTERM`, and `SIGHUP`, so a Ctrl-C while it waits does not end it. It exits only when the launch ID or the arguments are invalid, before the first attempt.
- The state files are writable by the agent. Jackin treats them as advisory. Jackin never decides on export or purge from them.
- `jackin attach` opens the main PTY in the jackin process (D03). OpenShell keeps the PTY alive after detach and replays up to 1 MiB of raw output on reattach. The replay can start inside an escape sequence, so after each attach jackin resizes the PTY to one row less and back to force a redraw.
- Ctrl-D always detaches, so Codex never receives it. A lone Ctrl-P is held until the next byte, and Ctrl-P Ctrl-Q detaches.
- A second attachment is a read-only viewer, but it still resizes the shared PTY. The console opens one pane for each main process.
- `jackin shell` opens a new process with `ExecSandboxInteractive` and a PTY. It has no detach and no replay. The process ends when the stream closes.
- `jackin stop` and `jackin start` stop and start the sandbox. The main process starts again from the beginning and writes `waiting`.
- There is no Capsule and no in-sandbox daemon. In the console, the tabs and panes of the `jackin-preview` demo run on the host (D28).

Evidence: `evidence/P0.2-gateway.md` steps 5, 7, 8; `evidence/P7.1-image.md` F3; `evidence/P0.6-panes.md`. Task P7.4 proves this supervisor live.

### D10. Credentials stay on the gateway

In this document, an **OpenShell provider** is the OpenShell credential object. A **provider** without the word OpenShell is an AI coding service (D26).

- Each account and agent pair has one OpenShell provider. Its name is `jackin-<account>-codex`. It uses the OpenShell provider profile `codex`, which starts from the upstream `examples/codex-app-server/codex.yaml`, because `providers/codex.yaml` has no refresh block. Its `binaries` list is `/usr/local/bin/codex`. A profile ID can be imported once, so jackin imports the profile only when it is absent.
- Mode A is an API key. The sandbox receives an `OPENAI_API_KEY` placeholder. The gateway replaces the placeholder only on the allowed hosts.
- Mode B is a ChatGPT login. The gateway holds the access token and the refresh token and refreshes the access token. At each launch, jackin writes `/sandbox/.codex/auth.json`, mode 0600, through exec stdin. The file holds the access token placeholder, an empty refresh token, a non-secret placeholder ID token, and the real ChatGPT account ID.
- The account ID is an identifier, not a secret. Codex 0.166.0 fails with a placeholder account ID. Jackin stores the account ID in the jackin account entry.
- Build the mode that the A1 credential permits first. The development Mac has only a host ChatGPT login, so build mode B first.
- Mode B imports an existing Codex login. The command `jackin account add codex --chatgpt-from <codex-home>` reads `auth.json` and puts the refresh material into the OpenShell provider. The gateway then rotates the refresh token, so the host login may need a new `codex login` later. A device login needs a person, so no proof includes it.
- The gateway replaces a placeholder by destination host, for each process that the policy lets reach the host. A rule for a binary also covers the processes that the binary starts. So a command that Codex starts can use the credential, but it cannot read it.
- Provider host closure: in the merged policy, only the generated `_provider_<name>` rule may match a host of the Codex OpenShell provider profile. This includes wildcard hosts, any port, and any binary. `jackin role check` and `jackin load` reject a role policy that breaks the rule. `jackin inspect` reports a live policy that breaks it.
- Jackin never writes a real token into a sandbox, an image, a log, a label, or the jackin config. The one exception is the Docker client key of D24.
- For live tests, find the credential in the order that PLAN.md item A1 gives. A shared ChatGPT refresh token can rotate and break the host login. Accept this risk, and record the choice in the trail.
- Run live runs that share one refresh token one after the other. Never run them in parallel.

Evidence: `evidence/P0.3-codex.md`.

### D11. The role manifest is small and strict

```toml
schema = 1

[role]
name = "sentinel"
dockerfile = "Dockerfile"
policy = "policy.yaml"

[codex]
model = "<optional model name>"

[docker]
enabled = false

[hooks]
setup = "hooks/setup.sh"
preflight = "hooks/preflight.sh"

[env.EXAMPLE_NAME]
default = "value"
prompt = "Text for the operator"
```

- The parser rejects unknown fields.
- Hook paths are relative to the role repository. The parser rejects a path that is absolute, contains `..`, or is not a regular file in the role repository. The derived layer copies the hooks into the image (D05). The agent cannot change them.
- Jackin runs the hooks through exec as the user `sandbox`, with no PTY, the working directory `/sandbox/project`, the `[env]` values, and stdin closed. `setup` runs once for each launch: one exec runs the hook and then writes `/sandbox/.jackin/setup-done-<launch-id>`. `setup` must be idempotent, because a crash before the marker runs it again. `preflight` runs before each Codex start.
- A non-zero exit stops the launch. Jackin shows the hook name, the exit code, and the output, with terminal control sequences removed. The sandbox stays `Ready` and is a restore candidate. A cancel ends the exec stream, which stops the hook.
- Put only non-secret values in `[env]`.
- Without a TTY, an `[env]` entry uses its `default`. An entry without a default stops the launch, unless `--env NAME=VALUE` gives the value.
- `[docker] enabled = true` enables the Docker capability for each launch of the role (D24).
- There is no schema migration. A change to the format is a breaking change.

### D12. Policy is native OpenShell policy

Jackin sends the role `policy.yaml` to OpenShell without change. OpenShell adds the `_provider_<name>` network rule of the Codex OpenShell provider itself. Jackin adds the D24 engine rule when the Docker capability is enabled. Jackin has no policy language of its own. After jackin creates the sandbox, only the `network_policies` section can change.

### D13. The operator owns the gateway

Jackin does not install, start, or stop an OpenShell gateway. The operator runs `openshell-gateway` and registers it with `openshell gateway add`. The operator, not jackin, chooses the compute driver. On the development Mac the driver is `vm`. The Homebrew package lacks the `openshell-driver-vm` binary, so the operator downloads it from the release, checks its digest, signs it with the hypervisor entitlement, installs `e2fsprogs`, and generates the certificates. The gateway how-to (from `drafts/gateway.md`) gives the script. The development Mac has OrbStack Docker 29.8.2 at `~/.orbstack/bin/docker`. Jackin uses it for BuildKit builds (D01). The `vm` driver reads images from the same engine through `/var/run/docker.sock` (D23). `jackin doctor` verifies the `openshell`, `ssh`, and `git` binaries, the gateway health, the pinned version, and the engine socket. It does not repair the gateway.

Evidence: `evidence/P0.2-gateway.md` step 1.

### D14. The workspace has eight crates

```text
crates/
  jackin/           binary. clap commands, terminal hand-off, wiring.
  jackin-core/      library. ids, Agent and AgentSpec, config model, role manifest, redaction. No I/O except config files.
  jackin-image/     library. Dockerfile validation, construct and derived layers, cache key, buildx runner, image delivery.
  jackin-openshell/ library. Gateway trait, SDK adapter, CLI argument builders for connect, exec, upload, and download.
  jackin-session/   library. launch plan and execution, attach, shell, export, list, stop, start, purge, host process runner.
  jackin-tui/       library. console wiring of the terminal-components-baseline blocks to jackin data (D27).
  jackin-testkit/   library. in-process fake gateway and fixtures. Dev-dependency only.
  xtask/            binary. repository gates.
```

The kept usage crates (D25) live under `crates/usage/`. They are below `jackin-tui` and `jackin`, and above `jackin-core`.

The layer rule orders the crates. `jackin-core` is below `jackin-image` and `jackin-openshell`. Both are below `jackin-session`. `jackin-session` is below `jackin-tui` and `jackin`. `xtask` verifies this rule from `cargo metadata`. Only `jackin-openshell` imports `openshell-sdk`, `tonic`, and `russh`. Only `jackin-image` starts `docker`.

### D15. Configuration is one TOML file

The config is `~/.config/jackin/config.toml`. It holds gateways, accounts, roles, and workspaces. Jackin parses it once at the boundary and rejects unknown fields. Jackin writes the file atomically. There is no SQLite, no turso, and no schema version.

### D16. These features are deleted

Delete these features. Do not rebuild them in this program:

- the native macOS app and `jackin-usage-ffi`
- OpenTelemetry export (keep `tracing` to stderr)
- the jackin daemon and its notifications. The console shows OpenShell gateway health instead (D28).
- workflows and repair loops
- the in-sandbox Capsule daemon. The host console gives the tabs and panes (D28).
- agent status inference
- config migrations
- the old Docker-in-Docker manager, Docker profiles, and Docker grants. D24 replaces them.
- every agent except Codex

### D17. Code has no comments

- Do not write comments in code.
- Write clap help text with `#[command(about = "...")]` and `#[arg(help = "...")]` attributes. Do not use doc comments for help text.
- Write only these comments:
  - SPDX license header lines
  - shebang lines, such as `#!/usr/bin/env bash`
  - parser directives, such as the Dockerfile `# syntax=` line
  - files that a repository generator writes and verifies byte for byte
- The comment step of `xtask ci` rejects every other comment in `.rs`, `.toml`, `.sh`, and `Dockerfile` files.

### D18. Tests are new, and each test is proven red then green

- Delete every old test.
- Write new tests only for the behavior that this program builds.
- Obey the test rules in `AGENTS.md`. Before you write a test, get approval of its plan from the `test-reviewer` subagent. Keep a test only after `test-reviewer` sees it fail on broken code and pass on fixed code.
- The nextest suite is hermetic. It uses the fake gateway from `jackin-testkit`.
- Live tests against a real gateway run through `xtask e2e`, outside nextest. The orchestrator runs them and records the output in [evidence/](evidence/).

### D19. Sentinel is the verification role

`jackin-sentinel` is a Codex role whose purpose is to prove that jackin works. Its image contains `jackin-sentinel-probe`. This script runs in the sandbox and prints one JSON object with the result of each in-sandbox journey step. `xtask e2e` runs the sentinel journey in [PLAN.md](PLAN.md#sentinel-journey). It compares what the host sees with what the probe reports. If a journey step has no credentials, its result is `SKIP`. The result is never `PASS`.

### D20. Agent Smith and The Architect become Codex roles

- Agent Smith is a Codex role for code review. Its policy allows only the Codex hosts and `github.com`.
- The Architect is a Codex role with the Rust toolchain for jackin development. Its policy also allows the crates.io, GitHub, and mise download hosts.
- Delete every Claude file, plugin, marketplace script, and hook from both roles.
- The final stage of each role `Dockerfile` starts `FROM construct`. Jackin builds both images.

### D21. The docs site stays and its content is rewritten

Keep the Fumadocs site in `docs/`. Rewrite the content for OpenShell and Codex. Delete the `research/`, `roadmap/`, `integration/`, and `development/` sections and every Docker or Apple page.

### D22. Earlier decisions change

- Delete `docs/openshell-v2/`. This document replaces it.
- This document supersedes ADR-005 (Capsule render path), ADR-007 (container backend trait), and ADR-011 (macOS usage app).
- ADR-008 stays. Launch creates or restores. It never silently reconnects.
- ADR-009 stays. A hook failure stops the launch.
- ADR-010 stays. Jackin never deletes a sandbox that has uncommitted or unexported work without explicit confirmation.

### D23. Jackin delivers the image to the gateway

The final image must be visible to the gateway compute driver. Jackin has one delivery method, `docker-local`. The `vm` driver looks up the image in the local Docker engine through `/var/run/docker.sock` or the `DOCKER_HOST` of the gateway process, before it tries a registry. Jackin loads the final image into that engine (D05) and creates the sandbox with the image tag. The driver ignores a local image of another platform, so jackin builds for the platform of the gateway. Jackin builds no registry method until a gateway needs one.

Evidence: `evidence/P0.3-codex.md` step 2.

### D25. The usage crates stay

Jackin keeps account usage tracking. Move these crates to `crates/usage/` and port them to the new design:

- `jackin-usage`, `jackin-usage-broker`, `jackin-usage-broker-publish`, and `jackin-usage-broker-wire`
- `jackin-usage-coordinator`, `jackin-usage-snapshot-store`, and `jackin-usage-store-backend`
- `jackin-usage-discovery`, `jackin-usage-credential-resolver`, and `jackin-usage-credential-snapshots`
- `jackin-usage-host-accounts`, `jackin-usage-host-credentials`, `jackin-usage-host-glance`, `jackin-usage-host-presentation`, and `jackin-usage-host-runtime`
- `jackin-usage-projection`, `jackin-usage-destination`, `jackin-usage-output`, and `jackin-usage-token-monitor`
- `jackin-usage-provider-core` and every `jackin-usage-provider-*` crate: amp, antigravity, claude, codex, cursor, gemini, grok, hermes, kimi, minimax, muse, opencode, openrouter, and zai

Obey these rules in the port:

- Remove every dependency on deleted crates, such as `jackin-protocol`, `jackin-telemetry`, Capsule, and Docker. Move the usage wire types into the usage crates.
- Delete the old tests and comments, as for all other code.
- Usage runs on the host. It reads the credentials of a jackin account on the host. It never sends a credential into a sandbox.
- Delete `jackin-usage-capsule-telemetry` and `jackin-runtime-usage-relay`. They exist only for Capsule.
- Usage supports every provider, not only the agents that jackin can run (D26).
- The task that ports the crates can merge crates when a crate has one caller. It records each merge in the trail.

`jackin usage` shows the limits of each account. For each account, it shows every limit window that its provider reports, for example:

- the 5-hour window
- the weekly window
- the monthly window, credits, or spend, when the provider has them

For each window, it shows the percentage used, the remaining amount when the provider gives it, and the reset time. When a provider gives no data for an account, `jackin usage` shows the reason, for example an expired login. `jackin usage --json` prints the same data with a typed data model. The console usage panel shows the same data for all accounts and for the bound accounts of each workspace.

### D26. Accounts cover every provider, and the console manages them

Jackin uses two different terms:

- A **provider** is an AI coding service that has accounts and usage, such as Codex, Claude, Amp, Grok, Gemini, Cursor, Kimi, MiniMax, Z.AI, or OpenRouter. The provider list comes from the usage provider crates (D25).
- An **agent** is a CLI that runs in a sandbox. Codex is the only agent (D04). The Codex agent uses accounts of the Codex provider.

The operator adds an account for any provider. The console and the CLI create, edit, and delete workspaces, roles, and accounts. They manage these bindings:

- The **default account** of each provider.
- A **workspace binding**. It holds one account for each provider in that workspace. The usage panel shows the usage of the bound accounts for each workspace.
- A **role binding**. It holds the default account of the role for each agent.

For each launch, jackin selects the agent account in this order:

1. the `--account` flag
2. the workspace binding for the provider of the agent
3. the role binding for the agent
4. the default account of the provider
5. the only account of the provider, when exactly one exists

If no rule selects an account, the launch stops with an error that names the missing binding. An OpenShell provider (D10) exists only for accounts that an agent uses. Accounts of other providers stay on the host for usage only.

### D24. The Docker capability is opt-in for each role and each workspace

Some workspaces must start Docker containers during implementation and tests. The Docker capability gives the agent in a sandbox a Docker engine.

- The capability is disabled by default. A launch enables it when the role manifest has `[docker] enabled = true` or the workspace has `docker = true`.
- When the capability is disabled, the image has no Docker client, the policy permits no engine endpoint, and the sandbox gets no engine file or variable.
- When the capability is enabled, the agent starts its containers through the engine. Jackin does not start containers.
- Jackin never starts or stops an engine itself.

The engine is a dedicated external engine. It holds nothing else. The operator starts it on the gateway host and names it in the jackin gateway config, because OpenShell has no place for an engine endpoint. The recommended form is rootless Docker-in-Docker. The engine listens on `127.0.0.1` with TLS and client certificates, and its server certificate names `host.openshell.internal`.

An engine inside the sandbox is impossible in OpenShell 0.1.6. The workload has no capabilities, `no_new_privs` is set, and seccomp denies `mount` and user namespaces. Upstream tracks it as `NVIDIA/OpenShell#4970`. When upstream allows it, it becomes the preferred source. The default engine of the host is not a source, because a sandbox that reaches it has root access to the host.

When the capability is enabled:

- The policy gets one rule: host `host.openshell.internal`, the engine port, `protocol: tcp`, `tls: skip`, binary `/usr/local/bin/docker`. `tls: skip` is required, because the proxy removes the `Upgrade` header and `docker run` then fails. A role that runs `docker build` adds registry egress for the `docker-buildx` plugin in its own policy.
- Jackin writes `ca.pem`, `cert.pem`, and `key.pem` to `/sandbox/.jackin/docker/` through exec stdin before the first attempt. The key has mode 0600.
- The sandbox environment and each shell request have `DOCKER_HOST=tcp://host.openshell.internal:<port>`, `DOCKER_TLS_VERIFY=1`, and `DOCKER_CERT_PATH=/sandbox/.jackin/docker`.

OpenShell 0.1.6 cannot keep a working engine credential outside the sandbox (`blockers/P0.5-docker-credentials.md`). So the agent can read the client key. `jackin load` and the console show a warning for each launch with the capability. The warning says these facts:

- The agent can read the engine client key.
- Containers bypass the sandbox network policy.
- A bind mount names a path on the engine host, not in the sandbox.
- The engine outlives the sandbox, and sandboxes that use one engine see each other's containers. The operator removes them.

Evidence: `evidence/P0.5-docker.md`, `blockers/P0.5-docker-credentials.md`.

### D27. The console looks identical to terminal-components-baseline

The jackin console is visually identical to the `jackin-preview` app in `tailrocks/terminal-components-baseline`, called "the baseline" here. Jackin does not use `termrock`.

- `jackin-tui` depends on the baseline library crate `junie-tui` as a git dependency at a pinned `rev`. The pin is a commit on the baseline branch `feature/openshell-native`.
- The baseline repository is `/Users/donbeave/Projects/jackin-project/terminal-components-baseline`. Its own `AGENTS.md` applies to every change in it. Read it before each baseline task.
- On 2026-11-02 the `jackin-preview` screens exist only in the baseline binary. Move each screen pattern into a reusable block of the baseline library. Make `jackin-preview` use those blocks. Make jackin use the same blocks with real data.
- If the baseline lacks what jackin needs, extend the baseline. Do not copy baseline code into jackin. The known extensions are:
  - the blocks for the jackin screens, behind a data interface
  - a terminal pane block that renders a session from a byte stream with the `vt100` parser and encodes keys, behind a Cargo feature. It does no I/O.
  - an input variant for external messages, for tokio events
  - a Cargo feature for `clap`, so the library does not always include it
- A baseline change must never change its existing visual output. Each baseline task runs the full baseline test suite. All approved tuiscotti baselines under `tests/functional/baselines/` must pass unchanged. Never move the `visual-baseline` git tag.
- Approve the new tuiscotti baseline of a new screen with `tuiscotti accept` after a `ux-specialist` review. Never change an existing baseline file.
- Every screen of `jackin-preview` exists in jackin with the same look and the same behavior (D28).
- Jackin adds these screens with baseline components: the role editor, the role binding, the Docker setting of a workspace, and the doctor panel.
- Jackin uses the baseline application loop (`Application`, `Page`, `Request`). Jackin does not add a second application loop.
- The identity gate runs the jackin console on the baseline fixture data in a PTY and compares it with the `jackin-preview` tuiscotti baselines.

### D28. The jackin-preview demo is the authority for how jackin is used

Jackin has the same screens, the same flows, the same key bindings, and the same navigation as the `jackin-preview` demo in the baseline. Each demo scenario and each key-driven demo state is an acceptance case for jackin.

- The demo shows Docker and Capsule words in some screens. Jackin shows the same screen with OpenShell content. For example, the cockpit stages name the image build, the OpenShell provider, the sandbox, and the upload. The daemon health screen shows the OpenShell gateway health.
- The session tabs and panes of the demo run in the host console. Each pane is an OpenShell terminal session in the jackin process (D03). The Codex pane attaches to the main process through `CreateSshSession`, `ForwardTcp`, and `russh`. A shell pane uses `ExecSandboxInteractive`. The console opens one pane for each main process, owns the detach and close keys, never sends Ctrl-D, and sends the pane size after each layout change. Each attach creates a fresh parser and runs the D09 redraw resize. Evidence: `evidence/P0.6-panes.md`.
- The 1Password flow of the demo stays. An account can name its credential as an `op://` reference. Jackin resolves it on the host with `op` and sends it only to the OpenShell provider.
- When a demo screen has no jackin data yet, build the data. Do not remove the screen.
- The CLI commands in this document stay. The console is the main way to use jackin, as in the demo.
- The identity gate drives the demo key sequences against the jackin console on the baseline fixture data. Each resulting screen must match its `jackin-preview` tuiscotti baseline.

## CLI surface

| Command | Result |
|---|---|
| `jackin` | Opens the console on a TTY. Prints help when there is no TTY. |
| `jackin doctor` | Verifies the `openshell`, `ssh`, and `git` binaries, the gateway health, the pinned version, and the engine socket (D13). |
| `jackin build [<role>] [--rebuild]` | Builds the role image and delivers it to the gateway. |
| `jackin load [<role>] [<path>] [--account <a>] [--gateway <g>] [--rebuild] [--launch-id <id>] [--env NAME=VALUE]...` | Builds the image when necessary, creates or restores a sandbox, uploads, runs hooks, starts Codex, and attaches. |
| `jackin attach <sandbox> [--restart]` | Reattaches to the Codex PTY. When Codex has exited, it shows the exit code. `--restart` runs `preflight` and starts Codex again (D09). |
| `jackin shell <sandbox>` | Opens a new shell in the sandbox. |
| `jackin ls [--json]` | Lists jackin sandboxes by label. |
| `jackin inspect <sandbox> --json` | Prints the sandbox, labels, OpenShell providers, and policy status. |
| `jackin export <sandbox>` | Fetches each sandbox branch into `refs/heads/jackin/<sandbox>/<branch>` (D08). |
| `jackin stop <sandbox>` and `jackin start <sandbox>` | Stops or starts the sandbox. |
| `jackin purge <sandbox> [--force]` | Deletes the sandbox. It refuses when the work is not exported and `--force` is absent. |
| `jackin account add <provider> [--chatgpt-from <codex-home>]\|ls\|rm` | Manages accounts of any provider. Codex accounts also get an OpenShell provider. |
| `jackin account default <account>` | Makes the account the default for its provider (D26). |
| `jackin account bind <account> --role <r> \| --workspace <w>` | Binds an account to a role or a workspace (D26). |
| `jackin usage [--json]` | Shows the 5-hour, weekly, and other limit windows of each account, with the percentage used and the reset time (D25). |
| `jackin role add\|ls\|rm` | Manages role sources. |
| `jackin role check <path>` | Validates a role manifest, its `Dockerfile`, and its policy file. Role repositories use it as their gate. |
| `jackin workspace add\|ls\|rm` | Manages workspaces, including the `docker` setting. |

## Launch sequence

`jackin-session` has a pure planner and an executor. The planner turns the config and the request into a `LaunchPlan`. The executor runs these steps against the `Gateway` trait. Each step is idempotent. The identity of a launch is its launch ID.

1. Resolve the Docker capability (D24). Build the image, or take it from the cache, and deliver it to the gateway (D05, D23).
2. Ensure that the OpenShell provider profile `codex` and the OpenShell provider for the account exist (D10).
3. Find the sandbox with the launch ID label. More than one is an error. If none exists, create the sandbox with the role image, the merged policy, the OpenShell provider, the labels, the environment, a kept PTY, and `jackin-start` as the main process.
4. Wait until the sandbox is `Ready`, because the create call returns first. Start a `Stopped` sandbox and wait. A `Completed` or `Error` sandbox stops the launch with its phase and exit code. Jackin never deletes it here.
5. Write the Codex `auth.json` (D10) and, when the capability is enabled, the Docker client files (D24), through exec stdin.
6. Upload the project (D08), unless the marker `/sandbox/.jackin/uploaded-<launch-id>` exists. Write the marker last.
7. Wait until `/sandbox/.jackin/run/state` exists, with a timeout. If it says `running`, go to step 10.
8. Run `setup`, unless `/sandbox/.jackin/setup-done-<launch-id>` exists. Run `preflight`. A failure stops the launch with the hook error (ADR-009). The sandbox stays `Ready`.
9. Write a new attempt ID to `/sandbox/.jackin/run/start`. Wait until the state says `running <attempt>`.
10. Attach (D09).

A retry with the same launch ID continues at the observed state. Jackin starts Codex again only on an explicit request: `jackin load` with the same launch ID, or `jackin attach --restart` (ADR-008).

If the operator cancels, jackin keeps the sandbox and reports it as a restore candidate.
