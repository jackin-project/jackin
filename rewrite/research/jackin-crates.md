> Research input dated 2026-11-01. It is evidence, not authority. If it disagrees with [DESIGN.md](../DESIGN.md), DESIGN.md is correct. Grok text is out of scope.

# jackin on OpenShell: crate inventory and target architecture

Repo: /Users/donbeave/Projects/jackin-project/jackin, branch main, HEAD 470fc5d6. Date checked: 2026-11-01.
OpenShell reference: local clone at <openshell-clone at v0.1.6> (HEAD ac7f615cd, a v0.1.6 prerelease line). All OpenShell claims below come from that clone's `proto/`, `docs/`, `providers/`, and `crates/openshell-sdk/`.

Scope as of 2026-11-02: Codex is the only supported agent. Grok Build is dropped completely.

## 0. Result in numbers

152 `Cargo.toml` files exist under `crates/`: 151 workspace members plus `jackin-lints` (excluded, own workspace). Total src LoC (wc -l on `src/**/*.rs`, including inline `#[cfg(test)]` modules, excluding `tests/` dirs) is about 346k.

| Verdict | Crates | src LoC |
|---|---|---|
| DELETE | 117 | 162k |
| REWRITE | 27 | 174k (console 53.5k and xtask 43k are most of it) |
| KEEP-PURE | 5 | 1.5k |
| UNSURE | 3 | 8.6k |

Nothing larger than a few hundred lines survives unchanged. The REWRITE count is high because concepts survive while every implementation is bound to Docker, the capsule, or the old config. Realistic salvage is about 15-20k LoC of fragments (listed in section 5).

## 1. Constraints from recorded decisions (stop-for-owner items)

The brief says `docs/openshell-v2/` is input, not authority, and the user goal explicitly removes Docker. The design below still contradicts these recorded items. Each needs the owner's explicit sign-off before work starts:

1. `docs/openshell-v2/SPEC.md` "Roles and reproducible image construction": "Keep launch-time image building" with Docker/Buildx. Contradicted: no image builds in jackin. OpenShell itself does not build Dockerfiles (`docs/how-it-works/sandboxes/overview.mdx` lines 192-210 in the OpenShell clone).
2. SPEC "Capsule v2 and persistent sessions": a resident unprivileged Capsule with relay-over-exec. Contradicted: this design drops Capsule in v1 and uses OpenShell's retained main-process PTY. See Q1.
3. SPEC "per-user Jackin control" host service and durable Operation journal. Contradicted: no host daemon; OpenShell is the state owner, with labels as the join key.
4. ADR-011 (accepted): native macOS usage menu-bar app over `jackin-usage-ffi`, display domain Claude, Codex, Amp, Grok, Z.AI, Kimi, MiniMax. With Codex as the only agent, six of seven providers are gone. Contradicted: `native/` loses its only Rust dependency if usage is dropped. See Q5.
5. ADR-007 (container backend lifecycle trait) is superseded outright. ADR-005 (capsule single render path) dies with the capsule.
6. ADR-001 sets a 150k LoC threshold for workspace adoption. The new code is far below 150k, so a single crate is permitted by the ADR's own rule. The proposed 11 crates are justified by boundary enforcement (pure domain vs I/O), not size. If the owner prefers ADR-001 literally, collapse to the 5-crate variant in section 4.
7. `compatibility.lock.toml` is named in the project note but does not exist in the repo (`find . -name compatibility.lock.toml` returned nothing). The OpenShell version pin has to be created.

