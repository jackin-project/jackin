# Target design of jackin on OpenShell

This document is dated 2026-11-02. Its status is approved for execution. The branch is `feature/openshell-native` in all five repositories (PLAN.md lists them).

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

### D03. Talk to OpenShell through the SDK, and use the CLI for terminal and file transfer

- Use the `openshell-sdk` crate as a git dependency at the pinned tag.
- Use its raw tonic clients for OpenShell providers and policy.
- Use the `openshell` CLI for four things only: `sandbox connect`, `sandbox exec --tty`, `sandbox upload`, and `sandbox download`. These run over SSH in OpenShell 0.1.6 and have no plain gRPC form. They need the system `ssh` binary.
- Pass the gateway, the OpenShell workspace, and the sandbox name explicitly on every call. Never change the active gateway of the operator. If you give no name, `sandbox connect` uses the last sandbox. Always give the name.
- A jackin workspace is not an OpenShell workspace. The jackin gateway config names the OpenShell workspace that jackin uses.
- Jackin gets the gateway endpoint from `openshell gateway list -o json`. For mTLS, jackin reads the `ca.crt`, `tls.crt`, and `tls.key` files of that gateway entry. Jackin does not write the registry.

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
3. Jackin builds the role image with `docker buildx build`. It gives the construct image to the build as the named build context `construct`.
4. Jackin derives the final image. The derived layer installs Codex at the D02 version at `/usr/local/bin/codex`. It disables automatic update. It sets the user to `sandbox`.
5. Jackin delivers the final image to the gateway (D23).

The image identity is its content digest. The cache key is a hash of the role commit, the `Dockerfile` digest, the construct digest, and the Codex version. When the key matches a cached image, jackin does not build again. `--rebuild` ignores the cache. Jackin streams the build output to the CLI and to the console.

### D06. Jackin owns the construct base image

`jackin-image` embeds `images/construct/Dockerfile`. Jackin builds the construct image locally and caches it by digest. The base image needs no registry. The image has these properties:

- It starts from the OpenShell default sandbox base image.
- It has the non-root user `sandbox`.
- It contains no `sudo`, no firewall tool, and no container tool.
- It contains the shared shell tools that roles need.

The derived layer adds the Docker client, and the engine when D24 needs one, only when the Docker capability is enabled. The cache key includes the capability.

The Codex binary path is part of the `AgentSpec` row. The derived layer and the OpenShell provider profile use the same path, so the profile and the image cannot differ.

### D07. One sandbox for each launch, and labels are the only index

- `jackin load` creates one OpenShell sandbox for one workspace, role, and account.
- The sandbox name is `jackin-<workspace>-<random suffix>`.
- Labels on the sandbox hold the workspace, role, agent, account, image digest, and launch ID.
- Jackin keeps no local instance index and no database. To find sandboxes, jackin lists them by label.
- A retry with the same launch ID finds the existing sandbox and continues from its observed state. It does not create a second sandbox.

### D08. Upload the project and export work as a git bundle

- At create time, jackin runs `openshell sandbox upload` to copy the project directory into `/sandbox/project`. The upload respects `.gitignore` and includes `.git`.
- To retrieve work, `jackin export` creates a git bundle in the sandbox and downloads it. It then fetches the bundle into the host repository as `refs/heads/jackin/<sandbox-name>`.
- Jackin never writes to the host working tree.
- Jackin has no bind mounts and no sync daemon.

### D09. The agent is the sandbox main process

- The main process of the sandbox is `/usr/local/bin/jackin-start`, with a kept PTY. The derived layer installs it. It waits for the marker `/sandbox/.jackin/ready-<launch-id>`. Then it runs the `setup` hook once and the `preflight` hook. Then it replaces itself with Codex. If a hook fails, it exits with a non-zero code. Jackin reads the code from the sandbox status.
- `jackin attach` gives the terminal to `openshell sandbox connect`. OpenShell keeps the PTY alive after detach and replays output on reattach.
- `jackin shell` gives the terminal to `openshell sandbox exec --tty`.
- There is no Capsule and no in-sandbox daemon. In the console, the tabs and panes of the `jackin-preview` demo run on the host (D28). Each pane is an OpenShell terminal session.