Survivors from the ADRs: ADR-008 (launch creates or restores, never silently reconnects), ADR-009 (role hooks are the author's domain, hard-fail on non-zero), ADR-010 (never auto-delete dirty or unpushed work), ADR-003 (ratatui).

## 2. What OpenShell gives jackin (verified in the clone)

| OpenShell primitive | Source | Consequence for jackin |
|---|---|---|
| Gateway gRPC API, Rust SDK `openshell-sdk` (git dep, pin to release tag) | `docs/sdk/rust.mdx`, `crates/openshell-sdk/src/client.rs` | Curated: health, create/get/list/delete/stop/start/suspend/resume sandbox, templates, workspaces, `wait_ready`, non-interactive `exec` (captured result), `watch_logs`. Everything else (providers, policy, interactive exec, port forward, SSH sessions, services) goes through `raw_grpc()`. |
| `ClientConfig` / `ClientIdentity` (mTLS), OIDC bearer | `crates/openshell-sdk/src/config.rs:62`, `lib.rs:48` | The SDK does not read the CLI's gateway registry on disk (`docs/sdk/rust.mdx`). Jackin must resolve gateway endpoint plus credentials itself (read `~/.config/openshell/gateways/<name>/metadata.json` layout, or run `openshell gateway list -o json`). Rust SDK docs say "no mTLS" but source has `ClientIdentity`; docs lag (SPEC already notes this). |
| `SandboxSpec { name, image, labels, environment, providers, command, tty, restart_policy, service_exposures, gpu }` | `crates/openshell-sdk/src/types.rs:280` | Labels are the free-form join key: jackin stores workspace, role, agent, account, launch-id as labels. No local instance index needed. |
| Canonical main process is a retained PTY. `sandbox connect` reattaches, replays up to 1 MiB, one stdin owner, auto-reconnect for 60 s | `docs/how-it-works/sandboxes/overview.mdx` lines 270-311 | This is the capsule's "keep agent running after disconnect" feature, for free. Extra independent shells via `exec --tty`. |
| `ExecSandboxInteractive` (stdin stream, resize, tty flag) | `proto/openshell.proto:293`, `:2040`, `:1946` | Embedded terminal panes are possible later through raw gRPC; not needed for v1. |
| `ForwardTcp`, `ExposeService`, `CreateSshSession` | `proto/openshell.proto:219-282` | Port-forward and dev-server exposure. `openshell sandbox connect` itself spawns system `ssh` with a ProxyCommand (`crates/openshell-cli/src/ssh.rs:226`). |
| File transfer: `sandbox upload` / `download`, git-ignore-aware, symlink-preserving, tar over SSH | `overview.mdx` lines 903-975 | There are no host bind mounts. Project delivery is upload or in-sandbox `git clone`. This ends mount isolation modes (shared/worktree/clone) as implemented today. |
| Providers: named credential bundles, attach/detach per sandbox, endpoint-bound injection, profile YAML imported via `ImportProviderProfiles` | `proto/openshell.proto:132-162, 302-444`, `providers/README.md` | Replaces auth-forward modes (sync/api_key/oauth/ignore), the account auth handoff, and the capsule credential setup. |
| `providers/codex.yaml` exists as an example (CODEX_AUTH_* OAuth tokens plus account id, endpoints api.openai.com, auth.openai.com, chatgpt.com, ab.chatgpt.com) | `providers/codex.yaml`, `ls providers` | Jackin ships its own Codex profile (adapted copy of the upstream example). Profile `binaries` must match the image layout or the credential is never injected (`providers/README.md`). So the agent's binary path and its provider profile are one unit: `AgentSpec`. |
| Policy: filesystem, network, process; delivered by gateway, enforced in sandbox; draft-chunk approval workflow (`GetDraftPolicy`, `ApproveDraftChunk`, ...) | `proto/openshell.proto:456-760`, `docs/how-it-works/policies/` | Replaces Docker profiles/grants, allowed_hosts, capsule firewall/Landlock. The console can host the policy-approval inbox. |
| Default image `nvcr.io/nvidia/base/ubuntu:24.04`, no agent CLIs; `--from` takes any image visible to the driver; no Dockerfile builds | `overview.mdx` lines 248-255 | Roles cannot carry a Dockerfile any more. See Q2. |
| Workspaces (authority scope), members | `proto/openshell.proto:798-852`, `docs/how-it-works/workspaces.mdx` | Not project directories. Jackin passes an explicit gateway and workspace on every call and never changes the active gateway (matches SPEC). |

Codex facts from the old code (reference only, `core/jackin-core/src/agent/adapters/codex.rs`): credentials `~/.codex/auth.json`, `CODEX_HOME` folder variable, `OPENAI_API_KEY` for api-key mode, a sibling code-mode host binary installed beside `codex`, `.codex/tmp` excluded from default home. The upstream provider example expects `codex` at `/usr/bin/codex`, `/usr/local/bin/codex` or the npm global path; the old jackin image put it at `/home/agent/.local/bin/codex`. Whichever path the role image uses must appear in the profile's `binaries`.

## 3. Domain concepts: survive or not, and OpenShell mapping

| Today (CONTEXT.md, config) | Survives? | OpenShell-native form |
|---|---|---|
| Operator (human) | yes | Same. CLI + console on the host. |
| Role (git repo with Dockerfile + `jackin.role.toml`; tool profile) | yes, redefined | `Role { image: ImageRef, agents, setup hooks, env, policy fragment, provider requirements }`. Image is a pulled reference, not built. Maps to OpenShell sandbox image (`--from`) or a workload template. |
| Workspace (named workdir + mounts + allowed roles + defaults + env) | yes, redefined | `Workspace { name, sources: Vec<Source>, workdir, default_role, default_agent, env, account bindings }`. A `Source` is a host path (uploaded) or a git remote (cloned in sandbox). No bind mounts, no isolation enum. |
| Account (named credential identity per agent, bindings, 1Password refs, zshrc import) | yes, shrunk | `Account` -> one OpenShell **Provider** instance per (account, agent). `ensure_provider` is create-or-update; attach at sandbox creation. Secrets sourced from env, `op://` refs resolved on the host, or discovered local logins; gateway stores them. |
| Agent (12 today) | shrunk to 1 | `enum Agent { Codex }` + a `const` `AgentSpec` table (binary paths, provider profile, state dir, trust seed, launch argv). Adding an agent later is one variant plus one table row; every `match` stays exhaustive. |
| Instance (container + durable home + identity; statuses active/running/crashed/preserved_dirty/...) | yes | One OpenShell **Sandbox**, found by labels. Status is derived from sandbox phase (`Completed`, `Error`, running, stopped) plus a host-side dirty check. Durable home becomes the sandbox filesystem (`stop`/`start` per driver). |
| Launch (create or restore, never reconnect; ADR-008) | yes | `LaunchPlan` -> idempotent steps: ensure providers, create sandbox (labels carry launch-id), wait ready, materialize workspace, run setup hooks, start agent as canonical command. Restore = start/resume a stopped or errored sandbox. |
| Reconnect / hardline | yes | `AttachPlan` -> `openshell sandbox connect -g <gw> <name>` with inherited terminal (reattach + replay come from OpenShell). `--new`/`--shell` = `exec --tty`. |
| Session (tab/pane in capsule) | reduced | v1: the canonical main process (agent) plus extra `exec --tty` shells. Multiplexer tabs/panes are gone (Q1). |
| Capsule | no (v1) | Replaced by OpenShell supervisor + retained PTY. Possible later: tmux in the role image, or a slim resident helper. |
| Eject / exile / purge / prune | yes, shrunk | `stop`, `delete` (with ADR-010 guard), list-by-label. No image, home, DinD, socket-dir pruning. |
| Console TUI | yes | Same product, much smaller: workspaces, roles, accounts, sandboxes/sessions, launch, settings, policy-approval inbox. |
| Launch progress screen (matrix rain, progress rail, failure dialog) | yes | Steps are now provider/sandbox/upload/hooks instead of image build. |
| Role hooks (setup-once, source, preflight; ADR-009) | yes | Run through gateway `exec` at first start and each attach, hard-fail on non-zero. |
| Docker profiles, grants, DinD, host Docker socket | no | OpenShell policy and compute-driver choice. |
| Dirty/unpushed isolation and restore | yes (concept) | Host-side check by `exec git status` inside the sandbox plus `download`/`git push` out; ADR-010 prevents delete when dirty. |
| Keep-awake (caffeinate) | yes | Host-side; salvage `jackin-host/src/caffeinate.rs`. |
| Host clipboard image bridge, file export | no (v1) | Needed a capsule peer. |
| Usage/limits tracking, broker, token monitor, macOS menu bar | undecided | Q5. |
| Workflows (agent repair loops with test/review workers) | no (defer) | Each worker would be a sandbox; separate product line. |
| Telemetry (OTel registry) | no | `tracing` only unless Q7 says otherwise. |
| Daemon (`jackin daemon`, reactive attention notifications) | no | OpenShell gateway is the long-lived process. |
| Config migrations / schema versions | no | AGENTS.md: breaking changes preferred, no compatibility. |

## 4. Target architecture

### 4.1 Crate layout (11 workspace members + 1 excluded)

```
crates/
  jackin/            bin    clap CLI, main, exit codes, terminal ownership, wiring
  jackin-domain/     lib    pure types, ids, Agent enum, selectors, env values, redaction
  jackin-agents/     lib    AgentSpec table (Codex only), embedded provider profile, policy fragments
  jackin-config/     lib    TOML model + atomic persistence for roles/workspaces/accounts/gateways
  jackin-openshell/  lib    Gateway trait + SDK-backed adapter, AttachPlan argv builder
  jackin-session/    lib    LaunchPlan, state machine, materialize, attach, list/eject
  jackin-host/       lib    host effects: git, op, subprocess runner, caffeinate, open-url
  jackin-tui/        lib    ratatui/termrock runtime, tokens (brand), shared widgets, launch chrome
  jackin-console/    lib    console screens and update logic
  jackin-testkit/    lib    fake in-process OpenShell gateway, fixtures (dev-dependency only)
  xtask/             bin    slim dev gate driver
tools/jackin-lints  (excluded, dylint)
```

Dependency rule (enforced by a slim `arch` gate): `domain` <- `agents`, `config` <- `session` (also uses `openshell`, `host`) <- `console`, `jackin`. `tui` depends only on `domain` tokens. `testkit` depends on `openshell` and `domain`. Nothing under `domain`, `agents`, `config` performs I/O except `config`'s file persistence.

Five-crate variant if ADR-001 is read literally: `jackin` (bin; absorbs console, tui, session), `jackin-core` (domain + agents + config), `jackin-openshell`, `jackin-host`, `xtask`.

### 4.2 Core types (jackin-domain, jackin-agents)

```rust
pub enum Agent { Codex }

pub struct WorkspaceName(String);
pub struct RoleName { namespace: Option<String>, name: String }
pub struct AccountId(String);
pub struct GatewayName(String);
pub struct SandboxName(String);
pub struct LaunchId(uuid::Uuid);

pub enum EnvValue { Literal(String), Host(String), OpRef(OpRef) }

pub struct AgentSpec {
    pub agent: Agent,
    pub binary_paths: &'static [&'static str],
    pub install: InstallSpec,
    pub credential_env: &'static [&'static str],
    pub state_dir: &'static str,
    pub provider_profile: ProviderProfile,
    pub network_policy: PolicyFragment,
}
pub const fn spec(agent: Agent) -> &'static AgentSpec;
```

`AgentSpec::provider_profile` is rendered from the same `binary_paths` the role image contract names, so profile and image cannot drift.

### 4.3 Config model (jackin-config)

```rust
pub struct Config { pub gateways: BTreeMap<GatewayName, GatewayRef>, pub accounts: BTreeMap<AccountId, Account>,
                    pub roles: BTreeMap<RoleName, RoleRef>, pub workspaces: BTreeMap<WorkspaceName, Workspace> }
pub struct Workspace { pub sources: Vec<Source>, pub workdir: SandboxPath, pub default_role: Option<RoleName>,
                       pub default_agent: Option<Agent>, pub accounts: BTreeMap<Agent, AccountId>, pub env: BTreeMap<String, EnvValue> }
pub enum Source { Local { path: HostPath, upload: UploadFilter }, Git { url: GitUrl, rev: Option<GitRev> } }
pub struct Role { pub image: ImageRef, pub agents: NonEmpty<Agent>, pub hooks: Hooks, pub env: ..., pub policy: PolicyFragment }
```

Parse once at the boundary with `deny_unknown_fields` into these types; no schema version field and no migrations. Persistence keeps old `atomic_write` and the config read lock; edits go through a `toml_edit` editor to preserve user formatting.

### 4.4 OpenShell adapter (jackin-openshell)

```rust
#[async_trait]
pub trait Gateway: Send + Sync {
    async fn health(&self) -> Result<GatewayHealth, GatewayError>;
    async fn ensure_provider(&self, p: &ProviderSpec) -> Result<ProviderRef, GatewayError>;
    async fn create_sandbox(&self, c: &SandboxCreate) -> Result<SandboxRef, GatewayError>;
    async fn find_by_label(&self, k: &str, v: &str) -> Result<Vec<SandboxRef>, GatewayError>;
    async fn list(&self, sel: &LabelSelector) -> Result<Vec<SandboxRef>, GatewayError>;
    async fn wait_ready(&self, n: &SandboxName, t: Duration) -> Result<SandboxRef, GatewayError>;
    async fn upload(&self, n: &SandboxName, src: &HostPath, dst: &SandboxPath, f: &UploadFilter) -> Result<(), GatewayError>;
    async fn exec(&self, n: &SandboxName, argv: &[String], o: &ExecOptions) -> Result<ExecOutput, GatewayError>;
    async fn stop(&self, n: &SandboxName) -> Result<SandboxRef, GatewayError>;
    async fn start(&self, n: &SandboxName) -> Result<SandboxRef, GatewayError>;
    async fn delete(&self, n: &SandboxName) -> Result<(), GatewayError>;
}
pub struct AttachPlan { pub program: PathBuf, pub args: Vec<OsString> }   // pure builder for `openshell -g .. sandbox connect|exec --tty`
#[non_exhaustive] pub enum GatewayError { Unreachable, Unauthenticated, NotFound, AlreadyExists, Precondition(String), Rejected(String), Transport(String) }
```

`Gateway` is the one owned interface at an unowned boundary (the gRPC service), so a fake behind it is allowed by AGENTS.md. `GrpcGateway` wraps `openshell_sdk::OpenShellClient` plus `raw_grpc()` for providers, policy and interactive exec. The `AttachPlan` builder is pure and unit-testable; the spawn lives in `jackin-host`.

### 4.5 Launch flow and idempotency (jackin-session)

Pure planner, impure executor.

```rust
pub struct LaunchRequest { workspace: WorkspaceName, role: RoleName, agent: Agent, account: AccountId, gateway: GatewayName }
pub struct LaunchPlan { launch_id: LaunchId, sandbox: SandboxName, providers: Vec<ProviderSpec>, create: SandboxCreate,
                        materialize: Vec<MaterializeStep>, hooks: Vec<HookRun>, start: AgentStart }
pub enum LaunchStep { EnsureProviders, CreateSandbox, WaitReady, Materialize, RunHooks, StartAgent, Done }
pub fn plan(cfg: &Config, req: &LaunchRequest) -> Result<LaunchPlan, PlanError>;      // pure
pub async fn execute(gw: &dyn Gateway, host: &dyn Host, plan: &LaunchPlan, progress: &Sender<Progress>, cancel: CancellationToken) -> Result<SandboxRef, LaunchError>;
```

Idempotency key: `LaunchId`, written as label `jackin.launch-id` on the sandbox. A retry runs `find_by_label` first and resumes from the observed state instead of creating a second sandbox. Provider names derive from `AccountId` + `Agent`, so `ensure_provider` converges. Materialization writes a marker file `/sandbox/.jackin/materialized-<launch-id>` last; its presence skips re-upload. Hooks run once per launch-id (`setup-once`) via a marker, always for `preflight`.

Concurrency: one tokio runtime. The TUI owns its model (Elm loop: model/update/view/effects, as in the existing console). Effects spawn tasks that send `Msg` over a bounded mpsc to the UI loop. `Arc<dyn Gateway>` is shared immutable. Each launch has a `CancellationToken`; cancel leaves the sandbox in place (ADR-010) and reports it as a restore candidate. Attach suspends the TUI (leave alt screen, give the tty to the child), then restores it on child exit; no other task touches the terminal meanwhile.

Error model: typed enums in libraries (`PlanError`, `LaunchError`, `GatewayError`, `ConfigError`), `anyhow` only in `jackin` bin for context at the edge.

### 4.6 Role images (open, see Q2)

OpenShell pulls images; jackin does not build. The proposal that fits the "no Docker" rule: a role names an `ImageRef` built elsewhere (CI of the role author, or a jackin-published base with `codex` preinstalled). Optionally, `AgentSpec.install` runs at first start through gateway `exec` when the image lacks the binary, which needs a network-policy fragment allowing the installer host.

## 5. Which old crates seed which new crate

| New crate | Seeds (salvage fragments, not whole crates) |
|---|---|
| jackin | `apps/jackin`: `cli.rs` banner/help styling, `cli/{config,workspace,account,role,status,doctor}.rs` shapes, `preflight.rs`, `terminal_ownership.rs`, `lifecycle.rs` |
| jackin-domain | `jackin-core`: `selector.rs`, `workspace_name.rs`, `session_id.rs`, `env_value.rs`, `env_model.rs`, `op_reference.rs`, `op_cache.rs`, `path_text.rs`, `url_text.rs`, `clock.rs`, `worktree_dirty.rs`; `jackin-diagnostics`: `redact.rs`, `secret_scrub.rs` |
| jackin-agents | `jackin-core/src/agent/adapters/codex.rs` (state paths, env names, version parsing, install command), `jackin-runtime-launch-trust` (Codex project-trust seeding), `jackin-instance-agents` (Codex credential file location) |
| jackin-config | `jackin-config`: `persist.rs` (atomic_write, read lock), `editor.rs` pattern, `paths.rs`, `mounts.rs` spec parsing (for upload sources), `accounts/discovery.rs` for the Codex login; `jackin-manifest` validation style |
| jackin-openshell | nothing; new. Reference: `docs/openshell-v2/CONTRACTS.md` C-series for contracts worth re-reading |
| jackin-session | concept only: `jackin-runtime`, `launch-plan`, `launch-dry-run`, `launch-restore`, `launch-programmatic-selection`, `launch-progress-helpers` (StepCounter), `launch-git-pull`; `jackin-isolation` `ref_transaction.rs` and `worktree_dirty` as reference for retrieve-work |
| jackin-host | `jackin-host`: `caffeinate.rs`, `host_desktop.rs`; `jackin-env`: `op_cli.rs`, `resolve.rs`, `op_runner.rs`; `jackin-process` reduced to a ~200 LoC tokio runner; `jackin-runtime-identity`: GitIdentity capture |
| jackin-tui | `jackin-tui` (all 792 LoC), `jackin-brand` (all 44), `jackin-launch`: `rain.rs`, `chrome.rs`, `progress_rail.rs`, `failure_dialog.rs`, `footer.rs`, `header.rs`; `jackin-oppicker` if 1Password stays |
| jackin-console | `jackin-console/src/tui`: `layout`, `keymap`, `focus`, `modal_chain`, `dialog_layout`, `scroll_block`, `list_geometry`, `components/{dialogs,confirm_save,error_popup,status_popup,spinner,footer_hints,keyboard_help,file_browser,op_picker,role_picker,account_picker,agent_choice,editor_rows}`, `screens/{workspaces,editor,settings}`, `file_browser`, `services/config_save`. Drop `screens/usage`, `screens/workflows`, `auth_panel`, `mount_*`, `container_info`, docker settings |
| jackin-testkit | `jackin-test-support`: `fixture_http.rs`, `fixture_process.rs`, `time.rs`, `snapshot.rs`, `seed.rs` |
| xtask | `jackin-xtask`: `ci.rs`, `ratchet.rs`, `lint.rs`, `layout_lint.rs`, `arch.rs`, `suppressions.rs`, `report.rs`; REUSE check |

External dependencies that stay as the TUI base: `ratatui 0.30`, `crossterm 0.29`, `termrock` (git, exact rev) and `termpane` (only capsule used termpane; drop). `bollard`, `rusqlite`, `turso` (and `vendor/branches-0.4.6`, a patch only for turso), `portable-pty`, `tonic` server pieces, OTel crates, `sigstore-*`, `boltffi`, `dockerfile-parser-rs`, `pklr` leave. `tonic`/`prost` return transitively through `openshell-sdk`.


## 6. Custom lint and policy tooling after the rewrite

Verified from `Cargo.toml` `[workspace.lints]`, `clippy.toml`, `ratchet.toml` (1471 lines), `container-path-allowlist.toml`, `code-health-baseline.toml`, `crates/tools/jackin-lints`, `crates/tools/jackin-xtask/src/{arch,container_paths_gate,layout_lint,ratchet}.rs`.

| Item | Today | After rewrite |
|---|---|---|
| `[workspace.lints]` clippy all=deny, pedantic/cargo=warn, deny of unwrap, expect, panic, todo, print_*, dbg, allow_attributes, mod_module_files, await_holding_lock, undocumented_unsafe_blocks | Workspace-wide | KEEP unchanged. They are runtime-neutral and match AGENTS.md. |
| `unsafe_code` deny with exceptions in `jackin-process-directory` and capsule `process_isolation` | Two FFI-ish crates | Both crates are deleted. Use `forbid(unsafe_code)` workspace-wide; no allowlist. |
| `clippy.toml` disallowed_methods (`Command::output`, `thread::sleep`, `File::open`, `OpenOptions::open`) | 88 suppressions in baseline | KEEP; they still protect the TUI render path and tokio threads. Expect far fewer suppressions because the new subprocess surface is one runner in `jackin-host`. Add a `cargo deny` ban on `bollard`/`docker*` crates to keep Docker out structurally. |
| `jackin-lints` `render_thread_purity` (dylint, nightly, excluded from workspace, "pilot only", no required lane) | One lint | KEEP; still meaningful for ratatui `render`/`compose` functions. Decide whether to promote it to a required lane (Q9). |
| `ratchet.toml` families: file-size-production (cap 1850), test-layout, bare-allow-per-crate, expect-per-lint-crate, agent-doc-bytes, perf, public-surface, build-time, rust-function-complexity | Hundreds of per-crate entries naming deleted crates (jackin-capsule, usage-provider-*, jackin-launch casts, ...) | Keep the mechanism and the families file-size-production, test-layout, bare-allow, expect-per-lint, public-surface, agent-doc-bytes. Regenerate from zero (bounds at current counts, which are near 0 in new code). Consider a lower file cap than 1850. Drop perf/build-time rows tied to capsule and telemetry unless re-measured. |
| `container-path-allowlist.toml` + `container_paths_gate.rs` + `jackin-core/src/container_paths.rs` | Container-path literals only in one module; exceptions for `jackin-image` Dockerfile templates, capsule `/etc/sudoers.d`, docker shell scrubber | Idea survives as "sandbox path literals only in `jackin-domain::sandbox_paths`" (`/sandbox`, `/sandbox/project`, `/sandbox/.jackin`). Re-point the gate; the allowlist starts empty because every current exception lives in deleted code. The `check_turso_sole_owner` part of that gate is deleted. |
| `code-health-baseline.toml` (187 lines: suppression counts `expect_attrs = 517`, per-lint counts, forbidden-roots exceptions) | Baseline of existing debt | Delete and regenerate; new code should baseline near 0. Forbidden-roots list keeps its purpose (reject `/etc`, `/var` literals) but every exception is in deleted files. |
| `xtask arch` TIERS table (hand-listed tiers for ~140 crates) | Maintenance burden | Replace with a layer rule computed from `cargo metadata` over 11 crates (section 4.1). |
| `telemetry_registry`, `weaver` in `mise.toml`, `telemetry_bench` | Typed OTel event registry gate | DELETE unless Q7 keeps OTLP. |
| `xtask desktop`, `boltffi`, `swiftlint`, `xcodegen`, `periphery` in `mise.toml`, swiftlint/swift-format steps in `hk.pkl` | macOS app pipeline | DELETE with `native/` (Q5). |
| `xtask construct`, `docker/` dir, `docker-bake.hcl`, `fixture_omp`, `release_sbom` Docker parts | Image tooling | DELETE. |
| `deny.toml` (openssl banned, 146 lines), `rustfmt.toml`, REUSE.toml, hk gitleaks/rumdl/jactionlint, nextest-only rule, no-comments rule | Repo hygiene | KEEP. |
| `fuzz/{jackin-config,jackin-env,jackin-manifest,jackin-protocol}` | Parser fuzzing | KEEP config, env and role manifest; delete protocol. |
| `vendor/branches-0.4.6` | Patch for turso | DELETE with turso. |
| `.github/workflows/*` | Generated by Velnor from `.velnor/config.toml` (per `.github/AGENTS.md`) | Do not hand-edit. Update the generator config upstream; the Docker e2e partition and Swift groups drop out. |

## 7. `native/` (Swift macOS app) and Rust

Verified:

- `native/Package.swift` declares `.binaryTarget(name: "JackinUsageFFI", path: "../target/xcframework/JackinUsage.xcframework")`; `native/project.yml` lines 43, 119, 135 link the same XCFramework; `native/Scripts/VisualQA/capture-final-matrix.sh:25` references `crates/jackin-usage-ffi`.
- The XCFramework is built by `jackin-xtask desktop xcframework` (`crates/tools/jackin-xtask/src/desktop.rs:108`) using `boltffi` pinned in `mise.toml`.
- `jackin-usage-ffi` (boltffi static/cdylib) depends on `jackin-core`, `jackin-usage`, `jackin-usage-provider-core`, `jackin-protocol`. `jackin-usage` fans out to 16 provider crates plus coordinator, store, token monitor. The transitive workspace closure of `jackin-usage-ffi` is 39 crates, about 105k LoC.
- The Swift app has no other Rust dependency. It has no knowledge of Docker, capsule or sandboxes.

Consequence: dropping usage deletes the app's whole data source. Options: (a) delete `native/` now and recreate later; (b) freeze `native/` out of the workspace and CI until a Codex-only `jackin-usage` crate exists (seeds: `jackin-usage-provider-codex` 1.4k LoC, `-provider-core` 5k LoC; the app's six other providers vanish); (c) keep the whole usage stack. I recommend (a) or (b), decided in Q5. ADR-011 must be superseded or amended either way.

## 8. Full crate inventory

LoC = `wc -l` over `src/**/*.rs` (inline test modules included); `(+n)` is `tests/` and `benches/` LoC. "Workspace deps" lists the first six normal dependencies on other workspace crates, with the `jackin-` prefix removed. Verdicts follow the Codex-only scope; every Grok item is DELETE. `jackin-runtime-*` micro-crates (81) are almost all DELETE because they are Docker, Apple container, capsule, or image plumbing; the 12 that map to surviving concepts are marked REWRITE with the reason.

| Path | Crate | LoC src (+tests/) | Workspace deps | Verdict | Purpose / note |
|---|---|---|---|---|---|
| adapters/jackin-docker | jackin-docker | 1999 | core,diagnostics,process,process-directory,telemetry | DELETE | bollard Docker client and shell runner for docker CLI (build, run, exec, inspect). Docker is gone. |
| adapters/jackin-image | jackin-image | 5490 | core,diagnostics,docker,manifest,process,telemetry | DELETE | Role image build: derived Dockerfile generation, agent binary download and release evidence, capsule binary, buildx recipe/decision. OpenShell does not build Dockerfiles; images are pulled. |
| adapters/jackin-omp-store | jackin-omp-store | 1816 |  | DELETE | rusqlite reader for omp agent account store (WAL capture, query). omp agent removed. |
| adapters/jackin-oppicker | jackin-oppicker | 2700 | core,telemetry | REWRITE | Ratatui/termrock 1Password picker (vault, item, field) used to choose op:// references. Salvage as TUI widget if 1Password refs stay (see Q6). Depends on jackin-core and telemetry only. |
| adapters/jackin-usage-ffi | jackin-usage-ffi | 5088 | core,protocol,usage,usage-provider-core | DELETE | boltffi static/cdylib facade over usage host runtime for the macOS menu-bar app. Only consumer is native/. Delete with native/ or rebuild later on a Codex+Grok-only usage crate. |
| apps/jackin | jackin | 21094 (+39) | brand,config,console,core,diagnostics,docker (+19) | REWRITE | Operator binary: clap CLI (load, hardline, eject, exile, purge, prewarm, prune, console, role, workspace, account, config, daemon, doctor, diagnostics, status, workflow, usage, help) plus console wiring. Seed for new `jackin`: banner/help styles in cli.rs, doctor/preflight shape, config/workspace/account subcommand structure, terminal_ownership. Everything Docker, daemon, workflow, usage, prune goes. |
| apps/jackin-capsule | jackin-capsule | 31709 (+263) | agent-status,brand,core,diagnostics,process,process-directory (+5) | DELETE | In-container PID1/daemon: PTY multiplexer with tabs/panes, status chrome, firewall, sudo provisioning, Landlock/process isolation, agent-status hooks, MCP server, usage relay proxy, file export, clipboard. OpenShell supplies PTY retention (main process reattach with 1 MiB replay), isolation and network policy. See open question on multi-session UX. |
| core/jackin-brand | jackin-brand | 44 |  | KEEP-PURE | Brand RGB palette constants (phosphor green, rain, menu colors). Move into jackin-tui as tokens. |
| core/jackin-build-meta | jackin-build-meta | 87 | process | KEEP-PURE | Build-script helper deriving version string from git short SHA. Keep or replace with `vergen`-style; tiny. |
| core/jackin-config | jackin-config | 10004 (+69) | core,omp-store,telemetry | REWRITE | TOML config: accounts, account bindings, agent configurations, roles, workspaces (mounts, workdir, allowed roles, env), docker, runtime, telemetry; migrations; ConfigEditor; zshrc account import; discovery. Salvage: atomic_write, read lock, toml_edit editor pattern, path expansion, mount spec parsing (for upload sources), account discovery for ~/.codex. Drop migrations, docker, runtime backend, telemetry, 11 of 12 agents. |
| core/jackin-core | jackin-core | 6124 |  | REWRITE | Shared domain: Agent enum (12 agents) + AgentRuntime registry, RoleManifest, selectors, env values/op refs, instance/session records, isolation, docker security, container ids/paths, profile credential fingerprints, worktree dirty probe. Split: selector, workspace_name, session_id, env_value/env_model/op_reference/op_cache, path_text, url_text, clock, worktree_dirty survive as seeds of jackin-domain. agent.rs becomes a one-variant enum (Codex) plus AgentSpec table. docker*, container_*, claude_keychain, isolation*, instance, profile_material_v2 go. |
| core/jackin-env | jackin-env | 3206 | config,core,diagnostics,process,protocol,telemetry | REWRITE | Env var resolution: op:// via `op` CLI, host env, literal; layered env (global/workspace/role); account env; picker. Salvage resolve.rs/op_cli.rs/env_layer into jackin-host secrets module (if 1Password stays). |
| core/jackin-manifest | jackin-manifest | 676 (+38) | config,core | REWRITE | Role repo manifest loading/validation (jackin.role.toml, Dockerfile contract, repo layout checks). New manifest has no Dockerfile; schema is image ref + agents + hooks + env + policy fragments. Salvage validate.rs style only. |
| core/jackin-process | jackin-process | 2822 |  | REWRITE | Subprocess runner: capture limits, retry, spooled output, child reaping registry, selective reaping. Replace with ~200 LoC tokio::process helper (timeout, capture cap, stdio mode). Child-reaper machinery served capsule/PID1. |
| core/jackin-process-directory | jackin-process-directory | 2403 |  | DELETE | Secure directory install with ACLs, macOS wait, running-image checks, sealed roots. Served Docker/capsule binary install. |
| core/jackin-protocol | jackin-protocol | 7879 | core,telemetry | DELETE | Wire types: capsule attach protocol, control protocol, capsule transport, usage broker, usage monitor, quota. Capsule/usage wire formats. Quota enum types may seed a later usage crate. |
| core/jackin-workflow-spec | jackin-workflow-spec | 2276 | process | DELETE | Pkl-driven repair-workflow spec: rules, eval, resolve, digest, store. Defer; verification-worker workflows are a separate product line. |
| services/jackin-agent-status | jackin-agent-status | 2229 | core,protocol | UNSURE | Agent state inference (working/waiting/idle) from OSC sequences, process tree and hooks; per-agent rules. Used only by capsule. Question: does the console show per-session agent state in v1? Without capsule there is no PTY stream to parse unless attach goes through jackin. |
| services/jackin-diagnostics | jackin-diagnostics | 5752 | core,telemetry | REWRITE | Run records, stage/operation tracking, logging setup, OTel bridge, secret scrubbing and redaction. Salvage redact.rs and secret_scrub.rs into jackin-domain; logging into jackin binary via tracing-subscriber. |
| services/jackin-host | jackin-host | 1038 | core,diagnostics,docker,process,protocol,telemetry | REWRITE | Host affordances: caffeinate keep-awake, clipboard image bridge, open-URL, universe/naming shims. Salvage caffeinate.rs and host_desktop.rs into jackin-host. host_clipboard needs a capsule-like peer; drop for v1. |
| services/jackin-instance | jackin-instance | 809 | config,core,instance-agents,instance-credentials,instance-roles | DELETE | Instance naming and manifest glue. Instance = OpenShell sandbox; names are canonical sandbox names. |
| services/jackin-instance-agents | jackin-instance-agents | 4118 | config,core,instance-credentials,process,telemetry | REWRITE | Per-agent auth handoff and state capture: agent slots, snapshots, single-file auth, validation, Claude specifics. Replaced by OpenShell providers. Salvage only the knowledge of credential file locations for codex (~/.codex/auth.json) into jackin-agents. |
| services/jackin-instance-credentials | jackin-instance-credentials | 2349 | config | DELETE | Auth directory permissions and mount outcomes for container handoff. Providers replace file handoff. |
| services/jackin-instance-roles | jackin-instance-roles | 732 | config,core,diagnostics,instance-agents,instance-credentials,manifest (+1) | DELETE | Host-side role state preparation and provisioning for containers |
| services/jackin-isolation | jackin-isolation | 7104 | config,core,diagnostics,docker,instance,process (+2) | DELETE | Mount isolation (shared/worktree/clone): materialize, ref transactions, capture, finalize, cleanup, backup, safe remove. OpenShell has no bind mounts. Materialization is upload or in-sandbox git clone. ref_transaction/worktree_dirty are reference for retrieve-work logic. |
| services/jackin-runtime | jackin-runtime | 9346 (+272) | config,core,diagnostics,docker,env,host (+82) | REWRITE | Launch/attach/eject orchestrator that depends on 81 micro-crates: load pipeline, image, DinD, capsule, attach, cleanup. Concept survives as jackin-session. No code survives intact. |
| services/jackin-runtime-apple-container-activate-entry | jackin-runtime-apple-container-activate-entry | 22 | runtime-universe-claims | DELETE | Apple container: entry claim activation |
| services/jackin-runtime-apple-container-attach | jackin-runtime-apple-container-attach | 35 | core,diagnostics,process,runtime-identity,runtime-process-telemetry | DELETE | Apple container: attach via `container exec` |
| services/jackin-runtime-apple-container-attach-outcome | jackin-runtime-apple-container-attach-outcome | 205 | core,runtime-apple-container-client,runtime-apple-container-running,runtime-isolation,runtime-launch-attach-outcome,telemetry | DELETE | Apple container: attach result classification |
| services/jackin-runtime-apple-container-check-dns | jackin-runtime-apple-container-check-dns | 38 | process,runtime-identity,runtime-process-telemetry | DELETE | Apple container: DNS check |
| services/jackin-runtime-apple-container-client | jackin-runtime-apple-container-client | 571 | process,runtime-identity,runtime-process-telemetry | DELETE | Apple `container` CLI client |
| services/jackin-runtime-apple-container-probe-version | jackin-runtime-apple-container-probe-version | 17 | process,runtime-process-telemetry | DELETE | Apple container: version probe |
| services/jackin-runtime-apple-container-running | jackin-runtime-apple-container-running | 29 | runtime-apple-container-client | DELETE | Apple container: running check |
| services/jackin-runtime-apple-container-session-contract | jackin-runtime-apple-container-session-contract | 54 | runtime-apple-container-client | DELETE | Apple container: session contract |
| services/jackin-runtime-apple-container-stop | jackin-runtime-apple-container-stop | 12 | runtime-apple-container-client | DELETE | Apple container: stop |
| services/jackin-runtime-apple-container-supervisor-env | jackin-runtime-apple-container-supervisor-env | 17 | protocol | DELETE | Apple container: supervisor env |
| services/jackin-runtime-apple-container-wait | jackin-runtime-apple-container-wait | 49 | process,protocol,runtime-identity,runtime-process-telemetry | DELETE | Apple container: wait |
| services/jackin-runtime-attach-admission | jackin-runtime-attach-admission | 266 | config,core,instance,runtime-launch-account-identity | DELETE | Docker attach admission (account/generation checks before reconnect) |
| services/jackin-runtime-attach-capsule-ready | jackin-runtime-attach-capsule-ready | 160 | core,diagnostics,docker,protocol,runtime-snapshot,runtime-spin-wait (+1) | DELETE | Wait for capsule socket readiness via docker exec |
| services/jackin-runtime-attach-exec-args | jackin-runtime-attach-exec-args | 42 | core,diagnostics,instance,telemetry | DELETE | Build docker exec args for capsule attach |
| services/jackin-runtime-attach-inspect | jackin-runtime-attach-inspect | 197 | core,docker,instance,isolation,runtime-attach-admission,runtime-attach-sessions | DELETE | Docker inspect of instance for attach |
| services/jackin-runtime-attach-running-gate | jackin-runtime-attach-running-gate | 46 | core,docker,runtime-attach-admission,runtime-attach-inspect,runtime-attach-sessions | DELETE | Gate attach on running container state |
| services/jackin-runtime-attach-sessions | jackin-runtime-attach-sessions | 92 | core,docker,protocol | DELETE | List capsule sessions via docker exec |
| services/jackin-runtime-attach-transport | jackin-runtime-attach-transport | 72 | core,runtime-attach-capsule-ready,runtime-snapshot,telemetry | DELETE | Attach transport selection (docker exec vs socket) |
| services/jackin-runtime-backend-selection | jackin-runtime-backend-selection | 25 | core,instance | DELETE | Pick docker vs apple backend |
| services/jackin-runtime-cleanup-absent-guards | jackin-runtime-cleanup-absent-guards | 42 | docker,instance | DELETE | Eject guards for absent containers |
| services/jackin-runtime-cleanup-dind-gc | jackin-runtime-cleanup-dind-gc | 284 | core,docker,instance,runtime-cleanup-timing,runtime-discovery,runtime-launch-dind (+1) | DELETE | DinD sidecar garbage collection |
| services/jackin-runtime-cleanup-eject-resources | jackin-runtime-cleanup-eject-resources | 52 | core,docker,instance,runtime-cleanup-socket-dir | DELETE | Remove container resources on eject |
| services/jackin-runtime-cleanup-prune-cache | jackin-runtime-cleanup-prune-cache | 17 | core,runtime-cleanup-prune-dir,runtime-coordination | DELETE | Prune cache dirs |
| services/jackin-runtime-cleanup-prune-dir | jackin-runtime-cleanup-prune-dir | 29 | runtime-cleanup-timing,runtime-isolation,runtime-prune-output | DELETE | Generic dir prune |
| services/jackin-runtime-cleanup-prune-home | jackin-runtime-cleanup-prune-home | 25 | core,runtime-cleanup-timing,runtime-coordination,runtime-isolation,runtime-prune-output | DELETE | Prune durable homes |
| services/jackin-runtime-cleanup-prune-images | jackin-runtime-cleanup-prune-images | 89 | docker,runtime-cleanup-timing,runtime-naming,runtime-prune-output | DELETE | Prune role images |
| services/jackin-runtime-cleanup-prune-roles | jackin-runtime-cleanup-prune-roles | 17 | core,runtime-cleanup-prune-dir,runtime-coordination | DELETE | Prune role repo checkouts |
| services/jackin-runtime-cleanup-resolve | jackin-runtime-cleanup-resolve | 144 | core,docker,instance | DELETE | Resolve eject/purge targets from docker state |
| services/jackin-runtime-cleanup-socket-dir | jackin-runtime-cleanup-socket-dir | 32 | core,runtime-coordination,runtime-isolation,telemetry | DELETE | Remove capsule socket dirs |
| services/jackin-runtime-cleanup-timing | jackin-runtime-cleanup-timing | 32 | diagnostics,telemetry | DELETE | Timing events for cleanup |
| services/jackin-runtime-coordination | jackin-runtime-coordination | 701 | core,telemetry | DELETE | Host filesystem lock namespace under the jackin data root |
| services/jackin-runtime-discovery | jackin-runtime-discovery | 49 | docker,runtime-naming | DELETE | Docker container discovery by labels |
| services/jackin-runtime-docker-profile | jackin-runtime-docker-profile | 1249 | core | DELETE | Docker security profiles and grants (caps, seccomp, hosts) |
| services/jackin-runtime-drift | jackin-runtime-drift | 44 | config,core,docker,runtime-discovery,runtime-isolation | DELETE | Detect workspace edit drift vs running container |
| services/jackin-runtime-exec-host | jackin-runtime-exec-host | 422 | env,process,protocol,runtime-process-telemetry,telemetry | DELETE | Run capsule exec on host for a container |
| services/jackin-runtime-exit-summary | jackin-runtime-exit-summary | 44 | instance | REWRITE | Format instance exit summary. Post-session summary (dirty/unpushed) survives conceptually. |
| services/jackin-runtime-host-attach | jackin-runtime-host-attach | 1382 | core,diagnostics,host,process,protocol,runtime-attach-transport (+3) | DELETE | Host-side attach session runner with host affordances |
| services/jackin-runtime-host-daemon | jackin-runtime-host-daemon | 1803 | core,diagnostics,process,protocol,runtime-process-telemetry,telemetry (+2) | DELETE | Per-user host daemon protocol and server (socket, pid file) |
| services/jackin-runtime-identity | jackin-runtime-identity | 86 | core,diagnostics | REWRITE | Host uid and git identity capture; capsule supervisor user. Keep GitIdentity capture (user.name/email) in jackin-host. |
| services/jackin-runtime-image | jackin-runtime-image | 2140 | core,diagnostics,docker,image,launch,manifest (+6) | DELETE | Role image resolution and build orchestration |
| services/jackin-runtime-isolation | jackin-runtime-isolation | 29 | core,isolation | DELETE | Thin wrapper over jackin-isolation |
| services/jackin-runtime-launch-account-config | jackin-runtime-launch-account-config | 1242 | config,core,instance | DELETE | Configure per-agent account files inside durable home |
| services/jackin-runtime-launch-account-identity | jackin-runtime-launch-account-identity | 3075 | config,core,docker,instance,protocol | DELETE | Account generation leases and config revision fingerprints for containers |
| services/jackin-runtime-launch-attach-outcome | jackin-runtime-launch-attach-outcome | 57 | core,instance,runtime-isolation | DELETE | Classify attach outcome for restore |
| services/jackin-runtime-launch-auth-error | jackin-runtime-launch-auth-error | 49 |  | DELETE | Proxy env helpers and auth token source references |
| services/jackin-runtime-launch-capsule-setup | jackin-runtime-launch-capsule-setup | 650 | config,core,diagnostics,instance,manifest,protocol (+1) | DELETE | Compute capsule auth modes, instance auth bindings and models |
| services/jackin-runtime-launch-debug-envs | jackin-runtime-launch-debug-envs | 7 |  | DELETE | Debug env constants |
| services/jackin-runtime-launch-dind | jackin-runtime-launch-dind | 799 | core,diagnostics,docker,instance,runtime-attach-capsule-ready,runtime-cleanup-resolve (+5) | DELETE | Docker-in-Docker sidecar launch |
| services/jackin-runtime-launch-dry-run | jackin-runtime-launch-dry-run | 117 | config,core,runtime-launch-capsule-setup | REWRITE | Dry-run identity and model projection for `load --dry-run`. Concept survives: `jackin load --dry-run` prints a typed LaunchPlan. Rebuild on new plan. |
| services/jackin-runtime-launch-exit-diagnosis | jackin-runtime-launch-exit-diagnosis | 193 | core,diagnostics,docker,isolation,telemetry | DELETE | Diagnose premature container exit via docker |
| services/jackin-runtime-launch-exit-outro | jackin-runtime-launch-exit-outro | 55 | diagnostics,instance,runtime-exit-summary,runtime-progress,runtime-universe-claims | REWRITE | Render exit observation outro. Same. |
| services/jackin-runtime-launch-failure | jackin-runtime-launch-failure | 68 | config,core,diagnostics,runtime-progress | REWRITE | Launch failure titles and CLI error mapping. Fold error-to-title mapping into session error enum. |
| services/jackin-runtime-launch-git-pull | jackin-runtime-launch-git-pull | 172 | config,diagnostics,process,runtime-process-telemetry,telemetry | REWRITE | git pull workspace repos on entry. Concept survives (host-side git before upload). Subprocess git call is reusable. |
| services/jackin-runtime-launch-image-plan | jackin-runtime-launch-image-plan | 114 | config,core,docker,runtime-image,runtime-launch-capsule-setup,runtime-naming (+1) | DELETE | Image plan for launch |
| services/jackin-runtime-launch-load-cleanup | jackin-runtime-launch-load-cleanup | 246 | core,diagnostics,docker,runtime-progress,telemetry | DELETE | Clean up partial docker state on failed load |
| services/jackin-runtime-launch-load-options | jackin-runtime-launch-load-options | 294 | config,core,env,protocol,runtime-docker-profile,runtime-universe-claims | DELETE | LoadOptions and per-lane agent env |
| services/jackin-runtime-launch-mounts | jackin-runtime-launch-mounts | 751 | config,core,instance,isolation,protocol,runtime-apple-container-client | DELETE | Docker/Apple bind-mount list builder and provider-authority guards |
| services/jackin-runtime-launch-plan | jackin-runtime-launch-plan | 107 | diagnostics | DELETE | LaunchPlan enum + telemetry emit |
| services/jackin-runtime-launch-post-run-failure | jackin-runtime-launch-post-run-failure | 11 | diagnostics,telemetry | DELETE | Post-run failure telemetry shim |
| services/jackin-runtime-launch-programmatic-selection | jackin-runtime-launch-programmatic-selection | 238 | config,core | REWRITE | Apply account/configuration selection to load options. Account/agent selection flags for non-interactive load survive. |
| services/jackin-runtime-launch-progress-helpers | jackin-runtime-launch-progress-helpers | 338 | config,core,diagnostics,env,runtime-progress,telemetry | REWRITE | StepCounter and progress plumbing for launch. StepCounter idea reused by launch progress screen. |
| services/jackin-runtime-launch-restore | jackin-runtime-launch-restore | 1113 | config,core,diagnostics,docker,instance,runtime-attach-sessions (+6) | REWRITE | Restore-candidate discovery and recovery of un-cleanly terminated instances. Restore of crashed instance maps to OpenShell restart/resume of a sandbox; logic must be redone. |
| services/jackin-runtime-launch-run-envs | jackin-runtime-launch-run-envs | 9 | telemetry | DELETE | Run env constants |
| services/jackin-runtime-launch-sibling-auth-prewarm | jackin-runtime-launch-sibling-auth-prewarm | 223 | config,core,diagnostics,instance,manifest,runtime-launch-capsule-setup (+1) | DELETE | Prewarm auth of sibling instances |
| services/jackin-runtime-launch-slot | jackin-runtime-launch-slot | 291 | config,core,diagnostics,docker,env,instance (+3) | DELETE | Claim container names and GitHub env for a launch slot |
| services/jackin-runtime-launch-supported-agents | jackin-runtime-launch-supported-agents | 54 | config,core,manifest,runtime-repo-cache,telemetry | DELETE | Resolve supported agents for console from role manifest |
| services/jackin-runtime-launch-trust | jackin-runtime-launch-trust | 111 | config,core,instance | REWRITE | Mise trusted paths env and Codex project-trust seeding. Codex project-trust seeding survives in jackin-agents; mise trust path env is optional. |
| services/jackin-runtime-naming | jackin-runtime-naming | 44 | core,image,instance | DELETE | Docker label and container-name constants |
| services/jackin-runtime-prewarm-trigger | jackin-runtime-prewarm-trigger | 165 | config,core,docker,runtime-image,runtime-launch-dind,telemetry | DELETE | Background image and DinD sidecar prewarm |
| services/jackin-runtime-process-telemetry | jackin-runtime-process-telemetry | 133 | process,telemetry | DELETE | Telemetry wrapper for child processes |
| services/jackin-runtime-progress | jackin-runtime-progress | 169 | core,diagnostics,host,launch | REWRITE | Host terminal launch output, standalone prompts and dialogs. Standalone dialogs/launch output fold into jackin-tui. |
| services/jackin-runtime-prune-output | jackin-runtime-prune-output | 133 |  | DELETE | Prune CLI row output |
| services/jackin-runtime-reactive-daemon | jackin-runtime-reactive-daemon | 287 | core,protocol,telemetry | DELETE | Reactive daemon request/response protocol for attention notifications |
| services/jackin-runtime-repo-cache | jackin-runtime-repo-cache | 569 | core,diagnostics,instance,manifest,runtime-identity,telemetry | REWRITE | Role repo clone/refresh cache and URL normalization. Only if roles stay git-sourced (Q2). |
| services/jackin-runtime-session-control | jackin-runtime-session-control | 504 | core,diagnostics,process,protocol,runtime-identity,runtime-process-telemetry (+2) | DELETE | Send text/control to capsule sessions |
| services/jackin-runtime-shared-runner | jackin-runtime-shared-runner | 70 | core | DELETE | Shared command runner abstraction for docker CLI |
| services/jackin-runtime-snapshot | jackin-runtime-snapshot | 328 | core,diagnostics,process,protocol,runtime-identity,runtime-process-telemetry (+1) | DELETE | Fetch capsule snapshot and usage accounts over docker exec or socket |
| services/jackin-runtime-spin-wait | jackin-runtime-spin-wait | 114 | brand,diagnostics | DELETE | Async spin wait helpers |
| services/jackin-runtime-universe | jackin-runtime-universe | 454 | core,docker,runtime-coordination,runtime-discovery,runtime-universe-claims,telemetry | DELETE | Universe boundary intro/outro claims on enter/exit |
| services/jackin-runtime-universe-claims | jackin-runtime-universe-claims | 270 | core,runtime-coordination,telemetry | DELETE | Entry/exit claim records |
| services/jackin-runtime-usage-relay | jackin-runtime-usage-relay | 1079 | config,core,env,instance,process,protocol (+5) | DELETE | Host usage-relay mounts and guards into containers |
| services/jackin-telemetry | jackin-telemetry | 11069 (+44) |  | DELETE | OpenTelemetry registry: typed events, metrics, spans, identity, OTLP spawn/export, weaver-checked naming. Heavy. Use `tracing` only. Open question if OTLP export is wanted (Q7). |
| services/jackin-usage | jackin-usage | 162 (+87) | usage-capsule-telemetry,usage-coordinator,usage-credential-resolver,usage-host-runtime,usage-output,usage-provider-amp (+14) | DELETE | Aggregate facade over all usage providers, coordinator, snapshot store, token monitor |
| services/jackin-usage-broker | jackin-usage-broker | 16238 (+359) | config,core,protocol,telemetry,usage-broker-publish,usage-broker-wire (+7) | DELETE | Host usage broker: monitors, evaluation, spend, legacy. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-broker-publish | jackin-usage-broker-publish | 2431 | core,protocol,usage-coordinator,usage-provider-core | DELETE | Broker publish projections. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-broker-wire | jackin-usage-broker-wire | 312 | core,protocol,telemetry,usage-coordinator | DELETE | Broker wire format. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-capsule-telemetry | jackin-usage-capsule-telemetry | 197 | diagnostics,telemetry | DELETE | Capsule usage telemetry. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-coordinator | jackin-usage-coordinator | 3686 | core,protocol,telemetry | DELETE | Refresh coordination state machine across providers. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-credential-resolver | jackin-usage-credential-resolver | 450 | config,core,protocol,usage-credential-snapshots,usage-host-credentials,usage-host-presentation (+10) | DELETE | Resolve credentials per provider from accounts. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-credential-snapshots | jackin-usage-credential-snapshots | 126 | core,protocol,usage-provider-core | DELETE | Credential snapshot types. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-destination | jackin-usage-destination | 75 | protocol | DELETE | Usage destination routing. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-discovery | jackin-usage-discovery | 2419 | config,core,protocol,usage-host-accounts,usage-host-credentials,usage-host-presentation (+10) | DELETE | Discover host accounts per provider. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-host-accounts | jackin-usage-host-accounts | 3119 | core,protocol,usage-host-presentation,usage-provider-core | DELETE | Host account inventory and dedup. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-host-credentials | jackin-usage-host-credentials | 163 | config,core,protocol,usage-host-presentation,usage-provider-core | DELETE | Host credential access. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-host-glance | jackin-usage-host-glance | 376 | protocol,usage-host-accounts,usage-host-presentation,usage-provider-core | DELETE | Compact glance rendering. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-host-presentation | jackin-usage-host-presentation | 322 | core,protocol | DELETE | Presentation DTOs for usage. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-host-runtime | jackin-usage-host-runtime | 460 | protocol,usage-broker,usage-discovery,usage-host-presentation,usage-provider-core | DELETE | Host runtime for the standard broker (used by FFI). Usage infrastructure; not in v1 scope. |
| services/jackin-usage-output | jackin-usage-output | 22 |  | DELETE | Usage CLI output helpers. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-projection | jackin-usage-projection | 373 | protocol,usage-broker-publish,usage-destination,usage-discovery,usage-host-accounts,usage-host-presentation | DELETE | Projection from broker state to UI. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-provider-amp | jackin-usage-provider-amp | 828 | protocol,telemetry,usage-provider-core | DELETE | Amp usage probe. Removed agent/provider. |
| services/jackin-usage-provider-antigravity | jackin-usage-provider-antigravity | 870 | protocol,usage-provider-core | DELETE | Antigravity usage probe. Removed agent/provider. |
| services/jackin-usage-provider-claude | jackin-usage-provider-claude | 2271 | core,protocol,telemetry,usage-provider-core | DELETE | Claude usage probe. Removed agent/provider. |
| services/jackin-usage-provider-codex | jackin-usage-provider-codex | 1415 | protocol,telemetry,usage-provider-core | UNSURE | Codex rate-limit windows via ChatGPT backend RPC + OAuth refresh. Pure HTTP+parsing; reuse if usage is in scope (Q5). |
| services/jackin-usage-provider-core | jackin-usage-provider-core | 4984 (+45) | core,protocol,telemetry | UNSURE | Quota model, refresh/cache, presentation formatting, HTTP helpers shared by providers. Seed for a later single `jackin-usage` crate if limits display is in v1 (Q5). |
| services/jackin-usage-provider-cursor | jackin-usage-provider-cursor | 1371 | protocol,usage-provider-core | DELETE | Cursor usage probe. Removed agent/provider. |
| services/jackin-usage-provider-gemini | jackin-usage-provider-gemini | 335 | protocol,usage-provider-core | DELETE | Gemini usage probe. Removed agent/provider. |
| services/jackin-usage-provider-grok | jackin-usage-provider-grok | 1183 | protocol,telemetry,usage-provider-core | DELETE | Grok/xAI billing and rate-limit RPC + views. Grok dropped (scope change: Codex only). |
| services/jackin-usage-provider-hermes | jackin-usage-provider-hermes | 194 | protocol,usage-provider-core | DELETE | Hermes usage probe. Removed agent/provider. |
| services/jackin-usage-provider-kimi | jackin-usage-provider-kimi | 1065 | protocol,telemetry,usage-provider-core | DELETE | Kimi usage probe. Removed agent/provider. |
| services/jackin-usage-provider-minimax | jackin-usage-provider-minimax | 1024 | protocol,telemetry,usage-provider-core | DELETE | MiniMax usage probe. Removed agent/provider. |
| services/jackin-usage-provider-muse | jackin-usage-provider-muse | 369 | protocol,usage-provider-core | DELETE | Muse usage probe. Removed agent/provider. |
| services/jackin-usage-provider-opencode | jackin-usage-provider-opencode | 461 | protocol,usage-provider-core | DELETE | OpenCode usage probe. Removed agent/provider. |
| services/jackin-usage-provider-openrouter | jackin-usage-provider-openrouter | 2997 | protocol,telemetry,usage-provider-core | DELETE | OpenRouter usage probe. Removed agent/provider. |
| services/jackin-usage-provider-zai | jackin-usage-provider-zai | 539 | protocol,telemetry,usage-provider-core | DELETE | Z.ai usage probe. Removed agent/provider. |
| services/jackin-usage-snapshot-store | jackin-usage-snapshot-store | 1998 | core,diagnostics,protocol,telemetry,usage-provider-core,usage-store-backend | DELETE | Snapshot persistence on turso. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-store-backend | jackin-usage-store-backend | 4091 | telemetry | DELETE | Turso local store: backup, registry, forensics. Usage infrastructure; not in v1 scope. |
| services/jackin-usage-token-monitor | jackin-usage-token-monitor | 912 | core,protocol,telemetry,usage-store-backend | DELETE | Token spend monitor. Usage infrastructure; not in v1 scope. |
| services/jackin-workflow | jackin-workflow | 6134 | core,telemetry,usage-store-backend,workflow-spec | DELETE | Durable agent repair-workflow engine: run store, reducer, effects, test/review workers, delivery gate. Defer. If rebuilt, each worker is an OpenShell sandbox. |
| testing/jackin-test-support | jackin-test-support | 1358 | core,manifest,telemetry | REWRITE | Test fixtures: docker harness, HTTP fixture, process fixture, time, snapshot, seed. Salvage fixture_http, fixture_process, time; drop docker.rs. Becomes jackin-testkit with fake OpenShell gateway. |
| tools/jackin-dev | jackin-dev | 977 |  | DELETE | `jackin-dev pr` worktree sync tooling referencing capsule path deps and construct registry. Repo-dev tool tied to old layout; redo in xtask if wanted. |
| tools/jackin-pr-trailers | jackin-pr-trailers | 303 | process | KEEP-PURE | CLI that extracts git trailers from PR commits into squash-merge bodies. Independent of runtime; depends on jackin-process only (swap to std::process). |
| tools/jackin-xtask | jackin-xtask | 43191 (+2255) | process,process-directory | REWRITE | Dev/CI orchestrator: ci partitions, lint gates (ratchet, arch tiers, container paths, test layout, agent files), docs, telemetry registry, desktop build/sign/notarize, release packaging, precommit snapshot, CI evidence. Shrink ~90%: keep ci driver + ratchet + file-size + test-layout + REUSE; drop desktop, telemetry registry, construct, fixture_omp, container paths, research, CI evidence ledger. |
| ui/jackin-console | jackin-console | 53551 | config,core,env,oppicker,protocol,telemetry (+3) | REWRITE | Operator console TUI (53k LoC): workspaces manager/editor, roles, accounts/auth panels, env + op picker, mounts/file browser, settings, usage screen, workflows screen, launch flow. Salvage list in report. Big shrink: drop usage, workflows, docker settings, mount isolation, auth modes UI. |
| ui/jackin-launch | jackin-launch | 6261 | brand,core,diagnostics,telemetry,tui | REWRITE | Launch-progress TUI: matrix rain, progress rail, build-log dialog, failure dialog, footer/header chrome. Keep rain, chrome, progress_rail, failure_dialog; drop build_log (no image build) and container_info. |
| ui/jackin-tui | jackin-tui | 792 | brand | KEEP-PURE | TUI foundations: tokens, operator info, runtime focus handling over termrock. Tiny; absorbs jackin-brand and launch chrome. |
| tools/jackin-lints | jackin-lints | 283 |  | KEEP-PURE | Dylint library: render_thread_purity (flags blocking I/O reachable from render fns); excluded from workspace, nightly. Still valid for TUI. Pilot only; no required lane today. |