### D10. Credentials stay on the gateway

In this document, an **OpenShell provider** is the OpenShell credential object. A **provider** without the word OpenShell is an AI coding service (D26).

- Each account and agent pair has one OpenShell provider. Its name is `jackin-<account>-codex`.
- Mode A is an API key. The sandbox receives an `OPENAI_API_KEY` placeholder. The gateway replaces the placeholder only on the allowed hosts.
- Mode B is a ChatGPT login. It uses the upstream Codex refresh pattern. The gateway holds the refresh token, and the sandbox receives a placeholder `auth.json`.
- Build the mode that the A1 credential permits first. The development Mac has only a host ChatGPT login, so build mode B first.
- Mode B imports an existing Codex login. The command `jackin account add codex --chatgpt-from <codex-home>` reads `auth.json` and puts the refresh material into the OpenShell provider. A device login needs a person, so no proof includes it.
- Jackin never writes a real token into a sandbox, an image, a log, a label, or the jackin config.
- For live tests, find the credential in the order that PLAN.md item A1 gives. A shared ChatGPT refresh token can rotate and break the host login. Accept this risk, and record the choice in the trail.
- Run live runs that share one refresh token one after the other. Never run them in parallel.

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
- `jackin-start` runs `setup` once after the upload and `preflight` before each Codex start (D09). A non-zero exit stops the launch.
- Put only non-secret values in `[env]`.
- Without a TTY, an `[env]` entry uses its `default`. An entry without a default stops the launch, unless `--env NAME=VALUE` gives the value.
- `[docker] enabled = true` enables the Docker capability for each launch of the role (D24).
- There is no schema migration. A change to the format is a breaking change.

### D12. Policy is native OpenShell policy

Jackin sends the role `policy.yaml` to OpenShell without change. Jackin adds the network rules from the Codex `AgentSpec` row. Jackin has no policy language of its own. After jackin creates the sandbox, only the `network_policies` section can change.

### D13. The operator owns the gateway

Jackin does not install, start, or stop an OpenShell gateway. The operator runs `openshell-gateway` and registers it with `openshell gateway add`. The operator, not jackin, chooses the compute driver. On macOS, try the `vm` driver first. If task P0.2 proves that the `vm` driver does not work, use the OpenShell Docker driver. The development Mac has OrbStack Docker 29.8.2 at `~/.orbstack/bin/docker`. Jackin uses it only for BuildKit builds (D01). `jackin doctor` verifies the gateway health and the pinned version. It does not repair the gateway.

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

The layer rule orders the crates. `jackin-core` is below `jackin-image` and `jackin-openshell`. Both are below `jackin-session`. `jackin-session` is below `jackin-tui` and `jackin`. `xtask` verifies this rule from `cargo metadata`. Only `jackin-openshell` imports `openshell-sdk`. Only `jackin-image` starts `docker`.

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

The final image must be visible to the gateway compute driver. The gateway config in jackin names one delivery method:

- `docker-local`. The OpenShell Docker driver reads the same Docker image store that BuildKit loads into.
- `registry`. Jackin pushes the image to a registry that the gateway can pull from, such as a local registry.

The `vm` driver can read a local engine first. Task P0.3 proves which method works with the chosen driver. Task P0.3 also records the buildx builder, because a `docker-container` builder cannot read a local `docker-image://` named context. Record the result in the trail.

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
- When the capability is disabled, the image has no Docker client, and the policy permits no engine endpoint.
- When the capability is enabled, the agent starts its containers through the engine. Jackin does not start containers.
- Jackin never starts or stops an engine itself.

Use the first engine source in this list that task P0.5 proves:

1. An engine inside the sandbox. The containers stay inside the sandbox isolation, and OpenShell deletes them with the sandbox. This is the preferred source.
2. A dedicated external engine that the operator names in the gateway config. The sandbox reaches it through an OpenShell network policy rule that permits only that endpoint.
3. The default Docker engine of the host. Use it only when the operator names it explicitly in the config. In this program, only the `xtask e2e` config names it, and `security-reviewer` records the acceptance in task P5.5. Access to it equals root access on the host. `jackin load` and the console show a warning for each launch that uses it.