## 9. Alternatives rejected

- Keep Capsule as a slim resident multiplexer inside the sandbox (SPEC v2). Rejected for v1: OpenShell already retains the main-process PTY with replay and reattach; Capsule is 31.7k LoC and needs relay-over-exec plus protocol versioning. Revisit only if tabs/panes are a must-have (Q1).
- Attach through the SDK (`ExecSandboxInteractive` plus crossterm raw mode) instead of spawning `openshell sandbox connect`. Rejected for v1: reattach, replay, stdin ownership and 60 s reconnect are implemented in the OpenShell CLI over SSH, and the SDK curated API does not wrap them. Re-implementing duplicates a moving target.
- Drive everything by shelling out to the `openshell` CLI and drop the SDK. Rejected: parsing human output is brittle; the typed `Gateway` trait is the seam that lets the fake gateway exist. The CLI is still used for attach only.
- Keep a local instance index (JSON or SQLite) like `InstanceIndexEntry`. Rejected: two sources of truth that drift; labels on the sandbox plus `find_by_label` give the same lookup with OpenShell as owner.
- One crate per old concept (about 40 small crates). Rejected: this is the failure mode of the current tree (90 `jackin-runtime-*` micro-crates with a hand-kept tier table).
- Single crate. Allowed by ADR-001, rejected for lacking compiler-enforced separation of pure domain from I/O; the 5-crate variant is the fallback.
- Keep Docker/Buildx image building (SPEC v2). Rejected by the user goal.
- Emulate host bind mounts via a sync daemon. Rejected: needs a resident host service and conflict handling; upload plus git is explicit and matches OpenShell's model.

## 10. Sequencing (each unit ends verifiable)

Gate for every unit: `cargo nextest run -p <crate>` and `cargo clippy -p <crate> --all-targets --locked`; from unit 2 on also the repo fast gate (`xtask ci --fast` once rewritten; until then `cargo clippy --workspace --locked` and `cargo deny --locked check`). The brief's `mise exec -- mbx +<toolchain>` wrapper applies to all cargo calls.

1. Subtract. In one change delete the 117 DELETE crates, `docker/`, `docker-bake.hcl`, `vendor/`, `native/` (per Q5), `fuzz/jackin-protocol`, and prune `Cargo.toml` members, `[workspace.dependencies]`, `mise.toml` tools, `hk.pkl` Swift steps. Delete all tests and all comments except SPDX and clap help. Workspace will not build until unit 2; do both in one branch. Gate: none standalone.
2. Skeleton. Create the 11 crates with empty public surfaces, workspace lints, `forbid(unsafe_code)`, the layer gate in a minimal xtask. Gate: `cargo check --workspace --locked`, `xtask lint --strict`, `cargo deny check`.
3. `jackin-domain` + `jackin-agents`: ids, selectors, `EnvValue`, redaction, `Agent::Codex` + `AgentSpec` table, rendered provider profile. Gate: `cargo nextest run -p jackin-domain -p jackin-agents`.
4. `jackin-config`: model, parse-at-boundary, atomic persistence, editor. Gate: nextest on config, plus `cargo fuzz` smoke on the parser.
5. `jackin-openshell` + `jackin-testkit`: `Gateway` trait, fake gateway, `GrpcGateway` against the pinned SDK. Create `compatibility.lock.toml`. Gate: nextest with the fake; one `#[ignore]`d live-gateway test is not allowed by the hermetic rule, so the live check is a manual script run by an agent (named in the PR), not a CI test.
6. `jackin-host` (runner, git, op, caffeinate) and `jackin-session` (plan, execute, attach plan, list, stop, delete). Gate: nextest on session through the fake gateway, covering idempotent retry and cancel.
7. `jackin` CLI: `load`, `attach`, `ls/status`, `eject`, `purge`, `workspace`, `account`, `role`, `config`, `doctor`. Gate: CLI tests through the binary against the fake.
8. `jackin-tui` + `jackin-console`: workspace manager, launch flow, sandbox list, attach hand-off. Gate: render/update tests with ratatui `TestBackend`; `custom-lints` for render purity.
9. Docs and policy: regenerate `ratchet.toml`, baseline, container/sandbox-path gate, docs site content, `AGENTS.md`/`CONTEXT.md` glossary (Instance becomes Sandbox; Capsule removed), workflow generator config upstream.