The engine credentials stay outside the sandbox when OpenShell can inject them. If OpenShell cannot inject them, task P0.5 records the gap in a blocker report. Then the operator-named engine must be a dedicated engine that holds nothing else.

### D27. The console looks identical to terminal-components-baseline

The jackin console is visually identical to the `jackin-preview` app in `tailrocks/terminal-components-baseline`, called "the baseline" here. Jackin does not use `termrock`.

- `jackin-tui` depends on the baseline library crate `junie-tui` as a git dependency at a pinned `rev`. The pin is a commit on the baseline branch `feature/openshell-native`.
- The baseline repository is `/Users/donbeave/Projects/jackin-project/terminal-components-baseline`. Its own `AGENTS.md` applies to every change in it. Read it before each baseline task.
- On 2026-11-02 the `jackin-preview` screens exist only in the baseline binary. Move each screen pattern into a reusable block of the baseline library. Make `jackin-preview` use those blocks. Make jackin use the same blocks with real data.
- If the baseline lacks what jackin needs, extend the baseline. Do not copy baseline code into jackin. The known extensions are:
  - the blocks for the jackin screens, behind a data interface
  - an API that gives the terminal to a child process and takes it back, for `openshell sandbox connect`
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
- The session tabs and panes of the demo run in the host console. Each pane is an OpenShell terminal session. The Codex pane connects to the sandbox main process. A shell pane uses `ExecSandboxInteractive`. Task P0.6 proves how the console attaches to the main process in a pane.
- The 1Password flow of the demo stays. An account can name its credential as an `op://` reference. Jackin resolves it on the host with `op` and sends it only to the OpenShell provider.
- When a demo screen has no jackin data yet, build the data. Do not remove the screen.
- The CLI commands in this document stay. The console is the main way to use jackin, as in the demo.
- The identity gate drives the demo key sequences against the jackin console on the baseline fixture data. Each resulting screen must match its `jackin-preview` tuiscotti baseline.

## CLI surface

| Command | Result |
|---|---|
| `jackin` | Opens the console on a TTY. Prints help when there is no TTY. |
| `jackin doctor` | Verifies the `openshell` binary, the `ssh` binary, the gateway health, and the pinned version. |
| `jackin build [<role>] [--rebuild]` | Builds the role image and delivers it to the gateway. |
| `jackin load [<role>] [<path>] [--account <a>] [--gateway <g>] [--rebuild] [--launch-id <id>] [--env NAME=VALUE]...` | Builds the image when necessary, creates or restores a sandbox, uploads, runs hooks, starts Codex, and attaches. |
| `jackin attach <sandbox>` | Reattaches to the Codex PTY. |
| `jackin shell <sandbox>` | Opens a new shell in the sandbox. |
| `jackin ls [--json]` | Lists jackin sandboxes by label. |
| `jackin inspect <sandbox> --json` | Prints the sandbox, labels, OpenShell providers, and policy status. |
| `jackin export <sandbox>` | Fetches the sandbox work into `refs/heads/jackin/<sandbox>`. |
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

`jackin-session` has a pure planner and an executor. The planner turns the config and the request into a `LaunchPlan`. The executor runs these steps against the `Gateway` trait. Each step is idempotent.

1. Resolve the Docker capability (D24). Build the image, or take it from the cache, and deliver it to the gateway (D05, D23).
2. Ensure that the OpenShell provider for the account exists.
3. Find a sandbox with the launch ID label. If none exists, create the sandbox with the role image, the merged policy, the OpenShell provider, the labels, and `jackin-start` as the main process.
4. Wait until the sandbox is ready.
5. Upload the project, unless the marker `/sandbox/.jackin/uploaded-<launch-id>` exists. Write the marker last.
6. Write the marker `/sandbox/.jackin/ready-<launch-id>`. `jackin-start` runs the hooks and starts Codex (D09).
7. Attach. If `jackin-start` exits with a non-zero code, show the hook error instead.

If the operator cancels, jackin keeps the sandbox and reports it as a restore candidate.