## 11. Test plan (needs `test-reviewer` approval before any test is written, per AGENTS.md)

All existing tests are deleted. Required behaviors and real scenarios, through public interfaces:

- Launch plan is pure and deterministic. Positive: workspace with a local source, role, Codex, bound account yields the expected provider specs, sandbox labels, upload steps, hook order. Negative: workspace with no allowed role, role without Codex, missing account binding each return a distinct `PlanError`.
- Launch is idempotent. Positive: run `execute` twice with the same `LaunchId` against the fake gateway; exactly one sandbox, one upload marker, hooks run once. Negative: a failure after sandbox creation (upload error) leaves the sandbox, reports it restore-eligible, and a retry resumes without re-creating.
- Cancel never deletes work (ADR-010). Cancel mid-materialize leaves the sandbox; `purge` of a dirty sandbox is refused without explicit confirm.
- Launch never reattaches to a live sandbox (ADR-008). A live sandbox with matching labels does not become the launch result; a stopped or errored one is offered as restore.
- Config round-trip. Valid file parses to typed model and re-serializes preserving user formatting through the editor; unknown fields, bad workspace names, relative sandbox paths are rejected with errors naming the key.
- Secrets never reach logs. A redaction test feeds an `op://`-resolved value and a Codex token through error paths and asserts absence in rendered output.
- Attach plan: argv contains explicit `-g <gateway>` and sandbox name and never relies on the global active gateway.
- Console update logic: key sequences drive model transitions (pick workspace, role, launch, cancel) asserted on resulting state, not on call order.
- Gateway adapter mapping: SDK status codes map to `GatewayError` variants (unreachable, unauthenticated, not found, already exists), driven by a fake tonic server in-process.

No mock of owned code: only `Gateway` (gRPC boundary), the clock and the filesystem are faked.

## 12. Risks

| Risk | Owner |
|---|---|
| OpenShell has no bind mounts; large repos and untracked build caches make upload slow, and writes in the sandbox must be pulled back (download or git push). Product behavior change from live-shared working tree. | `architect` with owner |
| `openshell-sdk` is source-only, tied to a release tag; pre-1.0 (0.1.x) API and proto breaks are expected (the clone's HEAD says "allow accepted protobuf breaks"). | `architect`; pin via new `compatibility.lock.toml` |
| SDK does not read gateway registry; jackin must understand `~/.config/openshell/gateways/*` or call the CLI. Layout is not a stable API. | `architect` |
| Attach depends on the `openshell` binary and system `ssh` on the host. | `architect` |
| Codex provider profile `binaries` must match the role image or the credential is silently not injected. | `architect` (AgentSpec owns both) |
| Codex ChatGPT OAuth refresh tokens rotate; sharing one login between host `~/.codex` and a gateway provider can invalidate the host copy. SPEC already flags this. | owner + `security-reviewer` |
| Deleting 117 crates and all tests in one change loses regression knowledge; mitigate by tagging the pre-rewrite commit and keeping `git log` pointers in the PR body. | implementer |
| Role distribution: nobody builds role images once jackin stops (Q2). | owner |
| Pre-existing Docker container labels/instances on user machines are orphaned; no migration by decision. | owner (accepted) |

## 13. Open questions (ranked)

1. Q1 Capsule/multi-session UX. Is a multiplexer with tabs/panes and a status bar required in v1, or is one retained agent PTY plus `exec --tty` shells enough? Owner. (Design assumes the latter.)
2. Q2 Role images. Who produces images containing `codex`? Options: role authors publish OCI images; jackin publishes a base image from CI (Docker used only in CI, outside the product); install `codex` at first start via exec (needs policy allowing the installer host). Does a role keep a git repo with a manifest at all, or become a config entry with an image ref? Owner.
3. Q3 Workspace delivery. Upload (tar over SSH, gitignore-aware) vs `git clone` inside sandbox vs both, and the return path (download, branch push, `git bundle`). What replaces `worktree` and `clone` isolation? Owner + `architect`.
4. Q4 Agent extension point: `Agent` is a one-variant enum plus a `const` `AgentSpec` table so more agents are one variant and one row. Confirm that is wanted rather than a trait registry. Owner.
5. Q5 Usage/limits and `native/`. Delete macOS app and usage stack now, or keep a Codex-only `jackin-usage` (seed: provider-codex + provider-core) and a trimmed app? ADR-011 needs an amendment either way. Owner.
6. Q6 Is 1Password (`op://`) integration in v1? It decides `jackin-oppicker`, `jackin-env` salvage, and the AGENTS.md `op` convention. Owner.
7. Q7 Telemetry: `tracing` only, or keep OTLP export (and the typed registry gate)? Owner.
8. Q8 Multiple gateways: one configured gateway per workspace/role, or a global default? Explicit gateway per call is assumed. Owner.
9. Q9 Promote `render_thread_purity` from pilot to a required lane? `architect`.
10. Q10 Policy approval inbox in the console (OpenShell draft chunks) in v1 or later? Owner.
11. Q11 Agent-status inference (needs a PTY stream; lost if attach is delegated to `openshell`). Drop? `jackin-agent-status` is the only UNSURE crate tied to this besides usage. Owner.
12. Q12 Whether to align crate boundaries with ADR-001 literally (5 crates) or the 11-crate layering. Owner.
