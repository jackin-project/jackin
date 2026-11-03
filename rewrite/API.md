# jackin public API (task P2.1)

Status: final design for the P2.2 to P9.5 implementers, synthesized by `pstack:arena` from two candidates. Authority: `rewrite/DESIGN.md` (D01 to D28, CLI table, Launch sequence) and `rewrite/PLAN.md`. The sentinel journey J00 to J17 is the acceptance.

Reading order: the rationale (Problem, Usage, Shape, Synthesis decision, Tradeoffs, Alternatives, Open questions), then one section per crate in layer order (jackin-core, jackin-image, jackin-openshell, jackin-session, the usage boundary, jackin-tui, jackin-testkit, jackin, xtask), then the sequencing.

Conventions. Code blocks hold signatures only: free functions end in `;`, method bodies are omitted, and no block contains a comment (D17). Prose that would be a comment sits outside the blocks. Every public item is reachable from the crate root, because `unreachable_pub` and `mod_module_files` are denied workspace lints: a module `foo` is `foo.rs` plus `foo/bar.rs`, and a module map never lists a `mod` file. `todo` and `unimplemented` are denied lints too, so P2.2 adds type definitions and trait signatures, and each later unit adds the bodies with their tests. Every public type implements `Debug` (`missing_debug_implementations`): derived where possible, written by hand where a field is not `Debug` (`AppEnv`). A crate-private struct has `pub(crate)` fields, because `unreachable_pub` denies `pub` on them too. Every test is an integration test in `crates/<crate>/tests/` that uses only the public API (see "Tests live in `tests/`" under Shape).

---

## Problem

Jackin runs Codex in OpenShell sandboxes and has to prove it end to end. Eight crates share one vocabulary: sandbox names, labels, launch IDs, accounts, role manifests, policies, images, and supervisor state. Four facts from the P0 evidence and the pinned v0.1.6 source shape the API.

1. OpenShell is the only state store. Jackin keeps no database and no instance index (D07). Every decision about a launch, an export, or a purge comes from what the gateway and the sandbox files say now. So the launch executor is a function of observed state, and the observed state is typed.
2. The gateway has hard limits that the types must make impossible to violate. A sandbox name, an OpenShell workspace name, and a service name are DNS-1123 labels of at most 19 characters (`MAX_ROUTABLE_NAME_LEN = 19`, line 150 of the `grpc` module root in `crates/openshell-server`, enforced by `validate_sandbox_name`, `validation.rs:241`). A label value is at most 63 characters of `[A-Za-z0-9._-]` that start and end with an alphanumeric (`validate_label_value`, `validation.rs:882`). D07 records the answer: the name is `jackin-<slug7>-<suffix4>`, and the image label is the first 32 hex characters of the image ID.
3. The image ID is not known when a launch is planned. The cache key hashes the built construct image ID (D05), so the final image tag, and the image label of the sandbox, exist only after the image step. Planning must stay pure anyway.
4. A sandbox in `Completed` or `Error` refuses exec and attach; in `Error`, `stop` fails and `start` stays in `Starting` (D09, P7.1 F3). The phase must carry the exit code, and the lifecycle code must refuse these phases before it calls the gateway.

Everything a test may fake is a boundary that jackin owns: the OpenShell gateway, the image engine the gateway driver reads (D23), the network, the clock, the filesystem (D18). Terminal sessions, file transfer, and the Docker build cross a boundary and must not leak tonic, russh, SDK, or wire types. Only `jackin-openshell` imports `openshell-sdk`, `openshell-policy`, `tonic`, and `russh` (D03). Only `jackin-image` starts `docker` (D01).

Repo constraints that bind the API: no code comments (D17), clap help through attributes, no compatibility code, workspace lints `unwrap_used`, `expect_used`, `panic`, `todo`, `print_stdout`, `print_stderr`, `unreachable_pub`, `missing_debug_implementations`, `mod_module_files` denied, `unsafe_code` forbidden, and clippy `disallowed-methods` for blocking `Command::output`, `thread::sleep`, and `File::open` (`clippy.toml`). The usage crates under `crates/usage/` are outside the workspace members until P6.3 (D25).

## Usage (caller's view)

The usage is the spec. Each snippet is the shape callers write after P9.

### Call site 1: `jackin load`

The binary parses config, plans offline, then connects and executes. The plan is a value. Prompting for `[env]` values is the only interactive step and sits between two pure calls: with `Prompting::Interactive` every entry becomes a question with its default pre-filled (D11). The plan needs no gateway connection, so a bad request fails before any network call.

```rust
pub async fn load(args: LoadArgs, app: &App) -> Result<ExitClass, CommandError> {
    let config = app.config.load().await?;
    let (gateway_name, gateway) = config.resolve_gateway(args.gateway.as_ref())?;
    let path = HostPath::resolve(&args.path, &app.cwd)?;
    let project = ProjectRoot::open(app.host.fs.as_ref(), &path).await?;
    let (_, workspace) = config.workspace_for(project.path())?;
    let role = app.roles.load(&config, args.role.as_ref(), workspace).await?;
    let launch = args.launch_id.clone().unwrap_or_else(LaunchId::generate);
    let mut answers = args.env.clone();
    let plan = loop {
        let request = LaunchRequest::new(gateway_name.clone(), project.clone(), role.name().clone(), launch.clone())
            .account(args.account.clone())
            .rebuild(Rebuild::from(args.rebuild))
            .env(answers.clone())
            .prompting(Prompting::for_terminal(app.tty));
        match plan_launch(&config, &role, request) {
            Ok(plan) => break plan,
            Err(PlanError::EnvNeeded(questions)) if app.tty => answers.extend(prompt(&questions)?),
            Err(error) => return Err(error.into()),
        }
    };
    let session = app.session_for(gateway_name, gateway).await?;
    let (events, feed) = launch_events(LineDelivery::Reliable);
    let render = tokio::spawn(render_launch(app.tty, feed));
    let outcome = session.launch(plan, events, app.cancel.child_token()).await;
    render.await?;
    match outcome {
        Ok(LaunchOutcome::Ready(target)) => attach_cli(app, &session, target).await,
        Ok(LaunchOutcome::Cancelled(candidate)) => Ok(report_restore_candidate(&candidate)),
        Err(failure) => Err(CommandError::Launch(failure)),
    }
}
```

`config.workspace_for` fails with `ConfigError::NoWorkspaceForPath`, whose message names `jackin workspace add`: the CLI never creates a workspace by itself, and the console does (D28). `LaunchFailure` carries the step that failed, the cause, and the sandbox that remains, so the CLI prints "sandbox jackin-sentin-k3x9 remains; retry with `--launch-id <id>`". A retry with the same launch ID plans again with that ID and `launch` resumes from observed state. The CLI never drops a build line (`LineDelivery::Reliable`); the console may (`BestEffort`).

### Call site 2: the console launches a sandbox

The console is the baseline `Application` loop (D27). A launch is a tokio task that sends `LaunchEvent`s into the external-message input of the loop. The cockpit page renders `LaunchStep` stages. The pane page owns a `TerminalSession`. A launch from a directory with no workspace opens the workspace editor first and writes `ConfigEdit::AddWorkspace`.

```rust
impl Console {
    fn start_launch(&mut self, request: LaunchRequest, role: LoadedRole) {
        let plan = match plan_launch(&self.config, &role, request) {
            Ok(plan) => plan,
            Err(error) => return self.show_plan_error(error),
        };
        let Some(session) = self.sessions.get(plan.gateway()) else {
            return self.show_gateway_unavailable(plan.gateway());
        };
        let (events, feed) = launch_events(LineDelivery::BestEffort);
        self.bridge.forward(feed, Msg::Launch);
        let session = session.clone();
        let cancel = self.cancel_for(plan.launch_id());
        self.tasks.spawn(async move { Msg::LaunchDone(session.launch(plan, events, cancel).await) });
    }
}
```

On `LaunchOutcome::Ready` the console calls `Session::attach` with the pane size and gets an `AttachedTerminal`, which wraps the `TerminalSession` and filters terminal replies. The same `Session::launch` and `Session::attach` serve the CLI and the console, and a shell pane holds a plain `TerminalSession`.

### Call site 3: `xtask e2e`

With the cargo feature `e2e`, `xtask` links `jackin-core`, `jackin-openshell`, and (from U13) the `jackin-usage` facade. It connects a `Gateway` to run the sentinel probe and read files in the sandbox (decision 12), and it drives the `jackin` binary as a user would (PTY for J11 and J12). A skipped step is `Skip`, never `Pass`.

```rust
pub async fn run(args: E2eArgs, out: &mut dyn Write) -> Result<ExitCode, E2eError> {
    let mut journey = Journey::start(&args).await?;
    let mut report = JourneyReport::new(args.allow_skip.clone());
    for step in JourneyStep::ALL {
        let result = journey.run(step).await;
        writeln!(out, "{}", result.line(step)).map_err(E2eError::Output)?;
        report.push(step, result);
    }
    journey.cleanup().await;
    Ok(report.exit_code())
}
```

`JourneyStep::ALL` is the one list of J00 to J17. `Journey::start` first runs the gateway contract suite (section 3) against the live gateway. The probe output parses into `ProbeReport`, a serde type whose generated JSON schema must equal the checked-in `rewrite/probe-schema.json`.

## Shape

### Crate graph and layer table

```text
jackin-core        vocabulary, config, manifest, agent table, boundary traits
jackin-image       Dockerfile validation, cache key, ImageEngine, ensure_image
jackin-openshell   Gateway trait, GrpcGateway, TerminalSession, policy model, discovery
jackin-session     plan_launch, Session, RoleCatalog, doctor, host implementations
jackin-usage       (crates/usage, from U13) facade: HostUsage, UsageReport
jackin-tui         Console over junie-tui; panes
jackin-testkit     fakes, fixtures, contract runs (dev-dependency only)
jackin             binary and lib: clap, wiring, terminal hand-off
jackin-xtask       gates and e2e
```

The layer step of `xtask ci` reads one table, `LAYERS`, and fails when a workspace member has no row. `dev` lists crates a member may use only as a dev-dependency.

| Crate | May depend on | May dev-depend on | May import beyond the common set |
|---|---|---|---|
| `jackin-core` | none | `jackin-testkit` | no process, HTTP, or gateway crates |
| `jackin-image` | core | testkit | no gateway crates |
| `jackin-openshell` | core | testkit | the gateway crates |
| `jackin-session` | core, image, openshell | testkit | no gateway crates |
| `jackin-usage` and `jackin-usage-*` (from U13) | core, `jackin-usage-*` | testkit | `reqwest`; no `jackin-protocol`, `jackin-telemetry`, or Docker crates |
| `jackin-tui` | core, image, openshell, session, `junie-tui`; `jackin-usage` from U13; the terminal pane block of `junie-tui` from U14 | testkit | no gateway crates |
| `jackin-testkit` | core, image, openshell, session, tui; `jackin-usage` from U13 | none | an HTTP server crate for `UsageServer` (from U13); no gateway crates |
| `jackin` | core, image, openshell, session, tui; `jackin-usage` from U13 | testkit | no gateway crates |
| `jackin-xtask` | none; optional (feature `e2e`): core, openshell, session, `jackin-usage` from U13 | testkit | `portable-pty` and `schemars`, optional (feature `e2e`) |

The gateway crates are `openshell-sdk`, `openshell-policy`, `tonic`, and `russh`; the common set is `serde`, `thiserror`, `tokio`, `tokio-util`, `async-trait`, and `clap` where a crate has a CLI. Rules that follow from the table. `image` and `openshell` never depend on each other. `session` never depends on `jackin-usage`, and no crate outside `crates/usage/` names a `jackin-usage-*` crate other than the `jackin-usage` facade. `docker` is started only from `jackin-image`; the `containers` step enforces it, and the layer table does not. `Secret::expose` is called only from an allowlist of files. `jackin-testkit` depends on `jackin-tui`, and `jackin-tui` dev-depends on `jackin-testkit`: Cargo allows this cycle for dev-dependencies, which is why every test is an integration test (below).

### State owners

| State | Single owner | Others |
|---|---|---|
| `config.toml` | `ConfigStore`, one writer API `edit(Vec<ConfigEdit>)` under a file lock | read a `Config` snapshot |
| Sandbox lifecycle, labels, providers, policy | the gateway | jackin reads through `Gateway` |
| Launch progress | observed sandbox state (labels, phase, marker files) | the executor keeps one per-run `Done` value |
| `/sandbox/.jackin/run/start` | jackin (`Session`) | `jackin-start` reads |
| `/sandbox/.jackin/run/state` | `jackin-start` | jackin reads and treats as advisory (D09) |
| `/sandbox/.jackin/run/env-kinds` | `jackin-start`, at each start | the sentinel probe reads it for J08; names and kinds only (D09) |
| `/sandbox/.jackin/env-<launch-id>` | jackin (`Session`, Credentials step) | hooks, `jackin-start`, and shells read it (D11) |
| Size of one main PTY | the pane or attach task that holds the `TerminalSession` | one pane per main process (D28) |
| Image cache | the Docker engine (tag = cache key) | `jackin-image` reads with `image inspect` |

### Boundaries and where each one lives

AGENTS.md and D18 allow fakes only at external boundaries, behind interfaces jackin owns. These are all of them. Each has one trait, one real implementation, one fake, and no other seam. Code that jackin owns (planning, cache keys, the credential resolver, the usage dispatch) runs for real in every test.

| Boundary | Trait (owner) | Real | Fake (testkit) | Consumers |
|---|---|---|---|---|
| OpenShell gateway, with file transfer, program runs, and terminals | `Gateway` (openshell) | `GrpcGateway`; its private `transfer.rs` runs `openshell sandbox upload` and `download` | `FakeGateway` | session, tui, binary, xtask |
| Image engine (the Docker engine the gateway driver reads, D23) | `ImageEngine` (image) | `BuildxEngine` over `ProcessRunner` | `FakeEngine` | session, binary |
| Network (usage providers) | `ProviderEndpoints` (usage) | `ProviderEndpoints::real()` | `UsageServer`, a local HTTP server, through a base URL override | usage, tui, binary |
| Clock | `Clock` (core) | `SystemClock` (session) | `FakeClock` | session, image, usage, tui |
| Filesystem | `FileSystem` (core) | `StdFileSystem` (core) | `MemFs` with fault injection | core, image, openshell, session, usage |
| Host programs | `ProcessRunner` and the closed `Program` enum (core) | `TokioProcessRunner` (session) | `ScriptedProcessRunner`, for `docker`, `openshell`, `ssh`, and `op` only | image, openshell, session, usage |

`git` is not a fake. `Program::Git` runs for real on temporary repositories in every test that needs it, and `ScriptedProcessRunner::expect` takes a `ScriptedProgram` enum without a `Git` variant, so faking git is unrepresentable. The CLI transfer runner is not its own trait: a trait there would let a test fake the part of the upload that decides what the upload is. Randomness is not a boundary: `LaunchId::generate` reads the OS and tests pass the launch ID. Attempt IDs are not random: `SupervisorState::next_attempt` derives them.

### Tests live in `tests/`

`jackin-testkit` depends on `jackin-session` and `jackin-tui`, and those crates dev-depend on it. A unit test inside `src/` would link two copies of the crate under test, one directly and one through the testkit, and the fakes would not implement the traits the test sees. So every test is an integration test in `crates/<crate>/tests/`, uses only the public API, and links one copy. Shared fixtures live in `jackin-testkit`, and no crate keeps a private support module. `jackin` and `jackin-xtask` have lib targets for the same reason: their tests call `run` and the check functions directly.

### Load-bearing decisions

1. **Launch is a convergent executor over typed observations, and planning is pure.** `plan_launch(config, role, request)` is a free function with no I/O and no randomness. It yields a `LaunchPlan` that holds a `SandboxDraft`: the sandbox name, the labels without the image label, the providers, the merged policy, and the main process, but not the image tag. The plan also holds the two non-secret files that the Credentials step writes, already rendered: the Codex `config.toml` and the env file. `launch` repeats `observe`, then `decide(plan, observed, done, limits)`, then one idempotent act. The image step returns a `BuiltImage`, and `SandboxDraft::complete(&BuiltImage)` produces the `SandboxCreate` with the final tag and the image label. The creation fence is the sandbox name: the suffix derives from the launch ID, so two racing creates for one launch collide on the name and the loser joins the winner.
2. **One `Gateway` trait whose methods are semantic, and the fake implements the same semantics.** File reads and writes, typed program runs, project upload, file download, and terminal open are methods. `ProgramRun` carries `touch_on_success`, so the setup hook and its marker are one typed request and the fake never parses a shell line. The real adapter builds these from exec, the `openshell` CLI, `CreateSshSession`, `ForwardTcp`, and `russh`. `TerminalSession` is a concrete channel-pair handle shared by the real and the fake side, so pane code is the same for the Codex pane and a shell pane.
3. **`jackin-core` owns the vocabulary and parses at the edge.** Every identifier that reaches a label, a sandbox name, a path, or an argv is a validated newtype. `Config` has private fields and one mutation entry, `Config::apply(ConfigEdit)`, so a parsed or edited config is always referentially valid. The agent table is `const` data. The `Provider` enum lives in core and `jackin-usage` matches it exhaustively (D26).
4. **The Docker capability is one sum type.** `DockerCapability::{Disabled, Enabled(EngineAccess)}` carries the engine and the source that enabled it. The image sees only `DockerLayer`. "Enabled without an engine" cannot exist after `DockerCapability::resolve`.
5. **The image cache key hashes the built construct image ID.** `ensure_image` builds or finds the construct, reads its ID, derives the final tag from it, and only then looks for the final image. The role revision is `Clean { commit }` or `Dirty { commit, digest }`.
6. **The usage boundary is one facade crate.** `jackin`, `jackin-tui`, and `jackin-testkit` import `jackin-usage` only. They see `HostUsage`, `UsageRequest`, and `UsageReport`, never a provider crate or a `QuotaBucketView`.
7. **Role validation and `doctor` need no gateway.** `RoleCatalog` is constructible without a `Session`, and `doctor` is a free function that takes a `GatewayAccess`. `jackin role check` and `jackin doctor` work with a stopped gateway (G-ROLE, J01).
8. **The 20-sandbox and 3-stream limits bound the console panes** (P0.6). `Gateway::open_terminal` mints one SSH session token per terminal and surfaces `GatewayError::ResourceExhausted`. There is no pool.

### Interface depth

| Crate | Public surface | Hidden behind it |
|---|---|---|
| jackin-image | `ensure_image` (1 function), `ImageEngine` (3 methods), `ImageRequest`, `BuiltImage`, `validate_dockerfile` | embedded construct and derived Dockerfiles, cache key, builder driver check, three buildx runs, build log parsing, `image inspect` |
| jackin-openshell | `Gateway` (16 methods), `TerminalSession`, `PolicyDocument`, discovery | SDK, tonic, russh, `openshell-policy`, host key pinning, token minting, status mapping, CLI argv, timeouts, stdin from `/dev/null` |
| jackin-session | `plan_launch`, `plan_image`, `Session` (12 operations), `RoleCatalog`, `doctor` | convergent executor, probe, hook runs, `auth.json` and `config.toml` rendering, reply filter, bundle export, work-state inspection |
| jackin-usage | `HostUsage` (1 method) | every provider crate, credential discovery, caches, HTTP |
| jackin-tui | `Console`, `run_console` | pages, panes, vt100 parsing, key encoding |

The widest surface is `Gateway`. Sixteen methods is the number of distinct external operations the launch needs. Splitting it into several traits would give the fake several things to implement and the session several handles to hold, with no hidden complexity gained.

The design does not read `/sandbox/.jackin/run/state` for any decision about export or purge (D09). It keeps no list of sandboxes. It has no registry delivery (D23), no engine start (D24), no policy language (D12), and no plugin boundary for agents: `Agent` is an enum and the table is exhaustive (D04). It builds no compatibility code for credential mode A: an account that needs it gets a typed `Unsupported` error until a profile exists (decision 5).

## Synthesis decision

**Base: candidate 1** (judge score 20 against 14). Reasons, each checked against the repo and the pinned source:

- **Name-length correctness.** Candidate 1 read the v0.1.6 tree and found the 19-character limit on sandbox names and the 63-character limit on label values. It made both unrepresentable in types (`SandboxName`, `ImageLabel`). Candidate 2 found only the label limit and kept a `SandboxName::for_launch` with no length rule. D07 now records the answer, and this design applies it.
- **Contract suite.** Candidate 1 puts one `contract::run(&dyn Gateway)` in `jackin-openshell`. The fake and the real adapter both pass it, so the fake cannot drift unobserved. Candidate 2 relies on a review of the fake.
- **Sequencing.** Candidate 1 orders the units so the session is built and proven against the fake before the SDK adapter exists, and each unit ends at a PLAN gate. It also has the `decide` table over typed observations, the state-owner table, and the interface-depth table.

**Fixes applied to the base** (orchestrator, binding):

| # | Fix | Where it lands |
|---|---|---|
| 1 | Cache key hashes the built construct image ID; role revision has `Dirty` | `ImageKey::derive`, `ImagePlan::final_tag`, `ensure_image`, `RoleRevision` (section 2) |
| 2 | `plan` stays pure; `LaunchPlan` holds a `SandboxDraft`; `BuiltImage` completes the labels | `plan_launch`, `SandboxDraft::complete`, `PlannedLabels::complete` (sections 1 and 4) |
| 3 | `--fast` equals `--no-docs` | `CiArgs::profile`, `CiProfile` (section 9) |
| 4 | `set_power` on `Completed` or `Error` | `Session::power` refuses with `LifecycleError::MainProcessEnded`; `FakeGateway` models stop and start (sections 3, 4, 7) |
| 5 | xtask layer row | layer table above; `LAYERS` (section 9) |
| 6 | `role check` and `doctor` without a gateway | `RoleCatalog::new`, `doctor`, `GatewayAccess`, `registered_gateways` (sections 3 and 4) |

**Grafts from candidate 2:**

| Graft | Source in candidate 2 | Where it lands |
|---|---|---|
| Typed `run_program` with `touch_on_success` | "Domain types", `ProgramRun` (about line 1431) | `Gateway::run_program`, `ProgramRun` |
| One `DockerCapability::{Disabled, Enabled(EngineAccess)}`, `DockerLayer` for the image | "Launch planning" (lines 1739 to 1754), `DockerLayer` (line 1081) | `jackin-core::docker`, `ImageRequest.docker` |
| `Phase::Completed { exit_code }` and `Phase::Error { exit_code }` | "Domain types" (line 1389) | `jackin-core::phase` |
| `ImagePlan::final_tag(&construct)` and the `ImageEngine` boundary with `ensure_image` | "jackin-image" (lines 1101 and 1158) | section 2 |
| Plan keeps labels without the image; the image step completes them | "Launch execution" (line 1876), `PartialLabels` | `PlannedLabels`, `SandboxDraft` |
| `LAYERS` table where a member without a row fails | "xtask" (line 2766) | layer table above, section 9 |
| `registered_gateways`, `GatewayAuth`, doctor without a gateway | "GrpcGateway" (line 1553), "jackin" (line 2418) | section 3 discovery, section 4 doctor |
| Tests as integration tests in `crates/*/tests/` | "Test plan outline (jackin-openshell)" (line 1604) and (line 1032) | every test plan |
| Open questions Q3, Q4, Q5, Q7 | candidate 2 open table | decided: `Provider` enum in core; `jackin gateway add\|ls\|rm`; the CLI errors without a workspace and the console creates one; the fake `SupervisorModel` follows the `SupervisorState` grammar in core, `agent-sandbox-engineer` reviews each `jackin-start` change against it, and J06 and J07 prove the real script |

**Orchestrator decisions 1 to 15** were taken while this file was synthesized. `decisions.tsv` has no row for them yet, so this table is their record until the orchestrator adds one (open question 7). DESIGN.md carries these: 1 and 2 in D07, 3 and 4 in D05, 5 and 14 in D10, 6 in the P2.3 step list, 7 in the CLI table, 11 in D03 and D14, and 13 in D11 as the env file. D14 also carries the `jackin-usage` facade rule and the lib targets of `jackin` and `xtask`. This file applies them as follows.

| # | Decision | Section |
|---|---|---|
| 1 | Sandbox name `jackin-<slug7>-<suffix4>`, at most 19 characters; suffix from the launch ID; full workspace name in a label; OpenShell workspace name at most 19 | 1 Identifiers (`SandboxSlug`, `NameSuffix`, `SandboxName`, `OpenShellWorkspace`), 4 `plan_launch` |
| 2 | Image label is the first 32 hex of the image ID; J03 compares the prefix | 1 `ImageLabel`, `PlannedLabels::complete`; 4 `SandboxDraft::complete` |
| 3 | Cache key includes the built construct image ID | 2 `ensure_image` |
| 4 | `RoleRevision::Dirty` with a content digest | 2 `RoleRevision`, 4 `RoleCatalog` |
| 5 | Mode B built in P4.1, P5.1, P6.1; mode A returns typed `Unsupported` | 1 `AgentSpec::profile_for`, 3 `ProviderMaterial`, 4 `PlanError::Unsupported`, `AccountError::Unsupported` |
| 6 | `--fast` per PLAN | 9 `CiArgs` |
| 7 | Global `--gateway`; config `default_gateway`; `jackin gateway add\|ls\|rm` | 1 `GatewayEntry`, `ConfigEdit`; 8 `GatewayCommand` |
| 8 | First `jackin load` without a workspace errors and names `jackin workspace add` | 1 `ConfigError::NoWorkspaceForPath`; 8 tests |
| 9 | Fake boundaries; `git` real; `IdSource` dropped | Shape "Boundaries", 7 testkit |
| 10 | `Provider` enum in core; exhaustive match in `jackin-usage` | 1 `Provider`, 5 dispatch |
| 11 | `openshell-policy` confined to `jackin-openshell` | 3 policy, layer table |
| 12 | xtask runs the probe through `Gateway::run_program` | 9 `Journey` |
| 13 | Role `[env]` values reach hooks, `jackin-start`, and shells through the env file `env-<launch-id>` (D11; this replaces the first synthesis, which put them in the sandbox environment) | 1 `EnvFile`, 4 `plan_launch`, hook rows of `decide`, `open_shell` |
| 14 | Provider host closure exempts the generated `_provider_<name>` rule on a live policy | 3 `closure_violations`, 4 `Session::inspect` |
| 15 | No `mod` files; no `todo`; signatures only | Conventions; all module maps; sequencing U0 |

Also decided here, from the other open questions: the contract suite runs in nextest against the fake (three groups) and the in-process server (the `Core` group), and in `xtask e2e` against the live gateway (candidate 1 question 11); the layer step grows by the import and `expose` rules inside the one `LAYERS` table (15); the testkit and tui dev-dependency cycle is safe because tests are integration tests (17); the gateway entry stores platform and builder explicitly (19, 20).

**Rejected from candidate 1:** `IdSource` and `SeqIds` (decision 9); the `ImageBuilder::ensure` seam and `FakeImageBuilder`, which fake code jackin owns (planning, key, cache) instead of the engine; the `CredentialResolver` trait with `StaticCredentials` and the `UsageSource` trait with `FixedUsage`, for the same reason; the global `[build]` config section and `SetBuilder` (the builder is a gateway-entry field, as the DESIGN CLI table row for `jackin gateway` says); `DockerSetting`, the second `DockerCapability`, and `DockerPlan`; the salted name retry; `Session::plan` and `Session::roles` and `Session::doctor` (replaced by free functions and a standalone catalog); `PolicyDocument` and the provider profile parse in core (moved behind `openshell-policy`); the generic `exec` plus composed `sh -c`.

**Rejected from candidate 2:** generic `Sessions<G, E, C>` and `Console<G, E, C>` (they spread type parameters over every caller and test, and `dyn` costs one allocation per call); a `Gateway` that knows `git_state` and `git_bundle` (export knowledge belongs in `jackin-session`, and the fake maps sandbox paths in git arguments instead); module maps with `mod` files (the `mod_module_files` lint denies them); per-step self-checking without a pure `decide` (the skip rules would spread over nine functions and the observation table could not be tested without I/O); `LaunchProgress` as a callback trait (a bounded channel gives backpressure and one owner); `AccountId` and `OpenShellProviderName` in place of the label-safe newtypes with their grammars; `Phase::Other(String)` (the adapter maps unknown values to `Unknown`).

### Review round 1

An independent review of the first synthesis found 16 problems. The orchestrator accepted all of them and recorded the J17 decision in PLAN.md. Each is resolved in this file as follows.

| # | Finding | Resolution | Where |
|---|---|---|---|
| 1 | `AttemptId` is a random string | `AttemptId(NonZeroU64)`, 1 to 18 digits, `FIRST`, `next`; `generate` deleted; pure `SupervisorState::next_attempt`; tests use `running 1`, `exited 1 3`, stale `0`, a repeated ID | 1 Identifiers, Supervisor state; 4 `Start` row; 7 `SupervisorModel` |
| 2 | Codex trust file is missing | `AgentSpec.user_config`; pure `render_codex_config`; written in the Credentials step; a plan test asserts the bytes | 1 `AgentSpec`; 4 `PlannedFiles`, `decide` |
| 3 | U0 creates items whose dependencies do not exist | every `jackin-usage` edge moves to U13 and every baseline-extension item to U14; marked "from U13" and "from U14" | layer table; 5, 6, 7, 8, 9; 10 |
| 4 | `const fn` and `const` rows cannot hold paths | `SandboxPath(Cow<'static, str>)`; `static CODEX_SPEC`; `Agent::spec` is a plain function; constants checked in a test | 1 Paths, Agents |
| 5 | Lint violations | `DockerError.enabled_by`; `pub(crate)` fields; `Redactor::new(&'_ Secret)`; `Compatibility::pinned` returns a `Result`; manual `Debug` for `AppEnv`; `poll_until` takes a generic future | 1 Docker, Secrets, Boundary traits, Views; 4 executor; 8 `AppEnv` |
| 6 | Two owners of the role environment | one env file per launch, written in the Credentials step and read by hooks, `jackin-start`, and shells; `SandboxDetail.environment` deleted; open question 1 is now the `jackin-start` change | 1 Env file; 3 types; 4 `decide`, `open_shell`; Shape state owners |
| 7 | Replay answers reach the PTY | one pure `split_replies` behind `AttachedTerminal::input`, which the CLI and every pane use; the state is read when a reply appears; tested with a replayed CPR query | 4 Attach; 6 panes; 8 `term.rs` |
| 8 | A transient create failure ends the launch | `SandboxView.provisioning`; `decide` deletes and recreates a sandbox this run created, up to `limits.create_attempts`; typed `ProvisioningFailed` after that | 3 types; 4 `decide`; 7 `ProvisionFails` |
| 9 | `[codex] model` is not used | rendered into the Codex `config.toml` | 1 Manifest; 4 `PlannedFiles` |
| 10 | A string rule for `docker` in the layer step | deleted; the `containers` step alone guards the crate boundary of `Program::Docker` and `DockerSubcommand` and flags `Command::new` of container programs and computed programs; the subcommand rule is the `DockerSubcommand` type | 9 Rules, tests |
| 11 | The contract suite needs infrastructure the in-process server lacks | groups `Core`, `Transfer`, `Terminal`; all against the fake and the live gateway; `Core` only in process; name length moves to `SandboxName::parse` tests and a raw adapter test | 3 Contract suite |
| 12 | The 0.1.6 input lease is not modeled | `Lease` event and the lease rules in the `open_terminal` contract; the fake models them; the Terminal group checks them | 3 Terminal sessions; 7 `open_terminal` |
| 13 | The `ToolRunner` seam fakes the repository's own tools | deleted; the run tests drive the shipped `jackin-xtask` binary with scripted `cargo`, `reuse`, and `bun` on `PATH` and assert the log of tool calls and the exit status (U1 fix round) | 9 Types, Rules, tests |
| 14 | Decisions are not cited accurately | the citation names DESIGN anchors and says `decisions.tsv` has no rows yet | Synthesis decision |
| 15 | Argument structs are missing | all structs and enums defined; `account add --name`; `build` prints the image ID and `--json` writes `BuildOutput` | 8 Commands, Output |
| 16 | Minor findings | `Suspending` and `Suspended` rows; private `SandboxCreate` fields with a constructor; `LineDelivery`; `Prompting` with defaults pre-filled; reserved credential keys and `CODEX_HOME`; three call-asserting tests rewritten; `boot` guards a stale `running`; profile update through `UpdateProviderProfiles`; U5 and U7 merged to follow PLAN P4.1 then P4.2; `capture_env` moved to `jackin-session::host`; xtask `e2e` feature | 1; 3; 4; 5; 9; 10 |
| J17 | Extra Codex accounts must not rotate the A1 token | fixture login with a non-working refresh token; J17 observes the account label and the OpenShell provider | 9 Rules |

## Tradeoffs accepted

- We accept `async_trait` boxing on `Gateway`, `ImageEngine`, `FileSystem`, `ProcessRunner`, and `Clock` in exchange for `Arc<dyn ...>`. One allocation per call is noise next to a gRPC round trip, and `dyn` removes generic parameters from `Session`, `Console`, and every test harness.
- We accept that `Config` changes go through one `ConfigEdit` enum in exchange for invariants held in one place. A new config feature adds a variant, and the compiler lists every match site.
- We accept a 16-method `Gateway` in exchange for a fake that implements semantics, not argv.
- We accept polling with the `Clock` for "wait until Ready" and "wait until the state file exists" instead of SDK `wait_ready`, in exchange for tests that run under a fake clock with no real sleeping.
- We accept that `jackin-session::host` holds the real `ProcessRunner` and `Clock` (PLAN P5.2) and that `jackin-session::process_group` holds `ProcessGroup` (process-group signals and `/proc` reads), the host process runner's group stop. `xtask` is a top-level tooling binary and depends on `jackin-session`, and the `LAYERS` row of `jackin-xtask` allows that edge, so the stop of a process group has one implementation (`rewrite/reconciliation/sandbox-pgid.md`). `GroupFault` stays in `jackin-core` because `ProcessError` holds it and it does no I/O. `xtask` carries a private runner of about 50 lines for the `openshell` calls of its `GrpcGateway` and a plain `tokio::process` call for the `jackin` binary, and both stop their children through `ProcessGroup`.
- We accept a sandbox-name suffix derived from the launch ID in exchange for a creation fence. A suffix collision between two different launches is a `NameCollision` error: the operator retries with a new launch ID. There is no salted retry.
- We accept truncating the workspace slug to 7 characters in the sandbox name. The full workspace name is a label.
- We accept the lib targets of `jackin` and `jackin-xtask` in exchange for integration tests that call the real entry point (D14 names them). Each package still ships one binary.
- We accept that `ScriptedProcessRunner` can fake `docker`, `openshell`, `ssh`, and `op` and nothing else, in exchange for a hard line between faking an external program and faking code we own.
- We accept that `jackin build` uses the role's Docker setting only. A launch from a workspace with `docker = true` builds a second image with the Docker client layer, because the capability is part of the cache key.
- We accept a closed `Program` enum in `ProcessSpec` in exchange for one greppable list of external programs.

## Alternatives considered

Whole-shape alternatives. Crate-level choices follow.

- **A. Gateway as a thin wire trait (`ExecSandboxInteractive`, `CreateSshSession`, `ForwardTcp` as methods) with the session building everything.** Rejected. The session would learn frames, tokens, and host-key checks, and the fake would have to emulate SSH. Interface depth is low and D03 says only `jackin-openshell` knows these.
- **B. A workflow engine with a durable plan and a step journal.** Rejected. D07 forbids a local index. Labels plus marker files already are the journal.
- **C. Typestate launch (`Launch<Created>`, `Launch<Uploaded>`, ...).** Rejected for the executor, because a crash loses the value and resume starts from observation anyway. The typed sum survives where it pays: `DockerCapability`, `SupervisorState`, `Phase`.
- **D. Config as a `toml::Value` with ad hoc setters in the console and CLI.** Rejected. Every writer would re-implement the binding rules. `Config::apply(ConfigEdit)` is one door.
- **E. Console ports (`WorkspacePort`, `FleetPort`, `UsagePort`) that the fixtures implement.** Rejected. The identity gate can seed the real wiring with `jackin-testkit` fakes; ports would repeat `Session` and `Config`.
- **F. Usage crates imported directly where needed.** Rejected. 34 crates in the import graph, wire types in the console, and a merge that D25 allows would break callers. One facade absorbs merges.
- **G. A fake of `ImageBuilder::ensure`.** Rejected. It fakes the planning, key, and cache code that jackin owns. The boundary is the engine, so the fake is `FakeEngine` and `ensure_image` runs for real.
- **H. Plan with a placeholder image tag, patched by the executor.** Rejected. A patched plan is mutable state. A `SandboxDraft` that only `complete(&BuiltImage)` can turn into a `SandboxCreate` makes the missing image impossible to forget.

Per-crate choices that lost are named where they arise: the `docker-container` builder (P0.3), a separate delivery step (D23), one trait per gateway concern, a `CliTransfer` trait, a pane-owned redraw nudge, a local launch journal (D07), reading `run/state` for export or purge (D09), a second event loop (D27), a fake that interprets shell lines, and a second crate list for the layer step.

## Open questions and risks

Each names the agent that resolves it. None blocks P2.2.

1. **`jackin-start` must source the env file.** D11 makes `/sandbox/.jackin/env-<launch-id>` the one source of the role environment, and `jackin-start` reads it before each attempt. The P7.4 draft (`drafts/jackin-start`) does not, and still only validates `JACKIN_LAUNCH_ID` (P7.4 F9). The file is `export NAME='value'` lines (`EnvFile::render`), so `. "$file"` before each attempt is enough. Resolver: the orchestrator assigns it to `agent-sandbox-engineer`, folded into P4.3 where the script moves into `jackin-image`, with a live check.
2. **`stop` and `start` on a `Completed` sandbox are unobserved.** The evidence covers `Error` only (P7.1 F3). `FakeGateway` models `Completed` like `Error`, and `Session::power` refuses both before it calls the gateway. Resolver: `openshell-specialist`, live in P4.1, recorded in `evidence/`.
3. **The provider-name grammar on the gateway is partly read.** The name rule was not read. `AccountName` is capped at 32 characters so `jackin-<account>-codex` stays short. Resolver: `openshell-specialist` in P4.1.
4. **The replay repaint on a real Codex pane is unproven** (P0.6 "Not verified"). `RepaintPolicy` lets the policy change without an API change. Resolver: `rust-cli-tui-engineer` in P9.3.
5. **A hook failure leaves the sandbox `Ready` only because `jackin-start` never exits** (D09). If P7.4 changes `jackin-start` to exit, launch recovery changes (P7.1 F3). Resolver: `agent-sandbox-engineer` in P7.4.
6. **The baseline data interfaces do not exist until P9.1.** Section 6 names pages, messages, and the screen-to-data table, not the baseline trait names. Resolver: `rust-cli-tui-engineer` in P9.1 and P9.3.
7. **CLI wording and decision records beyond DESIGN.** DESIGN now carries `account add --name/--credential`, `workspace add <name> <path> [--docker]`, `build --json`, and the lib targets. Still beyond it: `--credential` also takes `env:NAME` and `file:PATH` (the table names an op reference), `gateway add` takes the entry fields, and `inspect` and `account ls` take `--json`. `decisions.tsv` has no rows for orchestrator decisions 1 to 15. Resolver: orchestrator.
8. **The shape of a failed provisioning, and `Suspended`.** P7.4 F8 saw `ProvisioningFailed: wait for image preparation: Operation not permitted` on parallel creates from a new image. This file assumes that the sandbox ends in `Error` with no exit code and a status condition with the reason `ProvisioningFailed` (written by `openshell-driver-vm`, `driver.rs:1360`), and that delete then create works. The `Suspending` and `Suspended` phases are also unobserved. Resolver: `openshell-specialist`, live in P4.1.
9. **Profile update on drift.** `ensure_profile` replaces a differing profile through `UpdateProviderProfiles`, which needs the resource version. D10 says jackin imports the profile only when it is absent. The `codex` profile is jackin's embedded asset, so a difference means a newer jackin. Resolver: orchestrator confirms and amends D10.
10. **The `AgentSpec` draft lacks two fields.** `drafts/agentspec-codex.toml` has no `user_config` (`/sandbox/.codex/config.toml`, mode 0600) and no `reserved_env` (`CODEX_HOME`). Resolver: the orchestrator updates the draft, and `rust-engineer` builds both into the table in P3.1.

## Next implementation step

Create the eight crates with the module maps and the type, trait, and error definitions below, starting with `jackin-core::{ids, paths, secret, phase, boundary}`, because every other crate imports them (P2.2, then P3.1). U0 creates only the items whose dependencies exist today; the items marked "from U13" and "from U14" wait for their units (section 10).

---

## 1. jackin-core

Role: the shared vocabulary, the config model, the role manifest, the agent table, and the boundary traits. It performs file I/O only through `FileSystem`, and only `StdFileSystem` touches the OS (config files, D14). It never spawns a process, never reads the clock, never opens a socket. Its only impure function is `LaunchId::generate`, which reads OS randomness. It holds no YAML: policy and provider-profile parsing live in `jackin-openshell` (decision 11).

### Module map

```text
crates/jackin-core/src/
  lib.rs            re-exports only
  ids.rs            label-safe newtypes: names, launch and attempt IDs, sandbox name, image ID, label, tag
  paths.rs          HostPath, SandboxPath, RepoPath, ProjectRoot, sandbox_layout
  secret.rs         Secret, SecretBytes, Redactor, TerminalText
  platform.rs       Platform
  provider.rs       Provider
  agent.rs          Agent, AgentSpec, InstallSpec, OpenShellSpec, ProfileAsset, AuthFileSpec, ConfigFileSpec, CodexAuthMode
  labels.rs         Labels, PlannedLabels, SandboxLabels, LabelSelector
  phase.rs          Phase
  supervisor.rs     SupervisorState, StartRequest
  manifest.rs       RoleManifest, EnvSpec, HookKind
  envfile.rs        EnvFile
  docker.rs         DockerCapability, EngineAccess, DockerSource, DockerLayer, DockerRisk
  config.rs         Config, ConfigEdit, Account, CredentialSource, Workspace, RoleEntry, GatewayEntry
  config/{select,store}.rs   account selection (D26); ConfigStore
  boundary.rs       Clock, FileSystem, ProcessRunner and their data types
  fs.rs             StdFileSystem, write_atomic
  wait.rs           poll_until
  display.rs        plain, plain_list, plain_duration: text for #[error] messages
  tomlkit.rs        private: TOML reader and quoting helpers for config parse and render
  views.rs          SandboxSummary, InspectView, LsOutput, BuildOutput (serde views printed by --json)
  compat.rs         Compatibility, CompatibilityError (parsed compatibility.lock.toml)
  assets/codex-provider.yaml
```

### Identifiers and paths

Every type below is `Clone + Debug + Eq + Ord + Hash + Display + FromStr + Serialize + Deserialize`; `AttemptId` is also `Copy`. Deserialization calls `parse`, so a value that exists is valid. Each type has its own grammar:

| Types | Grammar |
|---|---|
| `WorkspaceName`, `RoleName`, `LaunchId`, `ImageLabel` (all become label values) | 1 to 63 characters of `[A-Za-z0-9._-]`, first and last alphanumeric: the gateway rule for label values |
| `AccountName` | DNS-1123 label, at most 32 characters (a label value and part of `ProviderName`) |
| `OpenShellWorkspace`, `SandboxName` | DNS-1123 label, at most 19 characters; `SandboxName` also starts with `jackin-` |
| `SandboxSlug` | lowercase alphanumeric and single hyphens, 1 to 7 characters, no leading or trailing hyphen |
| `NameSuffix` | exactly 4 base-36 digits |
| `ProviderName`, `ProfileId` | lowercase alphanumeric and single hyphens, at most 63 characters |
| `GatewayName`, `BuilderName` | 1 to 64 characters of `[A-Za-z0-9._-]` (not used in labels) |
| `EnvName` | POSIX name `[A-Za-z_][A-Za-z0-9_]*` |
| `ChatgptAccountId` | 1 to 64 characters of `[A-Za-z0-9-]` |
| `BranchName` | valid per `git check-ref-format --branch`, no leading `-`, not `HEAD`, no leading `jackin/`, and no Unicode control character or format character (general category Cf) |
| `CommitId`, `Sha256` | 40 or 64 hex, and 64 lowercase hex |
| `OpReference` | `op://` followed by at least two non-empty segments |
| `ImageTag` | `jackin.invalid/<name>:<16 hex>` |
| `AttemptId` | decimal integer of 1 to 18 digits, no leading zero, at least 1 |
| `ModelName` | 1 to 64 characters of `[A-Za-z0-9._:/-]`, first alphanumeric (it is written into TOML unescaped) |

```rust
pub struct WorkspaceName(String);
pub struct RoleName(String);
pub struct GatewayName(String);
pub struct BuilderName(String);
pub struct OpenShellWorkspace(String);
pub struct LaunchId(String);
pub struct AttemptId(NonZeroU64);
pub struct EnvName(String);
pub struct ChatgptAccountId(String);
pub struct BranchName(String);
pub struct CommitId(String);
pub struct Sha256(String);
pub struct OpReference(String);
pub struct AccountName(String);
pub struct ProviderName(String);
pub struct ProfileId(String);
pub struct SandboxSlug(String);
pub struct NameSuffix(String);
pub struct SandboxName(String);
pub struct ImageId(Sha256);
pub struct ImageLabel(String);
pub struct ImageTag(String);

impl WorkspaceName {
    pub fn slug(&self) -> SandboxSlug;
}

impl SandboxSlug {
    pub const MAX_LEN: usize = 7;
    pub fn parse(text: &str) -> Result<SandboxSlug, IdError>;
}

impl NameSuffix {
    pub const LEN: usize = 4;
    pub fn for_launch(id: &LaunchId) -> NameSuffix;
}

impl SandboxName {
    pub const MAX_LEN: usize = 19;
    pub fn new(slug: &SandboxSlug, suffix: &NameSuffix) -> SandboxName;
    pub fn parse(text: &str) -> Result<SandboxName, IdError>;
}

impl LaunchId {
    pub fn generate() -> LaunchId;
}

impl AttemptId {
    pub const FIRST: AttemptId;
    pub fn next(self) -> Option<AttemptId>;
}

impl ProviderName {
    pub fn for_account(account: &AccountName, agent: Agent) -> ProviderName;
}

impl ImageId {
    pub fn parse(text: &str) -> Result<ImageId, IdError>;
    pub fn label_value(&self) -> ImageLabel;
    pub fn full(&self) -> &str;
}

impl ImageTag {
    pub fn for_image(name: &str, key: &Sha256) -> ImageTag;
}

impl Sha256 {
    pub fn of(bytes: &[u8]) -> Sha256;
}

impl EnvName {
    pub fn is_reserved(&self) -> bool;
}
```

Rules. `WorkspaceName::slug` lowercases, maps every run of other characters to one hyphen, trims hyphens, truncates to 7, trims a trailing hyphen, and yields `ws` for an empty result. `NameSuffix::for_launch` takes the first 4 base-36 digits of a SHA-256 over the launch ID bytes. `SandboxName::new` always yields `jackin-<slug>-<suffix>`: at most 19 characters, DNS-1123, no `--`. `ImageId::label_value` is the first 32 hex characters (decision 2). `ImageTag::for_construct`, `for_role_stage` and `for_role` give `jackin.invalid/construct:<h>`, `jackin.invalid/role/<repo>:<h>` and `jackin.invalid/<repo>:<h>`, where `<h>` is the first 16 hex of the key (D05, U7 fix round). `EnvName::is_reserved` is true for the `JACKIN_` prefix, for `DOCKER_HOST`, `DOCKER_TLS_VERIFY`, and `DOCKER_CERT_PATH`, and for every name in `AgentSpec::reserved_env` (`CODEX_HOME`) and in the `CredentialKeys` of each `Agent` (the OpenShell credential keys), so a role `[env]` entry cannot shadow them. `ProviderName::for_account` builds `jackin-<account>-codex`. `LaunchId::generate` yields 26 lowercase base-32 characters from the OS random source. `AttemptId::FIRST` is 1, `next` adds 1 and gives `None` past 18 digits (`999999999999999999`), and there is no generator.

```rust
pub struct HostPath(PathBuf);
pub struct SandboxPath(Cow<'static, str>);
pub struct RepoPath(String);
pub struct ProjectRoot(HostPath);

impl HostPath {
    pub fn parse(path: &Path) -> Result<HostPath, PathError>;
    pub fn resolve(path: &Path, cwd: &HostPath) -> Result<HostPath, PathError>;
    pub fn as_path(&self) -> &Path;
}

impl SandboxPath {
    pub const fn from_static(path: &'static str) -> SandboxPath;
    pub fn parse(text: &str) -> Result<SandboxPath, PathError>;
    pub fn join(&self, segment: &str) -> Result<SandboxPath, PathError>;
    pub fn as_str(&self) -> &str;
}

impl RepoPath {
    pub fn parse(text: &str) -> Result<RepoPath, PathError>;
}

impl ProjectRoot {
    pub async fn open(fs: &dyn FileSystem, path: &HostPath) -> Result<ProjectRoot, PathError>;
    pub fn path(&self) -> &HostPath;
}

pub mod sandbox_layout {
    pub static PROJECT: SandboxPath;
    pub static JACKIN_DIR: SandboxPath;
    pub static RUN_STATE: SandboxPath;
    pub static RUN_START: SandboxPath;
    pub static DOCKER_DIR: SandboxPath;
    pub static HOOKS_DIR: SandboxPath;
    pub static BUNDLE_DIR: SandboxPath;
    pub static GIT: SandboxPath;
    pub static PRINTENV: SandboxPath;
    pub static SHA256SUM: SandboxPath;
    pub static STAT: SandboxPath;
    pub fn fixed() -> [&'static SandboxPath; 11];
    pub fn uploaded_marker(launch: &LaunchId) -> SandboxPath;
    pub fn setup_done_marker(launch: &LaunchId) -> SandboxPath;
    pub fn env_file(launch: &LaunchId) -> SandboxPath;
    pub fn hook_path(kind: HookKind) -> SandboxPath;
    pub fn bundle_path(sandbox: &SandboxName) -> SandboxPath;
}```

`SandboxPath` is absolute, normalized, and has no `..`. `from_static` is `const` and does not validate, because `panic` is denied and a `const fn` cannot return an error: a test in `jackin-core` parses every `sandbox_layout::fixed()` entry and every `AgentSpec::paths()` entry and requires the parsed value to equal the constant. `RepoPath` is relative with no `..` and no empty or `.` segment (D11). `HostPath::resolve` joins a relative CLI argument to the working directory and normalizes it without touching the disk. A `..` segment is `PathError::ParentSegment`. `ProjectRoot::open` checks that the path is an existing directory with a `.git` entry (D08 needs a work tree). `sandbox_layout` is the single owner of every in-sandbox path that jackin and `jackin-start` agree on (D09, D11), including the env file, and of the four absolute program paths that `run_program` uses (`/usr/bin/git`, `/usr/bin/printenv`, `/usr/bin/sha256sum`, `/usr/bin/stat`). `jackin-image` embeds `jackin-start`, and a test there asserts the script text contains each constant, so the script and the Rust cannot drift. `HostPath`, `SandboxPath`, `RepoPath`, and `ProjectRoot` implement `Display`. The text is the value itself, with each control character written as its Rust escape, so a file name cannot carry a control character into a message.

### Secrets and terminal text

```rust
pub struct Secret(Box<str>);
pub struct SecretBytes(Box<[u8]>);
pub struct Redactor { }
pub struct TerminalText(String);

impl Secret {
    pub fn new(value: String) -> Secret;
    pub fn expose(&self) -> &str;
}

impl SecretBytes {
    pub fn new(value: Vec<u8>) -> SecretBytes;
    pub fn expose(&self) -> &[u8];
}

impl Redactor {
    pub fn new(secrets: impl IntoIterator<Item = &'_ Secret>) -> Redactor;
    pub fn apply(&self, text: &str) -> String;
}

impl TerminalText {
    pub fn sanitize(bytes: &[u8]) -> TerminalText;
    pub fn as_str(&self) -> &str;
}
```

`Secret` and `SecretBytes` have a redacting `Debug`, no `Display`, no `Serialize`, and zero their memory on drop. `expose` is the only way out, and the `xtask` layer step lists the files allowed to call it (the OpenShell adapter, the credential resolver, the `auth.json` and Docker client file writers). `TerminalText::sanitize` removes ESC sequences, control characters except `\n` and `\t`, and bidirectional control characters, and replaces invalid UTF-8. Hook output (D11) and build tails pass through it before they enter an error. `TerminalText` implements `Display` and prints the sanitized text as it is.

### Platform, providers, agents

```rust
pub enum Platform { LinuxArm64, LinuxAmd64 }

impl Platform {
    pub fn host_default() -> Platform;
    pub fn docker_arg(self) -> &'static str;
    pub fn target_arch(self) -> &'static str;
}

pub enum Provider { Amp, Antigravity, Claude, Codex, Cursor, Gemini, Grok, Hermes, Kimi, Minimax, Muse, Opencode, Openrouter, Zai }

impl Provider {
    pub const ALL: [Provider; 14];
    pub fn as_str(self) -> &'static str;
    pub fn agent(self) -> Option<Agent>;
}

pub enum Agent { Codex }

impl Agent {
    pub const ALL: [Agent; 1];
    pub fn spec(self) -> &'static AgentSpec;
    pub const fn provider(self) -> Provider;
    pub fn as_str(self) -> &'static str;
}

pub enum CodexAuthMode { ApiKey, ChatgptLogin }
pub struct AgentSpec {
    pub agent: Agent,
    pub provider: Provider,
    pub binary: SandboxPath,
    pub code_mode_host: SandboxPath,
    pub state_dir: SandboxPath,
    pub workdir: SandboxPath,
    pub launch_command: &'static [&'static str],
    pub install: InstallSpec,
    pub openshell: OpenShellSpec,
    pub auth_file: AuthFileSpec,
    pub user_config: ConfigFileSpec,
    pub reserved_env: &'static [&'static str],
}
pub struct InstallSpec {
    pub version: &'static str,
    pub url_template: &'static str,
    pub archive_members: &'static [&'static str],
    pub version_output: &'static str,
    pub system_config: SandboxPath,
    pub system_config_content: &'static str,
    pub targets: &'static [InstallTarget],
}
pub struct InstallTarget { pub platform: Platform, pub triple: &'static str, pub sha256: &'static str }
pub struct OpenShellSpec { pub chatgpt_profile: ProfileAsset, pub api_key_profile: Option<ProfileAsset>, pub keys: CredentialKeys }
pub struct CredentialKeys {
    pub access_token: &'static str,
    pub refresh_token: &'static str,
    pub account_id: &'static str,
    pub token_url: &'static str,
}
pub struct ProfileAsset { pub id: &'static str, pub yaml: &'static str }
pub struct AuthFileSpec { pub path: SandboxPath, pub mode: FileMode }
pub struct ConfigFileSpec { pub path: SandboxPath, pub mode: FileMode }

impl AgentSpec {
    pub fn install_target(&self, platform: Platform) -> Option<&'static InstallTarget>;
    pub fn profile_for(&self, mode: CodexAuthMode) -> Option<&'static ProfileAsset>;
    pub fn profiles(&self) -> Vec<&'static ProfileAsset>;
    pub fn launch_argv(&self) -> Vec<String>;
    pub fn paths(&self) -> Vec<&SandboxPath>;
}
```

`Agent::spec` is a plain function that matches exhaustively on the enum and returns a reference to a `static` row (`static CODEX_SPEC: AgentSpec`), so adding a variant fails to compile until a row exists (D04). The rows are `static`, not `const`, because `SandboxPath` holds a `Cow` and no `const fn` validates a path. `user_config` is the Codex user file `/sandbox/.codex/config.toml` (mode 0600): the Credentials step writes the trust entry for `workdir` and the manifest `[codex] model` into it (section 4; P7.4 F1). `reserved_env` holds `CODEX_HOME`. The embedded profile YAML is a plain `&'static str`; `jackin-openshell` parses it into the endpoints and binaries that the D10 closure check needs, so the host list is derived from the asset and never written twice. `api_key_profile` is `None` until a profile is proven, so `profile_for(ApiKey)` is `None` and a mode A account gets a typed `Unsupported` error from `plan_launch` and `sync_account` (decision 5). The Codex row values come from `rewrite/drafts/agentspec-codex.toml` (P3.1). A test asserts `install.version` equals the `codex_version` of `Compatibility::pinned()` and that every `InstallTarget` platform appears once.

### Labels, phase, supervisor state

```rust
pub struct LabelKey(String);
pub struct LabelValue(String);
pub struct Labels(BTreeMap<LabelKey, LabelValue>);
pub struct PlannedLabels {
    pub workspace: WorkspaceName,
    pub role: RoleName,
    pub agent: Agent,
    pub account: AccountName,
    pub launch: LaunchId,
}
pub struct SandboxLabels { pub planned: PlannedLabels, pub image: ImageLabel }
pub struct LabelSelector(Vec<(LabelKey, LabelValue)>);

impl PlannedLabels {
    pub fn complete(self, image: &ImageId) -> SandboxLabels;
}

impl SandboxLabels {
    pub fn to_labels(&self) -> Labels;
    pub fn from_labels(labels: &Labels) -> Result<SandboxLabels, LabelError>;
}

impl Labels {
    pub fn is_managed(&self) -> bool;
    pub fn iter(&self) -> impl Iterator<Item = (&LabelKey, &LabelValue)>;
}

impl LabelSelector {
    pub fn managed() -> LabelSelector;
    pub fn launch(id: &LaunchId) -> LabelSelector;
    pub fn render(&self) -> String;
}

pub enum Phase {
    Provisioning,
    Starting,
    Ready,
    Stopping,
    Stopped,
    Completed { exit_code: i32 },
    Error { exit_code: Option<i32> },
    Deleting,
    Suspending,
    Suspended,
    Unknown,
}

impl Phase {
    pub fn accepts_exec(self) -> bool;
    pub fn main_process_ended(self) -> bool;
    pub fn exit_code(self) -> Option<i32>;
    pub fn is_settled(self) -> bool;
}

pub enum SupervisorState { Waiting, Running { attempt: AttemptId }, Exited { attempt: AttemptId, code: i32 } }
pub struct StartRequest(pub AttemptId);

impl SupervisorState {
    pub fn parse(text: &str) -> Result<SupervisorState, SupervisorStateError>;
    pub fn render(&self) -> String;
    pub fn attempt(&self) -> Option<AttemptId>;
    pub fn next_attempt(&self) -> Option<AttemptId>;
}

impl StartRequest {
    pub fn parse(text: &str) -> Result<StartRequest, SupervisorStateError>;
    pub fn render(&self) -> String;
}
```

Label keys are `jackin.dev/managed`, `jackin.dev/workspace`, `jackin.dev/role`, `jackin.dev/agent`, `jackin.dev/account`, `jackin.dev/image`, `jackin.dev/launch` (D07). `PlannedLabels` is everything known when the launch is planned; only a `BuiltImage` completes it, through `complete(&ImageId)`, so a sandbox cannot be created with a missing or invented image label. `Phase` is the jackin mapping of the gateway phases; the adapter maps unspecified and unknown values to `Unknown`. `Completed` and `Error` carry the exit code of the main process (D09); `Error` has no code when the sandbox failed before the main process ran. `accepts_exec` is true only for `Ready`. `main_process_ended` is true for `Completed` and `Error`. `is_settled` is false for `Provisioning`, `Starting`, `Stopping`, `Deleting`, `Suspending`, and `Unknown`. `SupervisorState::parse` accepts exactly `waiting`, `running <attempt>`, `exited <attempt> <code>`, where the attempt follows the `AttemptId` grammar. Anything else is an error, because the file is agent-writable and advisory. `next_attempt` is the only way to choose a start request: `Waiting` gives `AttemptId::FIRST`, `Exited { attempt }` gives `attempt.next()`, and `Running` gives `None`, because jackin never writes while Codex runs (P7.4 F5). `jackin-start` honors an ID only when it is greater than the last one it handled since the sandbox started, and a stop and a start reset both the counter and the state to `waiting`, so `FIRST` is valid again (P7.4 F6). Pure.

### Role manifest (D11)

```rust
pub struct RoleManifest {
    pub schema: SchemaVersion,
    pub role: RoleSection,
    pub codex: CodexSection,
    pub docker: DockerSection,
    pub hooks: HooksSection,
    pub env: BTreeMap<EnvName, EnvSpec>,
}
pub struct SchemaVersion;
pub struct RoleSection { pub name: RoleName, pub dockerfile: RepoPath, pub policy: RepoPath }
pub struct CodexSection { pub model: Option<ModelName> }
pub struct DockerSection { pub enabled: bool }
pub struct HooksSection { pub setup: Option<RepoPath>, pub preflight: Option<RepoPath> }
pub struct EnvSpec { pub default: Option<String>, pub prompt: Option<String> }
pub struct ModelName(String);
pub enum HookKind { Setup, Preflight }

impl RoleManifest {
    pub fn parse(text: &str) -> Result<RoleManifest, ManifestError>;
    pub fn render(&self) -> String;
    pub fn hook(&self, kind: HookKind) -> Option<&RepoPath>;
}
```

`SchemaVersion` is a unit type that deserializes only from the integer `1`. `[codex] model` is not read at launch time by jackin directly: `plan_launch` renders it into the Codex `config.toml` (section 4). The parser rejects unknown fields at every level and the error names the full key path (`env.FOO.defualt`). `[env]` values are non-secret (D11); a reserved `EnvName` is `ManifestError::ReservedEnvName`. File checks (regular file in the role repository, no symlink escape) need the filesystem and live in `RoleCatalog` in `jackin-session`.

### Docker capability (D24)

```rust
pub struct EngineConfig { pub port: NonZeroU16, pub client_certs: HostPath }
pub enum DockerSource { Role, Workspace, Both }
pub struct EngineAccess { pub engine: EngineConfig, pub enabled_by: DockerSource }
pub enum DockerCapability { Disabled, Enabled(EngineAccess) }
pub enum DockerLayer { Absent, Client }
pub enum DockerRisk { ClientKeyReadable, ContainersBypassPolicy, BindMountsNameEngineHost, EngineOutlivesSandbox }
pub enum DockerError { EngineMissing { enabled_by: DockerSource } }

impl DockerCapability {
    pub fn resolve(role_enabled: bool, workspace_enabled: bool, engine: Option<&EngineConfig>) -> Result<DockerCapability, DockerError>;
    pub fn layer(&self) -> DockerLayer;
    pub fn environment(&self) -> BTreeMap<EnvName, String>;
}

impl DockerRisk {
    pub const ALL: [DockerRisk; 4];
}

pub const ENGINE_HOST: &str;
```

`resolve` returns `Disabled` when neither the role nor the workspace enables the capability (the default), `Enabled` with the source when the gateway entry has an engine, and `EngineMissing` otherwise. So "enabled without an engine" is unrepresentable. `environment` is empty for `Disabled` and is `DOCKER_HOST=tcp://host.openshell.internal:<port>`, `DOCKER_TLS_VERIFY=1`, `DOCKER_CERT_PATH=/sandbox/.jackin/docker` for `Enabled`. The launch writes these into the env file with the role `[env]` values; the sandbox spec does not carry them. The image sees `DockerLayer` only, so the engine port never reaches the cache key. `DockerRisk::ALL` lists the four D24 warning facts; the CLI and the console map it with an exhaustive `match`. The engine policy rule is built in `jackin-openshell` (`NetworkRule::docker_engine`).

### Env file (D11)

```rust
pub struct EnvFile(BTreeMap<EnvName, String>);
pub enum EnvFileError { InvalidValue { name: EnvName }, Syntax { line: usize } }

impl EnvFile {
    pub fn new(values: BTreeMap<EnvName, String>) -> Result<EnvFile, EnvFileError>;
    pub fn parse(text: &str) -> Result<EnvFile, EnvFileError>;
    pub fn render(&self) -> String;
    pub fn values(&self) -> &BTreeMap<EnvName, String>;
}
```

The env file is the one source of the role environment (D11). `render` writes one line `export NAME='value'` for each entry in name order and writes each `'` inside a value as `'\''`, so `jackin-start` and a shell read it with `.`. `new` rejects a value with a newline or NUL. `parse` accepts exactly what `render` writes, so jackin reads the same file back for hooks, restarts, and shells.

### Config (D15, D24, D26)

TOML shape. Unknown fields are rejected everywhere. There is no schema version.

```toml
default_gateway = "jackin-local"

[gateways.jackin-local]
openshell_workspace = "default"
builder = "orbstack"
platform = "linux/arm64"

[gateways.jackin-local.docker_engine]
port = 18476
client_certs = "/Users/me/.config/jackin/docker/jackin-local"

[defaults.accounts]
codex = "work"
claude = "personal"

[accounts.work]
provider = "codex"
credential = { source = "codex-login", home = "/Users/me/.codex", chatgpt_account_id = "7a1f" }

[accounts.personal]
provider = "claude"
credential = { source = "env", var = "ANTHROPIC_API_KEY" }

[accounts.ci]
provider = "codex"
credential = { source = "op", reference = "op://Dev/OpenAI/credential" }

[roles.sentinel]
source = { path = "/Users/me/Projects/jackin-sentinel" }
bindings = { codex = "work" }

[workspaces.myproj]
path = "/Users/me/Projects/myproj"
docker = false
default_role = "sentinel"
bindings = { codex = "work" }
```

```rust
pub struct Config { }
pub struct GatewayEntry {
    pub openshell_workspace: OpenShellWorkspace,
    pub builder: BuilderName,
    pub platform: Platform,
    pub docker_engine: Option<EngineConfig>,
}
pub struct Account { }
pub enum CredentialSource {
    Env { var: EnvName },
    OnePassword { reference: OpReference },
    File { path: HostPath },
    CodexLogin { home: HostPath, chatgpt_account_id: ChatgptAccountId },
}
pub struct RoleEntry { pub source: RoleSource, pub bindings: BTreeMap<Agent, AccountName> }
pub enum RoleSource { Path(HostPath), Git { url: String, rev: Option<String> } }
pub struct Workspace {
    pub path: HostPath,
    pub docker: bool,
    pub default_role: Option<RoleName>,
    pub bindings: BTreeMap<Provider, AccountName>,
}
pub enum ConfigEdit {
    AddGateway { name: GatewayName, entry: GatewayEntry },
    RemoveGateway(GatewayName),
    SetDefaultGateway(Option<GatewayName>),
    AddAccount(Account),
    RemoveAccount(AccountName),
    SetDefaultAccount { account: AccountName },
    ClearDefaultAccount { provider: Provider },
    BindWorkspaceAccount { workspace: WorkspaceName, account: AccountName },
    UnbindWorkspaceAccount { workspace: WorkspaceName, provider: Provider },
    BindRoleAccount { role: RoleName, agent: Agent, account: AccountName },
    UnbindRoleAccount { role: RoleName, agent: Agent },
    AddRole { name: RoleName, source: RoleSource },
    RemoveRole(RoleName),
    AddWorkspace { name: WorkspaceName, workspace: Workspace },
    RemoveWorkspace(WorkspaceName),
    SetWorkspaceDocker { workspace: WorkspaceName, docker: bool },
    SetWorkspaceDefaultRole { workspace: WorkspaceName, role: Option<RoleName> },
}

impl Config {
    pub fn empty() -> Config;
    pub fn parse(text: &str) -> Result<Config, ConfigError>;
    pub fn render(&self) -> String;
    pub fn apply(&mut self, edit: ConfigEdit) -> Result<(), ConfigError>;

    pub fn default_gateway(&self) -> Option<&GatewayName>;
    pub fn gateway(&self, name: &GatewayName) -> Option<&GatewayEntry>;
    pub fn gateways(&self) -> impl Iterator<Item = (&GatewayName, &GatewayEntry)>;
    pub fn account(&self, name: &AccountName) -> Option<&Account>;
    pub fn accounts(&self) -> impl Iterator<Item = &Account>;
    pub fn default_account(&self, provider: Provider) -> Option<&AccountName>;
    pub fn role(&self, name: &RoleName) -> Option<&RoleEntry>;
    pub fn roles(&self) -> impl Iterator<Item = (&RoleName, &RoleEntry)>;
    pub fn workspace(&self, name: &WorkspaceName) -> Option<&Workspace>;
    pub fn workspaces(&self) -> impl Iterator<Item = (&WorkspaceName, &Workspace)>;

    pub fn workspace_for(&self, path: &HostPath) -> Result<(&WorkspaceName, &Workspace), ConfigError>;
    pub fn workspace_accounts(&self, name: &WorkspaceName) -> Vec<&Account>;
    pub fn resolve_gateway(&self, requested: Option<&GatewayName>) -> Result<(&GatewayName, &GatewayEntry), ConfigError>;
    pub fn select_account(&self, query: &AccountQuery<'_>) -> Result<AccountSelection, SelectionError>;
}

impl Account {
    pub fn new(name: AccountName, provider: Provider, credential: CredentialSource) -> Result<Account, ConfigError>;
    pub fn name(&self) -> &AccountName;
    pub fn provider(&self) -> Provider;
    pub fn credential(&self) -> &CredentialSource;
    pub fn codex_auth_mode(&self) -> Option<CodexAuthMode>;
}

pub struct AccountQuery<'a> {
    pub flag: Option<&'a AccountName>,
    pub workspace: &'a WorkspaceName,
    pub role: &'a RoleName,
    pub agent: Agent,
}
pub struct AccountSelection { pub account: AccountName, pub via: SelectionRule }
pub enum SelectionRule { Flag, WorkspaceBinding, RoleBinding, ProviderDefault, OnlyAccount }
pub enum MissingBinding {
    NoAccountForProvider { provider: Provider },
    SeveralAccountsNoDefault { provider: Provider, accounts: Vec<AccountName> },
}
pub struct ConfigStore { }

impl ConfigStore {
    pub fn new(fs: Arc<dyn FileSystem>, path: HostPath) -> ConfigStore;
    pub fn default_path(home: &HostPath) -> HostPath;
    pub async fn load(&self) -> Result<Config, ConfigError>;
    pub async fn edit(&self, edits: Vec<ConfigEdit>) -> Result<Config, ConfigError>;
}
```

Gateway keys are the registered OpenShell gateway names (D03). `jackin gateway add` writes `builder` (default `default`) and `platform` (default `Platform::host_default()` evaluated at add time), so a stored entry is always explicit and no later code guesses a default. The `docker_engine` entry exists because OpenShell has no place for an engine endpoint (D24).

Invariants that `Config::parse` and `apply` hold, so no later code re-checks them:

- A workspace binding key is a provider and its account has that provider. `BindWorkspaceAccount` takes only an account, so the provider cannot disagree (D26).
- A role binding key is an `Agent` and its account has `agent.provider()`. `BindRoleAccount` fails with `ConfigError::ProviderMismatch` otherwise.
- A default account for provider P has provider P. `CodexLogin` is allowed only for `Provider::Codex` accounts.
- `RemoveAccount` fails with `AccountInUse` while any default, workspace binding, or role binding names it. `RemoveRole` and `RemoveWorkspace` fail while another entry references them. `RemoveGateway` clears `default_gateway` when it names that gateway.
- Names are unique per table. A gateway `docker_engine.port` is non-zero.
- `resolve_gateway(None)` returns `default_gateway`, else the only configured gateway, else `NoGateway` or `AmbiguousGateway`.
- `select_account` is pure and follows D26: flag, workspace binding for `agent.provider()`, role binding for `agent`, default of the provider, the only account of the provider. If no rule selects, the error carries `MissingBinding` so the message names what to bind.

`ConfigStore::edit` takes an exclusive lock on `config.toml.lock` through `FileSystem::lock`, re-reads and parses, applies all edits in order, validates, and writes with `write_atomic` (temporary file in the same directory, `write_new`, `sync`, `rename`, `sync_dir`). If any edit fails, nothing is written. Two jackin processes (CLI and console) serialize on the lock, so there is no lost update. `render` is deterministic (sorted tables), so equal configs give equal bytes. The file holds no secret; a `CredentialSource` is a pointer.

### Boundary traits

These are the traits tests may fake, plus the closed `Program` list.

```rust
pub struct Timestamp(pub i64);
pub struct EnvSnapshot(BTreeMap<String, String>);

impl EnvSnapshot {
    pub fn from_pairs(pairs: impl IntoIterator<Item = (String, String)>) -> EnvSnapshot;
    pub fn get(&self, name: &EnvName) -> Option<&str>;
}

#[async_trait]
pub trait Clock: Send + Sync + Debug {
    fn now(&self) -> Timestamp;
    async fn sleep(&self, duration: Duration);
}

#[async_trait]
pub trait FileSystem: Send + Sync + Debug {
    async fn read(&self, path: &HostPath) -> Result<Option<Vec<u8>>, FsError>;
    async fn stat(&self, path: &HostPath) -> Result<Option<FileKind>, FsError>;
    async fn stat_following(&self, path: &HostPath) -> Result<Option<FileKind>, FsError>;
    async fn file_len(&self, path: &HostPath) -> Result<Option<u64>, FsError>;
    async fn list(&self, dir: &HostPath) -> Result<Vec<DirEntry>, FsError>;
    async fn create_dir_all(&self, dir: &HostPath) -> Result<(), FsError>;
    async fn create_private_dir_all(&self, dir: &HostPath) -> Result<(), FsError>;
    async fn write_new(&self, path: &HostPath, bytes: &[u8], mode: FileMode) -> Result<(), FsError>;
    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), FsError>;
    async fn remove_file(&self, path: &HostPath) -> Result<(), FsError>;
    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), FsError>;
    async fn mode(&self, path: &HostPath) -> Result<Option<FileMode>, FsError>;
    async fn sync_dir(&self, dir: &HostPath) -> Result<(), FsError>;
    async fn lock(&self, path: &HostPath) -> Result<FileLock, FsError>;
    async fn read_regular(&self, root: &HostPath, file: &RepoPath) -> Result<Result<RegularFile, RegularFileProblem>, FsError>;
    async fn read_at(&self, path: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError>;
}

pub struct FileMode(pub u32);
pub enum FileKind { File, Dir, Symlink, Socket }
pub struct RegularFile { pub bytes: Vec<u8>, pub mode: FileMode }
pub enum RegularFileProblem { Symlink, NotARegularFile, Missing }
pub struct DirEntry { pub name: String, pub kind: FileKind }
pub struct FileLock { }
pub struct StdFileSystem { }

pub async fn write_atomic(fs: &dyn FileSystem, path: &HostPath, bytes: &[u8], mode: FileMode) -> Result<(), FsError>;

pub enum Program { Docker(DockerSubcommand), Openshell, Git, Ssh, Op }
pub enum DockerSubcommand { BuildxBuild, BuildxCreate, BuildxInspect, ImageInspect, Push }

impl DockerSubcommand {
    pub fn argv(self) -> &'static [&'static str];
}

impl Program {
    pub fn binary(self) -> &'static str;
}
pub struct ProcessSpec {
    pub program: Program,
    pub args: Vec<String>,
    pub cwd: Option<HostPath>,
    pub env: Vec<(String, EnvValue)>,
    pub env_remove_prefixes: Vec<String>,
    pub stdin: ProcessStdin,
    pub timeout: Duration,
}
pub enum EnvValue { Plain(String), Secret(Secret) }
pub enum ProcessStdin { Null, Bytes(SecretBytes), File { path: HostPath, offset: u64 } }
pub struct ProcessOutput { pub status: ProcessStatus, pub stdout: Vec<u8>, pub stderr: Vec<u8> }
pub enum ProcessStatus { Exited(i32), Signaled(i32) }
pub struct OutputLine { pub stream: OutputStream, pub text: String }
pub enum OutputStream { Stdout, Stderr }
pub type LineSink = tokio::sync::mpsc::Sender<OutputLine>;

#[async_trait]
pub trait ProcessRunner: Send + Sync + Debug {
    async fn run(&self, spec: &ProcessSpec, lines: Option<&LineSink>, cancel: &CancellationToken) -> Result<ProcessOutput, ProcessError>;
}

pub struct PollPlan { pub interval: Duration, pub timeout: Duration }
pub enum Poll<T> { Ready(T), Pending }

pub async fn poll_until<T, E, Fut, F>(
    clock: &dyn Clock,
    plan: PollPlan,
    cancel: &CancellationToken,
    probe: F,
) -> Result<T, PollError<E>>
where
    F: FnMut() -> Fut + Send,
    Fut: Future<Output = Result<Poll<T>, E>> + Send;
```

`ProcessStdin::Bytes` carries a secret, so a secret never sits in `args` (D10). With `lines` set, the runner streams complete lines to the sink and returns a stdout and stderr tail of at most 64 KiB; with `lines` unset it returns all output. Cancellation kills the child and returns `ProcessError::Cancelled`. The runner ends the whole process group of the child through `ProcessGroup::stop` and does not return until the group holds no live process: it sends SIGKILL to the group, and where the group signal is denied (an OpenShell workload filters `kill` with a negative pid) it lists the group in `/proc` and sends SIGKILL to each live member. A zombie is not a member, and a `/proc/<pid>/stat` line that cannot be parsed is `GroupFault::Unreachable`, not a non-member. A group that is still alive after 5 s is `GroupFault::Alive`. The fault rides on `Timeout` and `Cancelled` (field `group`), so a caller that matches `Cancelled { .. }` still sees the cancel; for the other errors, and for a clean exit, it becomes `ProcessError::Group`. `GatewayError::{Timeout, Cancelled}`, `EngineError::Cancelled` and `ExportError::Cancelled` carry the same `group`, and their `Display` appends `; <fault>`. `ProcessGroup` lives in `jackin-session` (`process_group.rs`), and `xtask` stops its children through it; `GroupFault` stays in `jackin-core`. `ImageError::Cancelled` carries the `group` of the `EngineError::Cancelled` it comes from. The runner always applies `spec.timeout` and takes stdin from `/dev/null` for `ProcessStdin::Null` (D03, P0.2 G1). The real `ProcessRunner` and `Clock` live in `jackin-session::host`, and so does `capture_env`, the only reader of the process environment; `StdFileSystem` lives here.

`FileSystem` rules. `stat` does not follow a symlink, and `stat_following` does; both report a socket as `FileKind::Socket`. `create_private_dir_all` creates every missing level with mode 0700. `file_len` returns the length of the file behind the path, or `None` when it is absent. `read_at(path, offset, len)` returns at most `len` bytes from `offset`, never pads a short read, and fails with `Io` for a missing file or a directory. `ProcessStdin::File { path, offset }` feeds the file from `offset` to the end of the input. The runner opens and seeks the file before it starts the child, so a missing file or a path that is not a regular file refuses the run and starts nothing.

### Serde views printed by `--json`

```rust
pub struct LsOutput { pub schema: u32, pub sandboxes: Vec<SandboxSummary> }
pub struct SandboxSummary { pub name: SandboxName, pub labels: SandboxLabels, pub phase: Phase }
pub struct InspectView {
    pub schema: u32,
    pub summary: SandboxSummary,
    pub providers: Vec<ProviderName>,
    pub policy: PolicyStatus,
    pub closure_violations: Vec<ClosureViolationView>,
    pub supervisor: SupervisorReading,
}
pub struct PolicyStatus { pub hash: Option<String>, pub version: Option<u32>, pub loaded: bool }
pub struct ClosureViolationView { pub rule: String, pub host: String, pub provider_host: String }
pub enum SupervisorReading { Unreadable, Reported(SupervisorState), Invalid(String) }
pub struct Compatibility {
    pub openshell_version: String,
    pub openshell_tag: String,
    pub openshell_commit: String,
    pub codex_version: String,
}

impl Compatibility {
    pub fn pinned() -> Result<Compatibility, CompatibilityError>;
}

pub struct BuildOutput { pub schema: u32, pub image: ImageTag, pub id: ImageId }
```

These views derive `Serialize` and `Deserialize` and carry `schema: 1`; their field names are pinned by key-set tests, not by a generated JSON Schema (P6.1, OD-7). `xtask e2e` parses `jackin ls --json` and `jackin inspect --json` into them. `Compatibility::pinned` parses `compatibility.lock.toml` embedded with `include_str!` and returns an error value, so the OpenShell pin has one file (D02) and no panic path. The binary calls it once and passes the value to `doctor`. `BuildOutput` is what `jackin build --json` prints, so the final image ID is observable without `docker image inspect` (J03). The usage view belongs to `jackin-usage` (section 5).

### Errors

```rust
pub enum IdError {
    Empty { kind: &'static str },
    TooLong { kind: &'static str, max: usize, len: usize },
    InvalidChar { kind: &'static str, ch: char, index: usize },
    BadEdge { kind: &'static str },
    BadFormat { kind: &'static str, expected: &'static str },
}
pub enum PathError {
    NotAbsolute(String),
    NotRelative(String),
    ParentSegment(String),
    EmptySegment(String),
    NotADirectory(HostPath),
    NotAGitWorkTree(HostPath),
    Fs(FsError),
}
pub enum LabelError { Missing { key: &'static str }, Invalid { key: &'static str, source: IdError }, UnknownAgent(String) }
pub enum SupervisorStateError { Empty, UnknownWord(String), BadAttempt(IdError), BadExitCode(String), TrailingText(String) }
pub enum CompatibilityError { Syntax { message: String }, MissingField { key: String } }
pub enum ManifestError {
    Syntax { line: usize, column: usize, message: String },
    UnknownField { key: String },
    MissingField { key: String },
    InvalidValue { key: String, message: String },
    UnsupportedSchema { found: i64 },
    ReservedEnvName(EnvName),
}
pub enum ConfigError {
    Syntax { line: usize, column: usize, message: String },
    UnknownField { key: String },
    DuplicateName { kind: &'static str, name: String },
    MissingReference { kind: &'static str, name: String },
    ProviderMismatch { account: AccountName, expected: Provider, found: Provider },
    CredentialNotAllowed { provider: Provider },
    AccountInUse { account: AccountName, by: Vec<ConfigReference> },
    RoleInUse { role: RoleName, by: Vec<ConfigReference> },
    NoGateway,
    AmbiguousGateway { names: Vec<GatewayName> },
    UnknownGateway(GatewayName),
    NoWorkspaceForPath(HostPath),
    Fs(FsError),
}
pub enum ConfigReference {
    Default(Provider),
    WorkspaceBinding(WorkspaceName),
    RoleBinding(RoleName),
    WorkspaceDefaultRole(WorkspaceName),
}
pub enum SelectionError {
    UnknownFlagAccount(AccountName),
    FlagProviderMismatch { account: AccountName, expected: Provider },
    NoAccount { missing: MissingBinding },
}
pub enum FsError { Io { path: HostPath, kind: std::io::ErrorKind, message: String }, AlreadyExists(HostPath), LockHeld(HostPath) }
pub enum ProcessError {
    Spawn { program: Program, message: String },
    Timeout { program: Program, after: Duration, group: Option<GroupFault> },
    Cancelled { program: Program, group: Option<GroupFault> },
    OutputTooLarge { program: Program, stream: OutputStream, limit: usize },
    Read { program: Program, stream: OutputStream, message: String },
    Group { program: Program, fault: GroupFault },
}
pub enum GroupFault { Alive { after: Duration }, Unreachable { message: String } }
impl GroupFault { pub fn note(group: Option<&GroupFault>) -> String }
pub enum PollError<E> { Timeout { after: Duration }, Cancelled, Probe(E) }
```

All of these derive `Debug` and `thiserror::Error`. No variant holds a `Secret`. Errors that carry process or hook text hold `TerminalText` that already passed a `Redactor`. The `Display` of `ConfigError::NoWorkspaceForPath` names `jackin workspace add` (decision 8).

Error text. `display.rs` holds the three helpers that messages use.

```rust
pub fn plain(value: impl Display) -> String;
pub fn plain_list<T: Display>(items: &[T]) -> String;
pub fn plain_duration(duration: &Duration) -> String;
```

`plain` prints printable text as it is and each control character as its Rust escape. The characters `"`, `'`, and `\` stay as typed. `plain_list` passes each item through `plain`, joins them with `, `, and prints `none` for an empty list. `plain_duration` prints the compact form (`30s`, `250ms`). An `#[error]` message follows three rules, and `xtask ci --only errors` enforces them. It has no Debug placeholder, which is any format spec that ends in `?`. It names no field marked `#[source]` or `#[from]`, so the cause prints once, through `source()`. It prints no `String` field bare. Text that came from outside (a gateway, a file, a tool, the user) goes through `plain`, a collection through `plain_list`, and a duration through `plain_duration`, as format arguments: `#[error("... {}", plain_list(.0))]`. The four string variants of `PathError`, `SupervisorStateError::{UnknownWord, BadExitCode, TrailingText}`, `IdError::InvalidChar.ch`, and every other `String` field of an error enum follow this.

Types that appear in messages implement `Display` and print words, not Rust syntax. These are `HostPath`, `SandboxPath`, `RepoPath`, `ProjectRoot`, `TerminalText`, `Phase`, `Program`, `ProcessStatus`, `OutputStream`, `DockerSource`, `HookKind`, `CodexAuthMode`, `Platform`, and `SupervisorReading`. `Phase` prints `provisioning`, `starting`, `ready`, `stopping`, `stopped`, `completed with exit code N`, `error with exit code N`, `error without an exit code`, `deleting`, `suspending`, `suspended`, or `unknown`. `ProcessStatus` prints `exit code N` or `signal N`. `Program` prints the binary and its leading arguments. `Platform` prints its Docker argument (`linux/arm64`). `CodexAuthMode` prints `api key` or `chatgpt login`. `DockerSource` prints `the role`, `the workspace`, or `both the role and the workspace`. `SupervisorReading` prints `unreadable`, the rendered state, or `invalid (<text>)`.

### Test plan outline (mark for `test-reviewer`)

Location: `crates/jackin-core/tests/*.rs`, integration tests through the public API, using `MemFs` and `FakeClock` from `jackin-testkit`. Boundaries faked: filesystem and clock. Nothing else is needed: this crate has no gateway or network.

| Behavior | Positive | Negative |
|---|---|---|
| Ids parse once | each valid id round-trips through serde and `Display`; `LaunchId::generate` yields a valid id; `AttemptId` parses `1` and `999999999999999999`, `FIRST.next()` is 2, and the largest value has no `next` | empty, 64 characters, leading `-`, trailing `.`, space, non-ASCII each rejected with its `IdError` variant and the offending index; `AttemptId` rejects `0`, `01`, `a1`, `-1`, and 19 digits |
| Sandbox name fits the gateway (decision 1) | for 200 generated workspace names (long, unicode, empty, punctuation) `SandboxName::new(slug, suffix)` is DNS-1123, at most 19, no `--`; `NameSuffix::for_launch` is stable and differs across a fixed table of launch IDs | `SandboxName::parse` rejects 20 characters, uppercase, `--` |
| Image label (decision 2) | `label_value` is 32 hex and a valid label value; `PlannedLabels::complete` yields labels that `from_labels` parses back | `ImageId::parse` rejects a non-sha256 prefix and a wrong length |
| Paths and text | `SandboxPath` and `RepoPath` normalize; every `sandbox_layout::fixed()` constant and every `AgentSpec::paths()` entry parses to itself; CSI, OSC, and bell are removed from sample hook output | `..`, absolute `RepoPath`, relative `SandboxPath`, empty segment rejected; `ProjectRoot::open` rejects a directory without `.git` |
| Secrets never render | `format!("{:?}", secret)` and every error `Display` in the crate lack the secret value; `Redactor` masks it in a stderr sample | an error built from process stderr that contains the secret renders masked |
| Agent table | `Agent::ALL` rows pass: install version equals the lock file (`Compatibility::pinned()` is `Ok` on the shipped file), platforms unique, launch argv equals the D04 command, `user_config` is `/sandbox/.codex/config.toml`, `profile_for(ChatgptLogin)` is `Some` | `profile_for(ApiKey)` is `None` (decision 5); `CODEX_HOME` and each OpenShell credential key are reserved `EnvName`s |
| Manifest | the D11 example parses and `render` then `parse` is equal | unknown top-level field, unknown nested field (`env.X.defualt`), `schema = 2`, absolute hook path, `..` hook path, reserved `JACKIN_X` env name each give the right `ManifestError` naming the key |
| Docker capability | neither flag gives `Disabled`; role only, workspace only, and both give `Enabled` with the matching `DockerSource`; `layer` is `Client`; `environment` has the three `DOCKER_*` names | enabled with no engine gives `EngineMissing { enabled_by }`; `Disabled.environment()` is empty |
| Env file (D11) | `render` then `parse` is equal for values with quotes, spaces, `$`, and a backslash; the output is `export NAME='value'` lines in name order | `new` rejects a value with a newline or NUL; `parse` rejects a line that is not an `export`, an unterminated quote, and trailing text |
| Config round trip and edits | the example config parses, `render` then `parse` is equal, a second `render` is byte-equal; each `ConfigEdit` changes only what it names; `RemoveGateway` of the default clears it | unknown field names its key; a binding to an unknown account or an account of the wrong provider; `RemoveAccount` of a bound account gives `AccountInUse` listing every reference; a failed edit leaves the config unchanged |
| Gateway and workspace lookup (decisions 7 and 8) | `resolve_gateway(None)` returns the default, else the only gateway; `workspace_for` finds a workspace by path | two gateways and no default give `AmbiguousGateway`; an unknown name gives `UnknownGateway`; an unknown path gives `NoWorkspaceForPath` and its `Display` contains `jackin workspace add` |
| Account selection (D26, J17) | a table test over the five rules, each winning when the earlier rules are absent | none selected gives `NoAccount` with `MissingBinding`; a flag naming an account of another provider gives `FlagProviderMismatch` |
| Store | `edit` writes once, and a second concurrent `edit` on the same `MemFs` serializes and keeps both changes | a fault in `write_new`, `rename`, or `sync_dir` leaves the old bytes readable and no temporary file listed after the next `load` |
| Supervisor state and phase | `waiting`, `running 1`, `exited 1 3` parse and render identically; `next_attempt` is `FIRST` for `waiting`, 4 for `exited 3 0`, and `None` for `running 1` and for `exited` at the 18-digit maximum; `Completed { exit_code: 0 }` and `Error { exit_code: Some(3) }` report `main_process_ended` and the code; only `Ready` accepts exec | empty, `running`, `exited 1`, `exited 1 x`, `running a1`, `running 0`, `running 01`, trailing text, and an uppercase word are rejected; `Error { exit_code: None }` has no code |
| Poll | `poll_until` returns when the probe turns ready, under the fake clock with no real sleep | timeout after exactly `timeout` of fake time; cancel returns `Cancelled` |

---

## 2. jackin-image

Role: turn a loaded role into an image the gateway can start (D05, D06, D23). The only crate that names `Program::Docker`. It builds three images through `docker buildx build` with a builder whose driver is `docker`, and reads image IDs with `docker image inspect`. Under the `docker-local` method (D23) the `--load` into the engine is the delivery, so there is no separate delivery call. It never runs `docker run`, `create`, `start`, `exec`, or a Docker API client.

### Module map

```text
crates/jackin-image/
  images/construct/Dockerfile      embedded (include_str)
  images/derived/Dockerfile        embedded
  images/derived/scan              embedded POSIX sh, run by the scan stage of the derived build
  images/jackin-start              embedded, installed at /usr/local/bin/jackin-start
  src/lib.rs
  src/dockerfile.rs   validate_dockerfile, DockerfileError
  src/request.rs      ImageRequest, SourceFile, HookFiles, RoleRevision, Rebuild
  src/key.rs          ImageKey
  src/plan.rs         ImagePlan, PlannedBuild, FinalBuilds, BuildStage, ContextSource
  src/engine.rs       ImageEngine, BuildxEngine, EngineError
  src/ensure.rs       ensure_image, BuiltImage, ImageSource, ImageEvent, ProgressSink
  src/error.rs        ImageError
```

### Public types and signatures

```rust
pub struct SourceFile { pub path: RepoPath, pub bytes: Vec<u8> }
pub struct HookFiles { pub setup: Option<SourceFile>, pub preflight: Option<SourceFile> }
pub enum RoleRevision { Clean { commit: CommitId }, Dirty { commit: CommitId, digest: Sha256 } }
pub enum Rebuild { No, Yes }

impl From<bool> for Rebuild {
    fn from(flag: bool) -> Rebuild;
}

pub struct ImageRequest {
    pub role: RoleName,
    pub context: HostPath,
    pub dockerfile: SourceFile,
    pub hooks: HookFiles,
    pub revision: RoleRevision,
    pub agent: Agent,
    pub platform: Platform,
    pub builder: BuilderName,
    pub docker: DockerLayer,
    pub rebuild: Rebuild,
}

pub fn validate_dockerfile(text: &str) -> Result<(), DockerfileError>;

pub struct ImageKey(Sha256);

impl ImageKey {
    pub fn derive(request: &ImageRequest, construct: &ImageId) -> Result<ImageKey, ImageError>;
    pub fn sha256(&self) -> &Sha256;
}

pub enum BuildStage { Construct, Role, Derived }
pub struct PlannedBuild { pub stage: BuildStage, pub tag: ImageTag, pub args: Vec<String>, pub context: ContextSource }
pub enum ContextSource { RoleRepository(HostPath), Materialized(Vec<MaterializedFile>) }
pub struct MaterializedFile { pub name: String, pub bytes: Vec<u8>, pub mode: FileMode }
pub struct FinalBuilds { pub key: ImageKey, pub role: PlannedBuild, pub derived: PlannedBuild }
pub struct ImagePlan { }

impl ImagePlan {
    pub fn new(request: &ImageRequest) -> Result<ImagePlan, ImageError>;
    pub fn construct_build(&self) -> &PlannedBuild;
    pub fn final_tag(&self, construct: &ImageId) -> ImageTag;
    pub fn final_builds(&self, construct: &ImageId) -> FinalBuilds;
}

pub enum ImageSource { Cache, Built }
pub struct BuiltImage { pub tag: ImageTag, pub id: ImageId, pub source: ImageSource }
pub enum ImageEvent {
    StageStarted(BuildStage),
    StageCached(BuildStage),
    Line { stage: BuildStage, text: TerminalText },
    StageFinished(BuildStage),
}
pub type ProgressSink = tokio::sync::mpsc::Sender<ImageEvent>;

#[async_trait]
pub trait ImageEngine: Send + Sync + Debug {
    async fn find(&self, tag: &ImageTag) -> Result<Option<ImageId>, EngineError>;
    async fn check_builder(&self, builder: &BuilderName) -> Result<(), EngineError>;
    async fn build(&self, build: &PlannedBuild, progress: &ProgressSink, cancel: &CancellationToken) -> Result<ImageId, EngineError>;
}

pub struct BuildxEngine { }

impl BuildxEngine {
    pub fn new(runner: Arc<dyn ProcessRunner>, fs: Arc<dyn FileSystem>, work_dir: HostPath) -> BuildxEngine;
}

pub async fn ensure_image(
    engine: &dyn ImageEngine,
    request: &ImageRequest,
    progress: &ProgressSink,
    cancel: &CancellationToken,
) -> Result<BuiltImage, ImageError>;
```

Semantics.

- `validate_dockerfile` parses stages (`FROM ... AS name`), ignores `ARG` before the first `FROM` and parser directives, and requires that the final stage's base is exactly `construct` (D05 step 1). It runs in `RoleCatalog`, `jackin role check`, and `ImagePlan::new`, so the same error appears in all three.
- `ImagePlan::new` is pure. The construct build exists at once: `-f` the embedded Dockerfile, a materialized context, and a tag that hashes the embedded construct Dockerfile, its pinned base digest, and the platform. The role and derived builds depend on the construct image ID, so they come from `final_builds(&construct)`.
- `ImageKey::derive` is the SHA-256 of a length-prefixed concatenation of: role name, `RoleRevision` (commit, plus the content digest when dirty), the role Dockerfile digest, the built construct image ID, the Codex version and the archive checksum of the platform from the `AgentSpec`, the platform, the `DockerLayer`, the digest of the embedded `jackin-start`, the digest of the embedded derived Dockerfile, the digest of the embedded `scan`, and the path and digest of each hook file. The function destructures `ImageRequest` exhaustively, so a new request field that does not feed the key does not compile. The construct image ID, not the construct Dockerfile or its base digest, is the input (D05): a rebuilt construct with a new ID changes every final tag.
- The final tag is `ImageTag::for_image(role, key)`, and the role-stage tag is `jackin-role/<role>:<16 hex>`. Both come from the key, so tag equals cache key.
- `final_builds` renders two `PlannedBuild` values: role (`--build-context construct=docker-image://<construct tag>`, context is the role repository) and derived (`--build-context role=docker-image://<role tag>` and `--build-context construct=docker-image://<construct tag>`, context materialized with `jackin-start`, `scan`, and the hook files). Each carries `--builder <name>`, `--platform <platform>`, `--provenance=false`, `--sbom=false`, `--load`, `--progress=plain`, and `--no-cache` when `Rebuild::Yes` (D05). The derived build passes `CODEX_VERSION`, `CODEX_BIN_DIR`, and both archive checksums from the `AgentSpec` row, and the Docker client argument only for `DockerLayer::Client` (D06).
- `ensure_image` runs in this order. Validate the Dockerfile. `find` the construct tag, or build it when absent or when `Rebuild::Yes`, and take its ID. Derive the final tag. `find` the final tag and return `ImageSource::Cache` on a hit unless `Rebuild::Yes`. Otherwise `check_builder` once, build the role stage, build the derived stage, and return `ImageSource::Built` with the ID that the derived build returned. Build lines stream to `progress` as `Line` events. A cancel kills the build and returns `ImageError::Cancelled`. Each final-stage build re-reads its source tags before it starts and after it ends. The role build fences the construct tag, and the derived build fences the role tag and the construct tag. A tag that no longer holds the ID that `ensure_image` read fails that stage with `ImageError::SourceMoved`, and no later stage starts.
- `BuildxEngine::check_builder` runs `docker buildx inspect <builder>` and requires the driver `docker` (`EngineError::BuilderDriver`, D05 step 3). `build` writes the materialized context into a fresh per-call directory under `work_dir` through `FileSystem::write_new`, runs `docker buildx build` with a metadata file, reads the image ID, and removes the directory on every exit path. `find` runs `docker image inspect --format {{.Id}}`.
- The derived Dockerfile has four stages after the archive stages. `codex` (from `construct`) installs the pinned Codex files and `/etc/codex/config.toml` and checks `--version`. `assembled` (from `role`) copies the Codex files from `codex` as root with mode 0755, the config with `--chmod=u=rwX,go=rX` so that the created `/etc/codex` stays searchable, then adds `jackin-start` and the hooks and repairs the owner and mode of their directories. `scan` (from `construct`, as root) mounts `assembled` read-only at `/image` and `codex` at `/trusted`, runs `/bin/sh /jackin-scan`, and writes its output to `/verdict`. The final stage (from `assembled`) copies the verdict to `/usr/local/lib/jackin/image-scan` and sets `USER sandbox`. The scan runs only tools from the construct image, so a role cannot replace them. Its exit status gates the stage, and the `COPY --from=scan` of the final stage makes BuildKit build it.
- `images/derived/scan` refuses a setuid or setgid file, a file capability, one of 19 tool names anywhere in the tree as an executable file or a symlink (`sudo`, `su-exec`, `gosu`, `doas`, `iptables`, `ip6tables`, `nft`, `ufw`, `docker`, `podman`, `nerdctl`, `dockerd`, `containerd`, `containerd-shim`, `containerd-shim-runc-v2`, `runc`, `crun`, `buildkitd`, `rootlesskit`), and a Codex file that differs (`cmp`) from the copy in the `codex` stage. Each protected path (`jackin-start`, both hooks, and the two Codex files) must be a regular executable file, and each of its ancestors a real directory with no symlink, owned by `0:0` and not writable by group or others. A failing or missing `find` or `getcap` fails the build. It never runs a file under its root argument. Because the Codex files are installed in the trusted stage and compared, a planted `codex` file or symlink in the role is replaced or refused.
- `ensure_image` is idempotent. The tag is the key, so a second call with equal inputs finds the tag and builds nothing. Two concurrent calls for one key may both build and both succeed with the same ID; the cost is a duplicate build, never a wrong image.

### Error enums

```rust
pub enum DockerfileError {
    NoStages,
    FinalStageNotConstruct { line: usize, found: String },
    UnreadableFrom { line: usize, text: String },
    NotUtf8,
}
pub enum EngineError {
    BuilderMissing { builder: BuilderName },
    BuilderDriver { builder: BuilderName, driver: String },
    BuildFailed { stage: BuildStage, status: ProcessStatus, tail: TerminalText },
    DigestUnreadable { tag: ImageTag, output: TerminalText },
    Process(ProcessError),
    Fs(FsError),
    Cancelled { group: Option<GroupFault> },
}
pub enum ImageError {
    Dockerfile(DockerfileError),
    MissingInstallTarget { platform: Platform },
    Engine(EngineError),
    NotDelivered { tag: ImageTag, built: ImageId, found: Option<ImageId> },
    SourceMoved { tag: ImageTag, expected: ImageId, found: Option<ImageId> },
    LockTimeout { key: ImageTag },
    Cancelled { group: Option<GroupFault> },
}
```

### Test plan outline (mark for `test-reviewer`)

Location: `crates/jackin-image/tests/*.rs`. Boundary faked: the image engine, through `FakeEngine` from `jackin-testkit` (an in-memory image store). `BuildxEngine` argument tests fake the `docker` program only, through `ScriptedProcessRunner`. Pure tests need no fake. Real `ensure_image`, `ImagePlan`, and `ImageKey` code runs in every test.

| Behavior | Positive | Negative |
|---|---|---|
| Dockerfile validation | final stage `FROM construct AS role`; multi-stage with the last `FROM construct`; leading `ARG` and a `# syntax=` line | final stage `FROM ubuntu`; `construct` only in a middle stage; no `FROM`; an unresolvable variable: each with the right `DockerfileError` and line |
| Key sensitivity (J00, decisions 3 and 4) | changing only one input changes the final tag (table over role name, revision commit, dirty digest, Dockerfile, each hook, Codex version, platform, `DockerLayer`, `jackin-start`, derived Dockerfile, and the construct image ID that `FakeEngine` reports) | equal requests give equal tags; `Clean` and `Dirty` with the same commit differ; hook file order does not change the key |
| Plan arguments | the role and derived builds name the explicit builder, platform, `--provenance=false`, `--sbom=false`, `--load`, and the named contexts; `--no-cache` only with `Rebuild::Yes`; the Docker client argument only for `Client` | a platform without an `InstallTarget` gives `MissingInstallTarget` |
| Derived layer uses the agent row | the derived build args hold the binary dir and checksums from `Agent::Codex.spec()`; the embedded Dockerfile references only those args | no embedded Dockerfile text contains a literal Codex version or checksum |
| `ensure_image` builds then caches (J00) | first call builds construct, role, and derived and returns `Built`; the second call with the same request builds nothing and returns `Cache` with the same ID; a changed construct ID rebuilds only role and derived | `Rebuild::Yes` builds all three again; a failing role build returns `BuildFailed { stage: Role }` with a sanitized tail and builds no derived image; a cancel returns `Cancelled` |
| Build output is typed | the `RUN` output of the role build arrives as `Line` events in order (J00) | ESC sequences in a build line are removed |
| Builder check | a builder with driver `docker` passes; a cache hit returns `Cache` even when the configured builder has the `docker-container` driver | a `docker-container` builder fails with `BuilderDriver` and `FakeEngine` holds no new image |
| Docker subcommands (D01) | every `ProcessSpec` that crossed the scripted runner starts with `buildx build`, `buildx inspect`, or `image inspect` | the xtask container step (not a unit test) rejects `docker run` anywhere in the source |
| `jackin-start` and layout agree | the embedded script text contains each `sandbox_layout` path constant it uses | a changed constant fails the test |

Live proof (outside nextest): P4.3 builds the construct and a minimal role, builds again from the cache, and starts the delivered image on the gateway; `xtask e2e` J00 builds `sentinel` twice and compares the tags.

---

## 3. jackin-openshell

Role: everything that talks to a gateway. It defines the `Gateway` trait that `jackin-session` uses, the real `GrpcGateway`, the terminal session handle, gateway discovery, and the policy model. It is the only crate that depends on `openshell-sdk`, `openshell-policy`, `tonic`, and `russh`. No type from those crates appears in any public signature. The `openshell` CLI is used for exactly three things (D03): `sandbox upload`, `sandbox download`, and `gateway list -o json`.

### Module map

```text
crates/jackin-openshell/src/
  lib.rs
  gateway.rs        Gateway trait and the request and result types
  terminal.rs       TerminalSession, TerminalDriver, TerminalRequest, TerminalEvent
  discovery.rs      registered_gateways, resolve_gateway, ResolvedGateway
  policy.rs         PolicyDocument, NetworkRule, HostPattern, RuleKey, ProviderProfile, ClosureViolation
  policy/yaml.rs    private: YAML parse and render and proto conversion over openshell-policy
  error.rs          GatewayError, DiscoveryError, PolicyError
  grpc.rs           GrpcGateway, GrpcGatewayConfig
  grpc/{connect,sandbox,provider,program,shell,main_pty}.rs   adapter files by concern; main_pty.rs holds CreateSshSession, ForwardTcp, russh, host key pinning
  cli.rs            argv for upload, download, gateway list (pub(crate))
  transfer.rs       two-call upload, download, digest check (pub(crate))
  contract.rs       contract suite (feature "contract")
```

### The Gateway trait

```rust
#[async_trait]
pub trait Gateway: Send + Sync + Debug {
    fn name(&self) -> &GatewayName;

    async fn health(&self) -> Result<GatewayHealth, GatewayError>;

    async fn ensure_profile(&self, profile: &ProviderProfile) -> Result<ProfileSync, GatewayError>;
    async fn delete_profile(&self, id: &ProfileId) -> Result<(), GatewayError>;
    async fn provider_state(&self, name: &ProviderName) -> Result<ProviderState, GatewayError>;
    async fn put_provider(&self, spec: &ProviderSpec) -> Result<(), GatewayError>;
    async fn delete_provider(&self, name: &ProviderName) -> Result<(), GatewayError>;

    async fn create_sandbox(&self, spec: &SandboxCreate) -> Result<SandboxView, GatewayError>;
    async fn find_sandboxes(&self, query: &SandboxQuery) -> Result<Vec<SandboxView>, GatewayError>;
    async fn sandbox_detail(&self, name: &SandboxName) -> Result<SandboxDetail, GatewayError>;
    async fn set_power(&self, name: &SandboxName, action: PowerAction) -> Result<SandboxView, GatewayError>;
    async fn delete_sandbox(&self, name: &SandboxName) -> Result<(), GatewayError>;

    async fn run_program(&self, run: &ProgramRun, cancel: &CancellationToken) -> Result<ProgramOutcome, GatewayError>;
    async fn read_files(&self, sandbox: &SandboxName, paths: &[SandboxPath]) -> Result<Vec<Option<Vec<u8>>>, GatewayError>;
    async fn write_file(&self, sandbox: &SandboxName, file: &SandboxFile) -> Result<(), GatewayError>;

    async fn upload_tree(&self, sandbox: &SandboxName, source: &ProjectRoot, dest: &SandboxPath, cancel: &CancellationToken) -> Result<(), GatewayError>;
    async fn upload_file(&self, sandbox: &SandboxName, source: &HostPath, dest: &SandboxPath, cancel: &CancellationToken) -> Result<(), GatewayError>;
    async fn download_file(&self, sandbox: &SandboxName, remote: &SandboxPath, local: &HostPath, cancel: &CancellationToken) -> Result<Sha256, GatewayError>;

    async fn open_terminal(&self, request: &TerminalRequest) -> Result<TerminalSession, GatewayError>;
}
```

The gateway name and the OpenShell workspace are fixed at construction, so every call passes both explicitly to the SDK or CLI (D03) without the session threading them. The sandbox name is per call.

#### Request and result types

```rust
pub struct GatewayHealth { pub version: String, pub serving: bool }
pub enum ProfileSync { Imported, Unchanged, Updated }
pub enum ProviderState { Absent, Present }
pub struct ProviderSpec { pub name: ProviderName, pub profile: ProfileId, pub keys: CredentialKeys, pub material: ProviderMaterial }
pub enum ProviderMaterial {
    ChatgptLogin {
        access_token: Secret,
        refresh_token: Secret,
        client_id: String,
        chatgpt_account_id: ChatgptAccountId,
    },
}
pub struct MainProcess { pub program: SandboxPath, pub args: Vec<String> }
pub struct SandboxCreate { }
pub struct SandboxView { pub name: SandboxName, pub phase: Phase, pub labels: Labels, pub provisioning: Provisioning }
pub enum Provisioning { Normal, Failed { message: TerminalText } }
pub enum SandboxQuery { Name(SandboxName), Labels(LabelSelector) }
pub struct SandboxDetail {
    pub view: SandboxView,
    pub providers: Vec<ProviderName>,
    pub policy: PolicyReport,
}
pub struct PolicyReport { pub version: u32, pub hash: String, pub loaded: bool, pub document: PolicyDocument }
pub enum PowerAction { Start, Stop }
pub struct ProgramRun {
    pub sandbox: SandboxName,
    pub program: SandboxPath,
    pub args: Vec<String>,
    pub workdir: Option<SandboxPath>,
    pub env: BTreeMap<EnvName, String>,
    pub stdin: ProgramStdin,
    pub touch_on_success: Option<SandboxPath>,
    pub timeout: Duration,
}
pub enum ProgramStdin { Closed, Data(SecretBytes) }
pub struct ProgramOutcome { pub exit_code: i32, pub stdout: Vec<u8>, pub stderr: Vec<u8> }
pub struct SandboxFile { pub path: SandboxPath, pub mode: FileMode, pub content: SecretBytes }

impl SandboxCreate {
    pub fn new(
        name: SandboxName,
        image: ImageTag,
        labels: SandboxLabels,
        providers: Vec<ProviderName>,
        policy: PolicyDocument,
        main: MainProcess,
    ) -> SandboxCreate;
    pub fn name(&self) -> &SandboxName;
    pub fn image(&self) -> &ImageTag;
    pub fn labels(&self) -> &SandboxLabels;
    pub fn providers(&self) -> &[ProviderName];
    pub fn policy(&self) -> &PolicyDocument;
    pub fn main(&self) -> &MainProcess;
    pub fn environment(&self) -> BTreeMap<EnvName, String>;
}
```

`ProviderMaterial` has one variant. Credential mode A adds a variant when a profile exists (decision 5). `SandboxCreate` has private fields and one constructor. Its spec environment is derived, not passed: it is `JACKIN_LAUNCH_ID` from the launch label and nothing else, so the environment and the label cannot disagree. The role environment is not in the spec; it is the env file (D11). `Provisioning::Failed` is set when a status condition of the sandbox has the reason `ProvisioningFailed`, which the vm driver writes (`openshell-driver-vm`, `driver.rs:1360`); the shape that P7.4 F8 observed is confirmed live in P4.1 (open question 8).

Contract of each method. The fake and the real adapter obey the same text, and `contract::run` checks it.

- `ensure_profile`: imports the profile when its ID is absent and returns `Imported`. A present profile with identical content gives `Unchanged`. A present profile with different content is replaced through `UpdateProviderProfiles` (`openshell.proto:366`), with the resource version that `GetProviderProfile` returned in the same call, and gives `Updated`. That RPC rejects a zero version, so an update is optimistic: a change between the read and the update gives `FailedPrecondition` and writes nothing, and the next launch converges. Idempotent.
- `put_provider`: creates the provider, or updates its credentials when it exists, and configures refresh for `ChatgptLogin` (access token key, refresh token as secret material, client ID, and token URL, all from `ProviderSpec.keys`, so the adapter holds no agent knowledge). The refresh token is never readable back. Idempotent for equal input.
- `create_sandbox`: creates with the image tag, labels, the `JACKIN_LAUNCH_ID` environment, providers, policy, and the main process with a kept PTY and restart policy never. It returns when the call returns, not when `Ready` (P0.2). A second create with the same name returns `AlreadyExists`. Names and label values are validated by types, so the gateway never sees an invalid one, and the fake rejects an invalid one the same way the gateway does.
- `find_sandboxes`: `Labels` lists every matching sandbox across all pages, and `managed()` returns only sandboxes that carry the jackin label set. `Name` returns zero or one result. A missing sandbox is an empty list, not an error. A view carries `Provisioning::Failed` when the gateway reports a failed provisioning.
- `sandbox_detail`: the view, the attached OpenShell providers, and the effective policy as a `PolicyDocument`.
- `set_power`: `Start` on `Stopped` and `Stop` on `Ready` move the phase. OpenShell 0.1.6 behaves differently when the main process has ended (D09, P7.1 F3): `Stop` on `Error` fails with `MainProcessFailed`, and `Start` on `Error` returns a view in `Starting` that never reaches `Ready`. The adapter reports this as it is and the fake reproduces it. `Session::power` refuses both phases before it calls the gateway.
- `run_program`: runs an absolute program path without a PTY, as the user `sandbox`, with stdin as given (`Closed` closes the stream at once), and captures stdout and stderr to the end. When the program exits 0 and `touch_on_success` is set, the same request creates that file, so the setup hook and its marker are one request (D11). It enforces `timeout`. A cancel drops the stream, which stops the remote process (P0.6). A sandbox that does not accept exec gives `NotReady { phase }`. The real adapter composes the touch inside the one exec; the fake does it in its state.
- `read_files`: returns one entry per requested path, `None` when the file does not exist.
- `write_file`: writes atomically (temporary file in the same directory, mode applied, then rename), creates parent directories, and sends the bytes on stdin only. Idempotent.
- `upload_tree`: runs `openshell sandbox upload <sandbox> . <dest>` (D08 call 1) with the project root as the working directory, stdin from `/dev/null`, explicit gateway and workspace, `GIT_` removed, and a hard timeout; it never uploads `.git` and never deletes. `upload_file`: runs D08 call 2 (`--no-git-ignore`) to an exact destination name and compares the sha256 digests. Both fail with `Transfer` on a non-zero exit. `download_file` returns the sha256 it compared, so a caller never rereads the file. The project module in `jackin-session` (`Session::upload_project`) owns the git side of D08: host checks, the bundle, the sandbox repository, and the tree-id gate (U10b). Both upload calls pass `GIT_CONFIG_COUNT=2` with the pairs `core.fsmonitor=false` and `core.hooksPath=/dev/null`, set after the `GIT_` and `OPENSHELL_` removal, so a project's own git configuration cannot run a program in the CLI's git. The download passes none.
- `download_file`: reads the remote size first and refuses a file above 4 GiB (`DownloadTooLarge`). It runs `openshell sandbox download` into a temporary file beside the destination, named `.<name>.jackin-<pid>-<n>`, and checks the length of what landed against the same limit. Then it compares the SHA-256 of the downloaded bytes with the digest that `sha256sum` reports in the sandbox. A mismatch fails with `Transfer { op: Digest }`. The adapter renames the temporary file over the destination only after the digests match. On every error, including a timeout and a cancel, it removes the temporary file and leaves an existing destination as it was. A hard kill of the process can leave the temporary file behind.
- `delete_provider`, `delete_profile`, and `delete_sandbox`: idempotent. A missing target is `Ok(())` and never `NotFound`. The adapter sends `allow_missing` and also maps a `NotFound` reply to `Ok`. The fake removes the entry when it is present. A provider that a sandbox lists, and a profile that a provider uses, fail with `FailedPrecondition`. Callers do not match `NotFound` on a delete. The `Core` group deletes a provider and a sandbox twice and requires `Ok` both times.
- `open_terminal`: see the next section.

All methods map transport failures to `GatewayError`. None returns a wire status.

### Terminal sessions

One handle type serves every pane, `jackin attach`, and `jackin shell`. The pair constructor is public so that `jackin-testkit` builds fake sessions with the same type.

```rust
pub struct TermSize { pub cols: NonZeroU16, pub rows: NonZeroU16 }
pub enum InputMode { Owner, Viewer }
pub enum RepaintPolicy { Never, NudgeAfter(Duration) }
pub enum TerminalKind {
    MainProcess { input: InputMode, repaint: RepaintPolicy },
    Shell { workdir: SandboxPath, env: BTreeMap<EnvName, String> },
}
pub struct TerminalRequest { pub sandbox: SandboxName, pub kind: TerminalKind, pub size: TermSize }
pub enum TerminalCommand { Input(Vec<u8>), Resize(TermSize), Detach }
pub enum TerminalEvent { Attached, Lease(InputLease), Output(Vec<u8>), Notice(String), Exited(Option<i32>), Ended(EndReason) }
pub enum InputLease { Owner, ReadOnly }
pub enum EndReason { Detached, StreamClosed, Refused(String), Failed(String) }
pub struct TerminalSession { }
pub struct TerminalDriver {
    pub commands: tokio::sync::mpsc::Receiver<TerminalCommand>,
    pub events: tokio::sync::mpsc::Sender<TerminalEvent>,
}

impl TerminalSession {
    pub fn pair(capacity: usize, kind: TerminalKind) -> (TerminalSession, TerminalDriver);
    pub fn kind(&self) -> &TerminalKind;
    pub async fn input(&self, bytes: Vec<u8>) -> Result<(), SessionClosed>;
    pub async fn resize(&self, size: TermSize) -> Result<(), SessionClosed>;
    pub async fn detach(&self) -> Result<(), SessionClosed>;
    pub async fn next_event(&mut self) -> Option<TerminalEvent>;
}
```

Ownership: the task spawned by `open_terminal` owns the remote session (SSH channel or exec stream), the current size, and the repaint timer. The `TerminalSession` owner owns only the two channel ends. The channels are bounded, so a slow consumer applies backpressure to the transport task (no frame drop, no unbounded queue). Dropping the `TerminalSession` sends `Detach` for a main-process terminal (the process keeps running and the input lease is released) and closes the stream for a shell (the remote process dies, P0.6 fact 7). `Exited(code)` for a main-process terminal carries the SSH exit status; the authoritative exit code is the `Phase` from `find_sandboxes` (P0.6 fact 6). `RepaintPolicy::NudgeAfter(d)` makes the task send `rows-1` then `rows` once, `d` after `Attached`, using its own current size, so a resize from the pane in between is never overwritten.

Input lease. OpenShell 0.1.6 implements it as follows (`openshell-supervisor-process` `ssh.rs:803-826`, `953-958`, `975-988`) and `open_terminal` states it for a `MainProcess` terminal: `Attached` is followed by exactly one `Lease`. The first `Owner` attachment takes the lease and gets `Lease(Owner)`. A second attachment is read-only: it gets `Lease(ReadOnly)` with the server notice, its input is not forwarded, and `open_terminal` does not fail. When the owner has left, the next input from a read-only `Owner` attachment takes the lease: its bytes are forwarded and `Lease(Owner)` is delivered. `InputMode::Viewer` never takes it. A Ctrl-C typed in a read-only attachment closes only that attachment (`Ended(StreamClosed)`); the owner and the process are unaffected. Ctrl-D detaches either kind. `FakeGateway` models these rules and the Terminal contract group checks them.

Main-process transport (`grpc/main_pty.rs`): `CreateSshSession` (one token per terminal), `ForwardTcp` with an SSH relay target, a `russh` client with the host key pinned to the fingerprint of the `CreateSshSession` response, `authenticate_none("sandbox")`, a PTY request first, then the `openshell-main` subsystem, with `OPENSHELL_MAIN_READ_ONLY=1` for `InputMode::Viewer`. Keepalive 15 s, three misses (P0.6). Shell transport: `ExecSandboxInteractive` with `tty` set, the start message with `workdir` and `environment`, stdin chunks of at most 4096 bytes, resize messages. Neither transport needs the system `ssh`. The token is revoked best effort on end. A fourth stream on one token, or a 21st attachment on one sandbox, surfaces `GatewayError::ResourceExhausted`.

### Discovery and construction

```rust
pub struct RegisteredGateway { pub name: GatewayName, pub endpoint: String, pub auth: GatewayAuth }
pub enum GatewayAuth { Mtls, Other(String) }
pub struct ResolvedGateway { }

impl ResolvedGateway {
    pub fn name(&self) -> &GatewayName;
    pub fn endpoint(&self) -> &str;
}

pub async fn registered_gateways(runner: &dyn ProcessRunner, cancel: &CancellationToken) -> Result<Vec<RegisteredGateway>, DiscoveryError>;

pub async fn resolve_gateway(
    runner: &dyn ProcessRunner,
    fs: &dyn FileSystem,
    openshell_config_home: &HostPath,
    name: &GatewayName,
    cancel: &CancellationToken,
) -> Result<ResolvedGateway, DiscoveryError>;

pub struct GrpcGatewayConfig {
    pub gateway: ResolvedGateway,
    pub workspace: OpenShellWorkspace,
    pub runner: Arc<dyn ProcessRunner>,
    pub transfer_timeout: Duration,
    pub call_timeout: Duration,
}
pub struct GrpcGateway { }

impl GrpcGateway {
    pub async fn connect(config: GrpcGatewayConfig) -> Result<GrpcGateway, GatewayError>;
}
```

`registered_gateways` runs `openshell gateway list -o json` through the runner (stdin from `/dev/null`, hard timeout) and needs no connection, so `jackin doctor` can tell "not registered" from "registered but down". `resolve_gateway` selects the entry by name from the same list and reads `ca.crt`, `tls.crt`, and `tls.key` of that entry through `FileSystem`. It does not write the registry and does not change the active gateway (D03). `GrpcGateway::connect` builds the SDK client from the parsed material once; every pane clones the shared HTTP/2 connection inside the adapter.

### Policy model (D10, D12, D24)

The policy model lives here because `openshell-policy` is confined to this crate (decision 11). `policy/yaml.rs` is the only file that names it.

```rust
pub struct PolicyDocument { }
pub struct HostPattern(String);
pub struct RuleKey(String);
pub struct ClosedPolicy(PolicyDocument);
pub enum ClosedPolicyError { Policy(PolicyError), Violations(Vec<ClosureViolation>) }
pub struct NetworkRule { pub key: RuleKey, pub endpoints: Vec<RuleEndpoint>, pub binaries: Vec<SandboxPath> }
pub struct RuleEndpoint { pub host: HostPattern, pub port: u16, pub protocol: EndpointProtocol, pub tls: TlsMode }
pub enum EndpointProtocol { Tcp, Rest }
pub enum TlsMode { Default, Skip }
pub struct ProviderProfile {
    pub id: ProfileId,
    pub yaml: &'static str,
    pub endpoints: Vec<ProfileEndpoint>,
    pub binaries: Vec<SandboxPath>,
}
pub struct ProfileEndpoint { pub host: HostPattern, pub port: u16 }
pub enum ClosureViolation {
    Overlap { rule: RuleKey, host: HostPattern, provider_host: HostPattern },
    UnparsableHost { rule: RuleKey, host: String },
    InvalidPort { rule: RuleKey, port: u32 },
}

impl PolicyDocument {
    pub fn parse(text: &str) -> Result<PolicyDocument, PolicyError>;
    pub fn parse_authored(text: &str) -> Result<PolicyDocument, PolicyError>;
    pub fn network_rules(&self) -> Vec<NetworkRule>;
    pub fn closure_violations(&self, profile: &ProviderProfile, generated: &[RuleKey]) -> Vec<ClosureViolation>;
    pub fn with_rule(&self, rule: NetworkRule) -> Result<PolicyDocument, PolicyError>;
    pub fn render(&self) -> Result<String, PolicyError>;
}

impl ClosedPolicy {
    pub fn parse(text: &str) -> Result<ClosedPolicy, ClosedPolicyError>;
    pub fn document(&self) -> &PolicyDocument;
}

impl ProviderProfile {
    pub fn parse(asset: &'static ProfileAsset) -> Result<ProviderProfile, PolicyError>;
}

impl RuleKey {
    pub fn provider_rule(provider: &ProviderName) -> RuleKey;
    pub fn is_reserved(&self) -> bool;
}

impl HostPattern {
    pub fn parse(text: &str) -> Result<HostPattern, PolicyError>;
    pub fn overlaps(&self, other: &HostPattern) -> bool;
}

impl NetworkRule {
    pub fn docker_engine(port: NonZeroU16) -> NetworkRule;
}
```

`PolicyDocument` types only the `network_policies` view; the rest of the document is the author's. The adapter converts the document to the gateway policy message through `openshell-policy` with no change of its own (D12), so `render` is for display and `jackin inspect`. `parse_authored` rejects a rule key that starts with `_provider_`, which OpenShell reserves for the rule it generates. `parse` accepts those keys, for the live policy that `sandbox_detail` reads back. `closure_violations` implements the D10 provider host closure: any rule whose endpoint host pattern overlaps a profile host, on any port and with any binary, is a violation. `generated` lists the rule keys that are exempt. A caller checking a live policy passes `RuleKey::provider_rule(p)` for each OpenShell provider attached to the sandbox, so the generated `_provider_<name>` rule is not reported, while a foreign rule named `_provider_other` that matches a Codex host still is. A caller checking an authored file passes an empty slice. `overlaps` treats `*.openai.com`, `**.openai.com`, and exact hosts; a bare `*` or `**` does not parse. `ClosedPolicy` has a private field and one constructor: `parse` runs `parse_authored`, then requires `closure_violations` against the embedded Codex profile, with an empty `generated` list, to be empty. `SandboxCreate::new` takes a `ClosedPolicy`, so a raw `PolicyDocument` cannot reach `CreateSandbox`. `ClosedPolicyError` is `Policy` for a parse failure and `Violations` for an overlap. `NetworkRule::docker_engine` builds the D24 rule: host `host.openshell.internal`, the engine port, `tcp`, `tls: skip`, binary `/usr/local/bin/docker`.

`HostPattern::parse` accepts a host only when every label is made of ASCII letters, digits, `_`, `-`, and `*`, or is exactly `**`. It then refuses the wildcard shapes that the gateway refuses: a whole `*` or `**`, `*.x` or `**.x` with two labels, a first label that holds `**` without being exactly `**`, and a later label that holds `**` or holds `*` without being exactly `*`. The `openshell-core` pattern parser runs last, and the text is lowercased. The check sits in `parse`, so `parse_authored`, `ClosedPolicy::parse`, and the live scan that `jackin inspect` uses all apply it, and a character that the sandbox's glob matcher reads as syntax (`[`, `]`, `^`, `!`, `?`, `\`, `{`, `}`) cannot reach `overlaps`. A live host that fails the check is an `UnparsableHost` violation.

`parse_authored` also refuses a policy whose `landlock.compatibility` is not `hard_requirement` (`PolicyError::LandlockMode { found }`; a missing `landlock` section and `landlock: {}` are refused). It refuses a policy whose `filesystem_policy` lists no path in `read_only` or `read_write` (`PolicyError::NoFilesystemPaths`; `include_workdir` does not count). The Landlock check runs first, so a policy with both defects reports `LandlockMode`. `parse` and `with_rule` apply neither check.

`ClosureViolation` implements `Display`: `rule r1: <host> overlaps the provider host <provider>`, `rule r1: the host <text> cannot be read`, and `rule r1: the port N is invalid`. `RuleKey` implements `Display` and escapes control characters.

### Contract suite

```rust
#[cfg(feature = "contract")]
pub mod contract {
    pub async fn run(gateway: &dyn Gateway, fixture: &ContractFixture, group: ContractGroup) -> ContractReport;
}

pub enum ContractGroup { Core, Transfer, Terminal }
pub struct ContractFixture { pub image: ImageTag, pub project: ProjectRoot, pub name_prefix: SandboxSlug }
pub struct ContractReport { pub checks: Vec<ContractCheck> }
pub struct ContractCheck { pub name: &'static str, pub outcome: Result<(), String> }
```

The suite has three groups, because the three need different infrastructure.

- `Core` (gRPC only): create then find by label, `AlreadyExists` on a repeated name, `Provisioning::Failed` reporting, program exit code and stdin, `touch_on_success` on exit 0 and not on exit 1, `write_file` mode and atomicity, `read_files` of a missing file, `Imported`, `Unchanged`, and `Updated` for a profile, provider put and delete, power transitions, `Stop` and `Start` on an ended main process, and delete.
- `Transfer`: upload then download with the digest check. A failed or cancelled download leaves an existing local file as it was. A tree upload and a file upload to an absent sandbox or to a `Stopped` one are `Transfer` errors with exit code 1; the absent sandbox says `found` and not `ready`, and the stopped one says `ready` and not `found`. A tree upload whose listed path lies below a symlink that the same upload created is a `Transfer` error with exit code 1 and `tar` in the tail, and the file does not land in the sandbox. The group needs the `openshell` CLI and a real sandbox filesystem.
- `Terminal`: a main-process terminal with detach and re-attach replay, the input lease rules, `Viewer` input ignored, and a shell terminal exit code. It needs the SSH relay.

`jackin-testkit` runs all three groups against `FakeGateway` in nextest (`crates/jackin-testkit/tests/contract.rs`). `crates/jackin-openshell/tests/contract.rs` runs only `Core` against `GrpcGateway` and an in-process tonic server, because that server implements the gRPC subset only. `xtask e2e` runs all three groups against the live gateway before J00 and fails the run on a failed check. Name length is not a suite check: `SandboxName` makes a 20-character name unrepresentable, so `SandboxName::parse` unit tests in `jackin-core` cover the rule, and `crates/jackin-openshell/tests/raw_names.rs` sends a raw 20-character name through the tonic client to the in-process server and requires `GatewayError::InvalidArgument { field: "name" }`. The real v0.1.6 rejection is recorded in the P4.1 evidence.

### Error enums

```rust
pub enum GatewayError {
    Unreachable { endpoint: String, message: String },
    SandboxUnreachable { sandbox: SandboxName, message: String },
    Tls { message: String },
    Unauthenticated,
    PermissionDenied { what: String },
    NotFound { what: Missing },
    AlreadyExists { what: Missing },
    NotReady { phase: Phase },
    MainProcessFailed { sandbox: SandboxName },
    FailedPrecondition { message: String },
    ResourceExhausted { what: String },
    InvalidArgument { field: String, message: String },
    Timeout { op: GatewayOp, after: Duration, group: Option<GroupFault> },
    Cancelled { op: GatewayOp, group: Option<GroupFault> },
    Process(ProcessError),
    Transfer { op: TransferOp, status: ProcessStatus, tail: TerminalText },
    DownloadTooLarge { size: u64, limit: u64 },
    OutputTooLarge { op: GatewayOp, stream: OutputStream, limit: usize },
    HostKeyMismatch,
    TerminalRefused { reason: String },
    Protocol { message: String },
}
pub enum Missing { Sandbox(SandboxName), Provider(ProviderName), Profile(ProfileId), Workspace(OpenShellWorkspace) }
pub enum GatewayOp {
    Health, Profile, Provider, Create, Find, Detail, Power, Delete,
    Program, ReadFiles, WriteFile, UploadTree, UploadFile, Download, Terminal,
}
pub enum TransferOp { UploadFiltered, UploadFile, Download, Digest, Size }
pub struct SessionClosed;
pub enum DiscoveryError {
    ListFailed { status: ProcessStatus, tail: TerminalText },
    ListUnparsable { message: String },
    UnknownGateway(GatewayName),
    InsecureEndpoint { name: GatewayName, scheme: Option<String> },
    UnsupportedAuth { name: GatewayName, auth: String },
    MissingCertificate { path: HostPath },
    Process(ProcessError),
    Fs(FsError),
}
pub enum PolicyError {
    Syntax { message: String },
    NotAMapping { key: String },
    ReservedRuleName(String),
    InvalidHost(String),
    LandlockMode { found: Option<String> },
    NoFilesystemPaths,
    InvalidPort { rule: String, port: i64 },
    DuplicateRule(RuleKey),
    Profile { id: String, message: String },
}

impl GatewayError {
    pub fn is_retryable(&self) -> bool;
}
```

Status mapping (one function and one table in `grpc/sandbox.rs`): `NotFound` to `NotFound`; `AlreadyExists` to `AlreadyExists`; `FailedPrecondition` on program or terminal use of a sandbox that does not accept exec to `NotReady`; `FailedPrecondition` on stop of an `Error` sandbox to `MainProcessFailed`; other `FailedPrecondition` to `FailedPrecondition`; `Unauthenticated`; `PermissionDenied`; `ResourceExhausted` (stream or token limit); `InvalidArgument`; `DeadlineExceeded` to `Timeout`; `Unavailable` and connect errors to `Unreachable`; anything else to `Protocol`. `is_retryable` is true for `Unreachable`, `Timeout`, and `ResourceExhausted`; the session decides whether to retry (bounded, with the clock). `SandboxUnreachable` is retryable as well.

Messages print words. `Missing`, `GatewayOp`, `TransferOp`, and `ClosureViolation` implement `Display`, so `NotFound` reads `not found: provider jackin-work-codex` and a timeout reads `health timed out after 30s`. Every `String` that the gateway or a registry entry supplies (`endpoint`, `message`, `what`, `field`, `reason`, and the scheme and auth of `DiscoveryError`) prints through `plain`. The variants hold public `String` fields, so the escape sits in the `Display` of the variant, which every constructor passes. The stored text is unchanged, so the secret redaction in `grpc/provider.rs` still sees the original.

### Test plan outline (mark for `test-reviewer`)

Location: `crates/jackin-openshell/tests/*.rs`. Boundaries faked: the gateway network, as an in-process tonic server that implements the subset of the OpenShell service these tests need (it lives in this crate's test target, because only this crate may import tonic); the `openshell` program, through `ScriptedProcessRunner`; the filesystem (`MemFs`); the clock (`FakeClock`, and `tokio::time::pause` for transport timers). The public interfaces under test are `Gateway`, `registered_gateways`, `resolve_gateway`, `TerminalSession`, and `PolicyDocument`.

| Behavior | Positive | Negative |
|---|---|---|
| Status mapping (P4.1) | each gRPC code in the table yields its `GatewayError` variant through the public methods, with the right `Missing` | an unmapped code yields `Protocol`; `is_retryable` is false for `NotFound` and `AlreadyExists` |
| Create, find, labels (J03) | create with labels then `find_sandboxes(launch id)` returns exactly that sandbox with labels that parse to `SandboxLabels`; pages are followed; a 19-character name is accepted; a server that reports `ProvisioningFailed` yields `Provisioning::Failed` in the view | a second create with the same name gives `AlreadyExists`; a non-jackin sandbox on the server is absent from `managed()` results |
| Ended main process (fix 4) | `Stop` on an `Error` sandbox maps to `MainProcessFailed`; `Start` returns a `Starting` view; `Phase::Error` and `Phase::Completed` carry the exit code the server reports | none |
| Provider flows (D10) | `ensure_profile` imports once, then `Unchanged`; a changed profile gives `Updated` and the server holds the new content; `put_provider` with `ChatgptLogin` configures refresh; the refresh token never appears in any returned value or error `Display` | an update with a stale resource version gives `FailedPrecondition` and the server keeps its content; `put_provider` with an unknown profile gives `NotFound` |
| Program runs | exit code, stdout, stderr, and stdin bytes round trip; `touch_on_success` creates the file on exit 0 only; `Closed` stdin does not hang | timeout gives `Timeout`; a cancel returns `Cancelled` and the server saw the stream drop; a sandbox in `Completed` gives `NotReady` |
| Files | `write_file` result has the requested mode and exact bytes; `read_files` returns `None` for a missing file | a failing write leaves the old file intact (server drops the stream mid-write) |
| Upload and download argv | the tree upload runs with the project root as `cwd`; the file upload uses `--no-git-ignore` and an exact destination; both name the gateway, workspace, and sandbox, have stdin null and a timeout; download reads the remote size first and compares digests | a non-zero exit gives `Transfer { UploadFiltered }` or `Transfer { UploadFile }` with a sanitized tail; a missing remote file gives `Transfer { Size }`; a digest mismatch gives `Transfer { Digest }` |
| Terminal main process (J11) | attach delivers replay then live output; input reaches the process for `Owner`; `Resize` reaches the server; detach leaves the process running; re-attach replays; a second attach gets `Lease(ReadOnly)`, and after the owner detaches its next input is forwarded with `Lease(Owner)` | `Viewer` input is not forwarded; a Ctrl-C in a read-only attachment ends only that attachment; a wrong host key fingerprint gives `HostKeyMismatch` and no data; an attach after exit gives `Ended(Refused)` |
| Repaint nudge | with `NudgeAfter(600ms)` the server sees one resize to `rows-1` and one to `rows`, in that order, after the delay in paused time, with the size the pane last set | `Never` sends none; a pane resize before the timer changes what the nudge restores |
| Terminal shell (J12) | the start message carries `workdir` and `env`; the exit code arrives as `Exited` | dropping the session closes the stream and the server observes the process kill |
| Limits | a fourth stream on one token surfaces `ResourceExhausted` | none |
| Discovery (fix 6) | `registered_gateways` parses a `gateway list -o json` sample without any connection; `resolve_gateway` reads three certificate files | unknown name; missing `tls.key`; invalid JSON each give the right `DiscoveryError`; the registry is never written (no `write_new` reaches the fs fake) |
| Policy and closure (D10) | an authored role policy parses and `with_rule(docker_engine(p))` adds exactly one rule and keeps every other node; a live policy that holds `_provider_jackin-work-codex` passes when that key is in `generated` | `parse_authored` rejects `_provider_x`; a rule with host `*.openai.com`, a rule with `api.openai.com` on port 8080, and a rule with host `**.openai.com` each give a `ClosureViolation`; `**`, `*`, `*.com`, and `[c]hatgpt.com` give `InvalidHost`; a policy with `landlock.compatibility: best_effort`, with no `landlock` section, or with no filesystem path is refused by `parse_authored` and accepted by `parse`; a live rule named `_provider_other` that matches a Codex host is still reported |
| Contract suite | `contract::run(.., Core)` passes against `GrpcGateway` on the in-process server | a deliberately broken server (drops labels) makes `ContractReport` list the failed check |
| Name limit | `SandboxName::parse` accepts 19 characters (core) | a raw 20-character name sent through the tonic client gives `InvalidArgument { field: "name" }` |

Live (outside nextest, in `xtask e2e`): the three contract groups against `jackin-local`, a real main-process attach on Codex (open question 4), and the open questions 2, 3, and 8 of this file.

---

## 4. jackin-session

Role: every operation on a sandbox that the CLI and the console share: plan and run a launch, attach, shell, restart Codex, list, inspect, export, stop, start, purge, account sync, and doctor. It owns the D09 supervisor protocol on the jackin side and the real host implementations of `ProcessRunner` and `Clock` (PLAN P5.2). It depends on `jackin-core`, the `Gateway` and `TerminalSession` of `jackin-openshell`, and the `ImageEngine` of `jackin-image`. It holds no mutable state: a `Session` is cheap to clone, and the sandbox plus its marker files are the state. Planning, role validation, and `doctor` need no `Session` and no connected gateway.

### Module map

```text
crates/jackin-session/src/
  lib.rs
  service.rs        Session, SessionDeps, HostPorts
  host.rs           SystemClock, TokioProcessRunner, capture_env
  process_group.rs  ProcessGroup, Sweep (Linux-only test module and stat parser beneath it)
  roles.rs          RoleCatalog, LoadedRole, RoleError
  credentials.rs    read_chatgpt_login, ChatgptLogin, CredentialError
  accounts.rs       AccountSync, AccountError
  plan.rs           plan_launch, plan_image, LaunchRequest, Prompting, LaunchPlan, PlannedFiles, SandboxDraft, PlanError, EnvQuestion
  files.rs          pub(crate): render_auth_json, render_codex_config
  launch.rs         LaunchStep, LaunchEvent, LaunchEvents, LineDelivery, LaunchFeed, LaunchOutcome, LaunchFailure, LaunchError
  launch/{decide,probe,hooks}.rs   pub(crate): Observed, Done, Decision, decide; SandboxProbe; run_hook
  attach.rs         AttachRequest, AttachTarget, AttachedTerminal, ShellRequest, errors; pub(crate) split_replies
  export.rs         ExportReport, WorkState, ExportError, BundleRefusal
  export/{bundle,host,sandbox}.rs   pub(super): bundle header and pack walker; host git import and publish; sandbox repository reader
  project.rs        UnsupportedProject, HostGitError, TreeId, ObjectFormat; pub(crate): hold, precheck, stage
  project/{bundle,facts,gate,git,listing,refuse,sandbox,shadow,version}.rs   pub(super): the D08 upload; git.rs holds HostGit, version.rs the git floor table, shadow.rs the git directory of the host gate
  lock.rs           LockError; pub(crate) hold: lock files under cache_dir
  state.rs          pub(crate): run_state, env_file, reading (the supervisor state and env file readers)
  lifecycle.rs      list, inspect, power, purge, PurgeMode, PurgeOutcome, errors
  doctor.rs         doctor, DoctorInput, GatewayAccess, DoctorReport, CheckId
```

### Session and host ports

```rust
pub struct HostPorts { pub clock: Arc<dyn Clock>, pub fs: Arc<dyn FileSystem>, pub runner: Arc<dyn ProcessRunner> }
pub struct SessionDeps {
    pub gateway: Arc<dyn Gateway>,
    pub engine: Arc<dyn ImageEngine>,
    pub host: HostPorts,
    pub cache_dir: HostPath,
    pub limits: LaunchLimits,
}
pub struct Session { }

impl Session {
    pub fn new(deps: SessionDeps) -> Session;
    pub fn gateway_name(&self) -> &GatewayName;
    pub fn gateway(&self) -> &dyn Gateway;

    pub async fn launch(&self, plan: LaunchPlan, events: LaunchEvents, cancel: CancellationToken) -> Result<LaunchOutcome, LaunchFailure>;

    pub async fn attach(&self, request: AttachRequest) -> Result<AttachedTerminal, AttachError>;
    pub async fn open_shell(&self, request: ShellRequest) -> Result<TerminalSession, AttachError>;
    pub async fn restart_agent(&self, sandbox: &SandboxName, events: LaunchEvents, cancel: CancellationToken) -> Result<AttachTarget, RestartError>;

    pub async fn list(&self) -> Result<SandboxListing, LifecycleError>;
    pub async fn inspect(&self, sandbox: &SandboxName) -> Result<InspectView, LifecycleError>;
    pub async fn power(&self, sandbox: &SandboxName, action: PowerAction) -> Result<SandboxSummary, LifecycleError>;

    pub async fn work_state(&self, config: &Config, sandbox: &SandboxName) -> WorkState;
    pub async fn export(&self, config: &Config, sandbox: &SandboxName, cancel: CancellationToken) -> Result<ExportReport, ExportError>;
    pub async fn purge(&self, config: &Config, sandbox: &SandboxName, mode: PurgeMode) -> Result<PurgeOutcome, PurgeError>;

    pub async fn sync_account(&self, account: &Account, cancel: &CancellationToken) -> Result<AccountSync, AccountError>;
    pub async fn forget_account(&self, account: &Account, cancel: &CancellationToken) -> Result<(), AccountError>;
}

pub struct SystemClock;
pub struct TokioProcessRunner;

pub struct ProcessGroup(/* private */);
pub enum Sweep { Empty, Signalled }
impl ProcessGroup {
    pub fn of_leader(id: u32) -> Option<ProcessGroup>;
    pub fn strike(self);
    pub fn sweep(self) -> Result<Sweep, GroupFault>;
    pub async fn stop(self) -> Result<(), GroupFault>;
}

impl HostPorts {
    pub fn system(fs: Arc<dyn FileSystem>) -> HostPorts;
}
```

`list` returns `SandboxListing { sandboxes, unreadable }`: a managed sandbox whose labels do not parse is listed in `unreadable` with its name and `LabelError`, and the other sandboxes still list. `SystemClock` implements `Clock`, and `HostPorts::system(fs)` builds the real host ports. A `Session` is bound to one gateway. The console holds one per configured gateway that connected. The operations take `&Config` only where they need it (the workspace path for export); the session owns no config copy.

### Roles (no gateway needed)

```rust
pub struct RoleCatalog { }
pub struct LoadedRole { }

impl RoleCatalog {
    pub fn new(fs: Arc<dyn FileSystem>, runner: Arc<dyn ProcessRunner>, cache_dir: HostPath) -> RoleCatalog;
    pub async fn load(&self, config: &Config, role: Option<&RoleName>, workspace: &Workspace) -> Result<LoadedRole, RoleError>;
    pub async fn load_path(&self, root: &HostPath) -> Result<LoadedRole, RoleError>;
}

impl LoadedRole {
    pub fn name(&self) -> &RoleName;
    pub fn root(&self) -> &HostPath;
    pub fn manifest(&self) -> &RoleManifest;
    pub fn policy(&self) -> &PolicyDocument;
    pub fn dockerfile(&self) -> &SourceFile;
    pub fn hooks(&self) -> &HookFiles;
    pub fn revision(&self) -> &RoleRevision;
}
```

`RoleCatalog::new` takes the filesystem and the runner only, so `jackin role check <path>` (G-ROLE) builds one and calls `load_path` with no gateway, no config, and no engine. `load` picks the role from the argument, else the workspace `default_role`, else the only configured role, else `RoleError::NoRole`. A `RoleSource::Git` is cloned or fetched into `cache_dir/roles/<name>` through the runner (`Program::Git`). Both do all role-intrinsic validation once: parse the manifest; require that the Dockerfile, policy, and hook paths are regular files inside the repository (no symlink escape); `validate_dockerfile`; `PolicyDocument::parse_authored`; and the D10 provider host closure check against every profile of every `Agent` (`ProviderProfile::parse` of each `AgentSpec::profiles`, empty `generated` slice), reported as `RoleError::ProviderHostClosure`. The manifest read, shared by `load` and `findings`, checks the root first with `stat_following`. A path that does not exist is `RootMissing`. A path that is not a directory, a symlink to a file included, is `RootNotDirectory`. An existing directory without `jackin.role.toml` is `ManifestMissing`. A symlink to a directory is checked like the directory. Every git call of the catalog goes through `HostGit`, and a role checkout fetches with `--tags --force`. `RoleRevision` is `Clean { commit }` when `git status --porcelain` is empty, else `Dirty { commit, digest }`, where `digest` hashes the sorted names and contents from `git ls-files -co --exclude-standard` (decision 4).

### Credentials and account sync (mode B, decision 5)

```rust
pub struct ChatgptLogin {
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub client_id: String,
    pub account_id: ChatgptAccountId,
}

pub async fn read_chatgpt_login(fs: &dyn FileSystem, home: &HostPath) -> Result<ChatgptLogin, CredentialError>;

pub enum AccountSync { HostOnly, ProviderReady { provider: ProviderName, profile: ProfileSync } }
```

`read_chatgpt_login` parses `<home>/auth.json` once into typed fields; `jackin account add codex --chatgpt-from <home>` calls it to learn the account ID for `CredentialSource::CodexLogin`. The secrets appear only inside `ChatgptLogin` and `ProviderMaterial`. `sync_account` runs on `account add` and on launch when the OpenShell provider is absent on the selected gateway. For a Codex account with a `CodexLogin` source it reads the login, calls `ensure_profile`, and calls `put_provider`. For a Codex account with any other source (credential mode A) it returns `AccountError::Unsupported { account, mode }`, because `profile_for(ApiKey)` is `None` until a profile is proven (P10.1 or later). For an account of any other provider it returns `HostOnly` and touches no gateway (D26). When mode A gets a profile, this file gains the `Env`, `OnePassword`, and `File` readers (D28 1Password flow); the usage crates read their own credentials. `sync_account(account, cancel)` and `forget_account(account, cancel)` hold the host lock file `account-<account>.lock` under `cache_dir`. `lock::hold` creates `cache_dir` with mode 0700, polls every 100 ms on the `Clock`, and returns `LockError::Cancelled` when the token fires. `sync_account` always puts. The launch calls the internal `sync_if_absent` instead: it takes the lock, re-checks `provider_state`, and puts only when the provider is still absent, so two launches of one account cannot overwrite a rotated login, and it never calls the public `sync_account`, which would wait on the lock it holds. `forget_account` ignores an account that is not a Codex login and deletes the OpenShell provider of a Codex login account (`account rm`). The gateway refuses to delete a provider that a sandbox lists, so the account stays in that case.

```rust
pub enum LockError { Cancelled, Fs(FsError), Path(PathError) }
```

### Launch

#### Request and plan

```rust
pub struct LaunchRequest { }

impl LaunchRequest {
    pub fn new(gateway: GatewayName, project: ProjectRoot, role: RoleName, launch: LaunchId) -> LaunchRequest;
    pub fn account(self, account: Option<AccountName>) -> LaunchRequest;
    pub fn rebuild(self, rebuild: Rebuild) -> LaunchRequest;
    pub fn env(self, values: BTreeMap<EnvName, String>) -> LaunchRequest;
    pub fn prompting(self, prompting: Prompting) -> LaunchRequest;
}

impl Prompting {
    pub fn for_terminal(is_tty: bool) -> Prompting;
}

pub struct EnvQuestion { pub name: EnvName, pub prompt: Option<String>, pub default: Option<String> }
pub struct LaunchLimits {
    pub ready: Duration,
    pub supervisor: Duration,
    pub attempt: Duration,
    pub hook: Duration,
    pub upload: Duration,
    pub create_attempts: NonZeroU8,
    pub bundle_bytes: u64,
    pub export_entry_bytes: u64,
    pub export_objects: u32,
    pub export_total_bytes: u64,
}
pub enum Prompting { Never, Interactive }
pub struct PlannedFile { pub path: SandboxPath, pub mode: FileMode, pub text: String }
pub struct PlannedFiles { pub codex_config: PlannedFile, pub env: PlannedFile }
pub struct LaunchPlan { }
pub struct SandboxDraft { }

pub fn plan_launch(config: &Config, role: &LoadedRole, request: LaunchRequest) -> Result<LaunchPlan, PlanError>;

pub fn plan_image(
    config: &Config,
    role: &LoadedRole,
    gateway: &GatewayName,
    workspace: Option<&Workspace>,
    rebuild: Rebuild,
) -> Result<ImageRequest, PlanError>;

impl LaunchPlan {
    pub fn launch_id(&self) -> &LaunchId;
    pub fn gateway(&self) -> &GatewayName;
    pub fn sandbox_name(&self) -> &SandboxName;
    pub fn workspace(&self) -> &WorkspaceName;
    pub fn role(&self) -> &RoleName;
    pub fn account(&self) -> &AccountSelection;
    pub fn docker(&self) -> &DockerCapability;
    pub fn warnings(&self) -> &[LaunchWarning];
    pub fn image_request(&self) -> &ImageRequest;
    pub fn draft(&self) -> &SandboxDraft;
    pub fn files(&self) -> &PlannedFiles;
}

impl SandboxDraft {
    pub fn name(&self) -> &SandboxName;
    pub fn labels(&self) -> &PlannedLabels;
    pub fn provider(&self) -> &ProviderName;
    pub fn complete(&self, image: &BuiltImage) -> SandboxCreate;
}

pub enum PlanError {
    Config(ConfigError),
    Selection(SelectionError),
    Project(PathError),
    Image(ImageError),
    Policy(PolicyError),
    Docker(DockerError),
    EnvNeeded(Vec<EnvQuestion>),
    EnvUndeclared(Vec<EnvName>),
    EnvFile(EnvFileError),
    Unsupported { account: AccountName, mode: CodexAuthMode },
}
```

`bundle_bytes` caps the sandbox bundle that `export` downloads (`DEFAULT_BUNDLE_BYTES`, 4 GiB, the download limit of the adapter). `export_entry_bytes` (`DEFAULT_EXPORT_ENTRY_BYTES`, 512 MiB), `export_objects` (`DEFAULT_EXPORT_OBJECTS`, 5,000,000, the pack header holds a `u32`), and `export_total_bytes` (`DEFAULT_EXPORT_TOTAL_BYTES`, 64 GiB) cap the pack walker. The library holds no other timing or size constants; the binary builds the value in `launch_limits()`.

`plan_launch` and `plan_image` are pure: no I/O, no clock, no randomness. The caller supplies the `LaunchId` (`LaunchId::generate()` in the binary, `--launch-id` or a fixed value in tests). `plan_launch` does, in order: resolve the gateway entry (platform, builder, engine); resolve the workspace from the project path; select the account with `Config::select_account`; derive the Codex auth mode and require `AgentSpec::profile_for(mode)` (else `PlanError::Unsupported`); resolve `DockerCapability::resolve(manifest.docker.enabled, workspace.docker, gateway.docker_engine)`, and when enabled merge `NetworkRule::docker_engine(port)` into the role policy with `with_rule`; resolve `[env]` (a `--env` value wins; with `Prompting::Interactive` every other entry becomes an `EnvQuestion` that carries its prompt and its default, so the TTY prompt shows the default pre-filled; with `Prompting::Never` an entry uses its default, and an entry with neither becomes an `EnvQuestion`; a `--env` name the manifest does not declare is `EnvUndeclared`); render the files (below); build the `ImageRequest` through `plan_image`; build `PlannedLabels`; derive `SandboxName::new(workspace.slug(), NameSuffix::for_launch(launch))`; and build the `SandboxDraft`.

The `SandboxDraft` holds the name, the `PlannedLabels` (no image label), the providers `[ProviderName::for_account]`, the merged policy, and the main process `/usr/local/bin/jackin-start` with `AgentSpec::launch_argv`. It holds no environment: `SandboxCreate::new` derives the spec environment, which is `JACKIN_LAUNCH_ID` only. It does not hold the image tag, which depends on the built construct image ID. `complete(&BuiltImage)` is the only way to a `SandboxCreate`: it sets the image tag from `BuiltImage.tag` and the image label from `PlannedLabels::complete(&BuiltImage.id)`.

`LaunchPlan::files` holds the two non-secret files of the Credentials step as text, so a plan test reads the bytes with no gateway. `codex_config` is at `AgentSpec::user_config` and is produced by `render_codex_config`, a pure function that serializes a typed struct with the `toml` crate: the table `[projects."/sandbox/project"]` with `trust_level = "trusted"` for `AgentSpec::workdir` (without it Codex opens a trust question instead of its prompt, P7.4 F1), and `model = "<name>"` first when the manifest has `[codex] model`. `env` is at `sandbox_layout::env_file(launch)` (mode 0600) and holds `EnvFile::new(role values plus DockerCapability::environment())` (D11, D24).

`plan_image` serves `jackin build` as well, with `workspace: None`, so the Docker layer follows the role manifest only. The caller prompts for `EnvNeeded` only on a TTY and calls `plan_launch` again with answers; without a TTY it reports the names (D11).

#### Steps, events, outcome

```rust
pub enum LaunchStep { Image, Provider, Sandbox, Ready, Credentials, Upload, Supervisor, Hooks, Start }

impl LaunchStep {
    pub const ALL: [LaunchStep; 9];
}

pub enum LaunchWarning { DockerCapability }
pub enum SkipReason { ImageCached, ProviderPresent, SandboxExists, AlreadyUploaded, SetupAlreadyDone, AlreadyRunning }
pub enum StepResult { Done, Skipped(SkipReason) }
pub enum LaunchEvent {
    Warning(LaunchWarning),
    StepStarted(LaunchStep),
    StepFinished { step: LaunchStep, result: StepResult },
    Image(ImageEvent),
    SandboxKnown(SandboxName),
}
pub struct LaunchEvents { }
pub struct LaunchFeed { }

pub enum LineDelivery { Reliable, BestEffort }

pub fn launch_events(lines: LineDelivery) -> (LaunchEvents, LaunchFeed);

impl LaunchFeed {
    pub async fn recv(&mut self) -> Option<LaunchEvent>;
}

pub struct AttachTarget { pub sandbox: SandboxName, pub state: SupervisorState }
pub struct RestoreCandidate { pub launch_id: LaunchId, pub sandbox: Option<SandboxName> }
pub enum LaunchOutcome { Ready(AttachTarget), Cancelled(RestoreCandidate) }
pub struct LaunchFailure { pub launch_id: LaunchId, pub step: LaunchStep, pub sandbox: Option<SandboxName>, pub error: Box<LaunchError> }
pub enum LaunchError {
    Image(ImageError),
    Gateway(GatewayError),
    Credential(CredentialError),
    Fs(FsError),
    Process(ProcessError),
    Path(PathError),
    HostGit(HostGitError),
    Unsupported(UnsupportedProject),
    ProjectMismatch(Box<TreeMismatch>),
    ProjectFailed { step: ProjectStep, exit_code: i32, output: TerminalText },
    ProjectOutput { step: ProjectStep, message: String },
    ProviderUnavailable { account: AccountName, source: Box<AccountError> },
    AmbiguousLaunch { launch: LaunchId, sandboxes: Vec<SandboxName> },
    NameCollision { name: SandboxName, other: Option<LaunchId> },
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
    SandboxSuspended { sandbox: SandboxName },
    ProvisioningFailed { sandbox: SandboxName, attempts: u8, message: TerminalText },
    AttemptsExhausted { sandbox: SandboxName },
    ReadyTimeout { sandbox: SandboxName, phase: Phase, after: Duration },
    SupervisorTimeout { sandbox: SandboxName, after: Duration },
    AttemptTimeout { sandbox: SandboxName, attempt: AttemptId, last: SupervisorReading },
    HookFailed { hook: HookKind, exit_code: i32, output: TerminalText },
    ProbeInvalid { message: String },
    PlaceholderMissing { sandbox: SandboxName, key: String },
}
pub enum UnsupportedProject {
    SparseCheckout,
    PopulatedSubmodules(Vec<RepoPath>),
    NestedRepositories(Vec<RepoPath>),
    PartialClone { output: TerminalText },
    SymlinkedPath(Vec<RepoPath>),
    RepositoryFilter { drivers: Vec<String>, paths: Vec<RepoPath> },
}
pub enum ObjectFormat { Sha1, Sha256 }
```

`LaunchEvents::emit` never drops a step event. With `LineDelivery::Reliable` it waits for room for `Image(Line)` events too, so the CLI never drops a build line and the build runs at the speed of the renderer. With `BestEffort` it drops `Image(Line)` events when the consumer lags, because the cockpit keeps its own tail. `Cancelled` is an outcome, not an error: it carries the sandbox so the CLI and console report a restore candidate (D22, ADR-008). A failure keeps the sandbox and names it. `LaunchFailure` implements `Display` as `launch <id> failed at <step>` and exposes the cause through `source()`, so the cause prints once, in the report of the binary.

#### How the executor works (concurrency and idempotence)

`launch` is one async function with one owner of the run state. It loops: `observe` (one `find_sandboxes` by launch label, plus one `read_files` probe when the sandbox is `Ready`), then the pure `decide(plan, observed, done, limits)`, then one act. Each act is idempotent against the observation that selected it. The loop checks `cancel` before every act, and long acts (`ensure_image`, `run_program`, the project upload) receive the token.

```rust
pub(crate) struct SandboxProbe {
    pub(crate) uploaded: bool,
    pub(crate) setup_done: bool,
    pub(crate) hooks: HookPresence,
    pub(crate) supervisor: SupervisorReading,
}
pub(crate) struct HookPresence { pub(crate) setup: bool, pub(crate) preflight: bool }
pub(crate) struct Observed {
    pub(crate) sandbox: Option<SandboxView>,
    pub(crate) duplicates: Vec<SandboxName>,
    pub(crate) probe: Option<SandboxProbe>,
}
pub(crate) enum Boot { Observed, Powered }
pub(crate) struct Done {
    pub(crate) steps: BTreeSet<LaunchStep>,
    pub(crate) image: Option<BuiltImage>,
    pub(crate) created: u8,
    pub(crate) boot: Boot,
    pub(crate) started: Option<AttemptId>,
}
pub(crate) enum Decision { Run(LaunchStep), Finish(AttachTarget), Fail(LaunchError) }

pub(crate) fn decide(plan: &LaunchPlan, observed: &Observed, done: &Done, limits: &LaunchLimits) -> Decision;
```

`Done` is the per-run memory. `steps` holds the steps this run finished (`Provider`, `Ready`, `Credentials`, `Hooks`), `created` counts the creates this run issued, and `boot` is `Powered` after this run called `set_power(Start)`. Its flags are sets and enums, not booleans (`struct_excessive_bools`).

`decide` is a table over `(observed phase, probe, done)`. It is the Launch sequence of DESIGN.md made executable. Hooks run only when the hook file exists in the image (`HookPresence`, read from `hook_path`). They run with `ProgramRun.env` set to the parsed env file of the launch, read with `read_files(env_file)`, never from the gateway (D11). The Credentials step writes that file first, in every run, so a retry with new values replaces it.

| Observed and done | Next | Act (idempotent how) | Skip event |
|---|---|---|---|
| image not done | `Image` | `ensure_image`; the tag is the key, so a hit builds nothing | `ImageCached` when `BuiltImage.source` is `Cache` |
| provider not done | `Provider` | `sync_if_absent`: take the account lock, re-check `provider_state`, and when the provider is still absent run the `sync_account` steps (read login, `ensure_profile`, `put_provider`). A differing profile is updated by `ensure_profile`. Equal material changes nothing | `ProviderPresent` |
| no sandbox with the launch label | `Sandbox` | `create_sandbox(draft.complete(image))`, `created += 1`. On `AlreadyExists`, `find_sandboxes(Name)`: the same launch label means join, another launch label means `NameCollision` | `SandboxExists` |
| more than one sandbox with the launch label | fail | none | `AmbiguousLaunch` |
| phase `Provisioning`, `Starting`, `Stopping`, `Suspending`, `Unknown` | `Ready` | poll `find_sandboxes(Name)` with the clock (500 ms, `limits.ready`) | none |
| phase `Stopped` | `Ready` | `set_power(Start)`, `boot = Powered`, then poll | none |
| phase `Suspended` | fail | none; `StartSandbox` discards the retained memory and `ResumeSandbox` is the operator's call, so jackin does neither | `SandboxSuspended` |
| phase `Error { exit_code: None }`, `Provisioning::Failed`, `created >= 1`, `Ready` not in `steps`, `created < limits.create_attempts` | `Sandbox` | `delete_sandbox`, poll `find_sandboxes(Name)` until empty, then create again (P7.4 F8) | none |
| the same, with `created == limits.create_attempts` | fail | none; the last sandbox remains | `ProvisioningFailed` |
| phase `Deleting`, `created >= 1`, `Ready` not in `steps` | `Sandbox` | poll until the sandbox is gone, then create | none |
| phase `Completed`, other `Error`, other `Deleting` | fail | none; the sandbox is never deleted here | `MainProcessEnded` |
| `Ready`, credentials not done | `Credentials` | `run_program(printenv)` for the access token placeholder; `write_file` `auth.json` (mode 0600) with the placeholder, an empty refresh token, and the real account ID; `write_file` of `files().codex_config` and `files().env`; when Docker is enabled, `write_file` of `ca.pem`, `cert.pem`, and `key.pem` into the Docker client directory (key mode 0600). Overwrite is safe | none |
| `Ready`, `uploaded` marker absent | `Upload` | the D08 project upload (empty the project directory, `upload_tree`, `upload_file` of the bundle, build the sandbox repository, tree-id gate), then `write_file` of the marker last | `AlreadyUploaded` |
| uploaded, supervisor unreadable, or `Running` while `boot` is `Powered` | `Supervisor` | poll `read_files(RUN_STATE)` until a valid state (`limits.supervisor`). After this run started the sandbox, a `running` reading is the file of the earlier boot, so it is not accepted until `waiting` has been read (`boot = Observed`) | none |
| supervisor `Running` | finish | none | `AlreadyRunning` |
| supervisor `Waiting` or `Exited`, hooks not done | `Hooks` | `run_program(setup, touch_on_success = setup_done marker)` unless `setup_done`; `run_program(preflight)`; a non-zero exit fails with `HookFailed` and the sandbox stays `Ready` | `SetupAlreadyDone` |
| hooks done, no attempt started | `Start` | `write_file` of `StartRequest(state.next_attempt())` for the observed state; `None` fails with `AttemptsExhausted`; poll until the state names that attempt as `Running` or `Exited` (`limits.attempt`) | none |
| attempt started and state names it | finish | none | none |

The delete in the recreate row is safe because this run created the sandbox and never saw it accept exec, so it holds no upload. The upload marker itself cannot be read: an `Error` sandbox refuses exec. A retry run that finds an `Error` sandbox it did not create takes the `MainProcessEnded` row and deletes nothing.

A run that knew a sandbox and then sees none, or sees one whose probe answers `NotFound`, treats the sandbox as deleted. `Done::forget_sandbox` keeps `Provider` in `steps` and resets `boot` and `started`, because `Ready`, `Credentials`, and `Hooks` belonged to the old sandbox, and the run creates again.

The Upload act runs in this order. It takes the host lock `upload-<launch>.lock` under `cache_dir`. It reads the uploaded marker, and a marker written by a peer ends the act with `AlreadyUploaded`. Then `stage` runs, then the project directory is emptied, `upload_tree` and the bundle upload run, the sandbox repository is built, and the tree-id gate runs, and the marker is written last. `precheck` runs the same `stage` into a throwaway staging directory before the image build and removes it, so a partial clone is refused before any gateway call. The limit `limits.upload` is a real-time `tokio::time::timeout` that starts after the lock is held. It covers the marker read, the refusals, the bundle, the upload, and the gate, and it ends the act with `Gateway(Timeout { op: UploadTree })`. It is not a `Clock::sleep` race, because `FakeClock` fires a lone sleeper after 64 yields, which would fire during any real git call.

In `jackin-session` every host git call goes through `HostGit`. The one other `Program::Git` call is the `git --version` probe, which reads no repository. `HostGit` puts `-c core.fsmonitor=false -c core.hooksPath=/dev/null` before the subcommand, sets `GIT_NO_LAZY_FETCH=1` and `GIT_OPTIONAL_LOCKS=0`, and removes every `GIT_` variable, so a repository's own configuration cannot run a program. The version floor is data (`GIT_FLOORS`): 2.45.4, 2.46.4, 2.47.3, 2.48.2, 2.49.1, 2.50.1, and 2.51.0. A release on a listed line must reach that line's floor. A release on no listed line passes only above 2.51.0, and `HostGitError::TooOld` names the first floor above it.

`stage` reads the project in this order. `refuse::check` refuses a sparse checkout, populated submodules, nested repositories, and a partial clone. `facts::read` reads the HEAD, the object format, and `info/exclude` below the common git directory (`git rev-parse --git-common-dir`) with `read_regular`, so a symlinked `info/exclude` or a symlinked `info` is absent and never carries the bytes behind the link into the sandbox. The listing is `git ls-files -z -co --exclude-standard --deduplicate` minus `git ls-files -z -d`, one NUL-separated list that both gates use. `refuse::symlinked_parents` refuses with `SymlinkedPath` a listed path that has a symlink as any leading directory. The CLI fails on a listed link (`tar: ...: Cannot open: Not a directory`) and copies the file behind an ignored link, so either would diverge from the host. A symlink with nothing listed below it uploads and arrives as a link. `refuse::filters` refuses with `RepositoryFilter { drivers, paths }` a listed path that selects a filter driver (`clean`, `smudge`, or `process`) that the repository's own configuration defines, in scope `local` or `worktree`. It reads `git config --show-scope -z --list` and, when such a driver exists, `git check-attr -z --stdin filter` on the listing. Neither command runs a filter. A driver named `set`, `unset`, or `unspecified` is refused whenever a listed path shows that word, because `check-attr` prints the same word for the state. A local driver that no listed path selects does not stop the launch. The message names the drivers, up to five paths, and the remedy: define the filter in the global configuration. Last, `stage` creates the staging directory `cache_dir/upload-<launch>-<pid>-<n>` with mode 0700 and the bundle in it.

The tree-id gate adds the listed paths on both sides with `git --literal-pathspecs add -f --pathspec-from-file=<file> --pathspec-file-nul`; the sandbox gets the same bytes in `gate-<launch>/paths`. A file deleted after the listing makes the host `add` fail with `ProjectFailed`. The host side runs in a shadow git directory `<staging>/shadow` (`project/shadow.rs`) with `GIT_DIR` the shadow, `GIT_WORK_TREE` the project, `GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=/dev/null`, a temporary index and object directory, and the project's objects as an alternate. The shadow `config` holds the object format, every `system` and `global` entry as `git config --show-scope -z --list` evaluates it in the project (conditional includes already resolved, `include.path` and `includeif.*.path` dropped), and from `local` and `worktree` scope only `core.autocrlf`, `core.eol`, `core.safecrlf`, `core.symlinks`, `core.filemode`, `core.ignorecase`, `core.precomposeunicode`, `core.attributesfile`, and `core.checkroundtripencoding`. `info/attributes` is the host file, read with `read_regular` (a symlinked file is absent). The gate therefore applies the filters, attributes, and end-of-line rules that a global configuration or a conditional include selects, and none that the repository defines. The listing and the refusals still run in the project repository with the user's configuration.

A retry with the same launch ID re-enters the loop at the observed row. A crash between the upload and its marker uploads again (safe overwrite). A retry after `HookFailed` runs `preflight` again, and runs setup again only when its marker is absent. A retry when Codex exited starts a new attempt after `preflight` (explicit request, ADR-008). The supervisor state is read only to choose between these rows. It never decides export or purge (D09).

Fencing keys: the launch ID label (which sandbox is this launch), the sandbox name (at most one creator wins), the marker file names that embed the launch ID (upload and setup scope), and the attempt ID (which start request the supervisor honors, D09).

### Supervisor protocol (D09), jackin side

`/sandbox/.jackin/run/state` is written only by `jackin-start`: absent (not up yet, or clearing old state), `waiting` (ready for an attempt; hooks may run), `running <attempt>` (Codex is a child; attach), `exited <attempt> <code>` (the PTY is alive and shows one line; restart on request). `/sandbox/.jackin/run/start` holds an `AttemptId`, written only by jackin (`write_file`, atomic) and only while the state is `waiting` or `exited`, because a request written while Codex runs takes effect at its exit (P7.4 F5). Jackin picks the ID with `SupervisorState::next_attempt`; `jackin-start` honors an ID only when it is the last one it handled plus one, so a stale, repeated, or forged ID does nothing (P7.3 F6). `jackin-start` also sources `/sandbox/.jackin/env-$JACKIN_LAUNCH_ID` before each attempt (D11; open question 1). The launch command is a bare name (`codex`), and `jackin-start` resolves it to `/usr/local/bin/<name>` before the loop, so a `PATH` set in the env file cannot choose the binary. After the env file is sourced it calls only `/usr/bin/awk`, `/usr/bin/tr` (at startup), and `/bin/mv`, by absolute path. It writes `/sandbox/.jackin/run/env-kinds` through a temporary file and `mv`, before it publishes `waiting` (the environment `jackin-start` started with, attempt 0) and at each attempt (the environment Codex receives, after the env file is sourced). A failed write does not stop the supervisor, and a stale record is removed at start. The record is `jackin-env-kinds 1`, `attempt N`, then `main NAME KIND` lines and `codex NAME KIND` lines, each part sorted in byte order without duplicates. It names credential-like variables (a name with a word `KEY`, `TOKEN`, `SECRET`, `PASSWORD`, `PASSWD`, `CREDENTIAL`, `CREDENTIALS`, or `AUTH`, except `SSH_AUTH_SOCK`) and a kind: `placeholder` for a value that starts with `openshell:resolve:`, `empty`, or `other`. It never holds a value. A name outside `[A-Za-z_][A-Za-z0-9_]*` is recorded as `INVALID_NAME other`. The sentinel probe reads it for J08. `SupervisorState::parse` and `StartRequest::render` in `jackin-core` are the only Rust code that reads or writes these formats; `jackin-start` implements the same grammar in sh (P7.4).

### Attach, shell, restart

```rust
pub struct AttachRequest { pub sandbox: SandboxName, pub size: TermSize, pub input: InputMode }

impl AttachRequest {
    pub fn owner(target: AttachTarget, size: TermSize) -> AttachRequest;
    pub fn viewer(sandbox: SandboxName, size: TermSize) -> AttachRequest;
}

pub struct AttachedTerminal { }

impl AttachedTerminal {
    pub fn supervisor_at_attach(&self) -> &SupervisorReading;
    pub async fn input(&mut self, bytes: Vec<u8>) -> Result<(), SessionClosed>;
    pub async fn resize(&self, size: TermSize) -> Result<(), SessionClosed>;
    pub async fn detach(&self) -> Result<(), SessionClosed>;
    pub async fn next_event(&mut self) -> Option<TerminalEvent>;
}

pub enum EnvFileProblem { NoLaunchLabel, Missing, Invalid(EnvFileError) }
pub struct ShellRequest { pub sandbox: SandboxName, pub size: TermSize }
pub enum AttachError {
    Gateway(GatewayError),
    NotFound(SandboxName),
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
    EnvFile { sandbox: SandboxName, problem: EnvFileProblem },
}
pub enum RestartError {
    Gateway(GatewayError),
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
    AlreadyRunning { sandbox: SandboxName, attempt: AttemptId },
    AttemptsExhausted { sandbox: SandboxName },
    EnvFile { sandbox: SandboxName, problem: EnvFileProblem },
    StateUnreadable { sandbox: SandboxName },
    Busy { sandbox: SandboxName },
    Fs(FsError),
    Path(PathError),
    HookFailed { hook: HookKind, exit_code: i32, output: TerminalText },
    AttemptTimeout { sandbox: SandboxName, attempt: AttemptId },
}
```

`attach` finds the sandbox, requires `Ready` (a `Completed` or `Error` sandbox has no live main PTY, so `MainProcessEnded` names the phase and exit code), opens `TerminalKind::MainProcess { input, repaint: NudgeAfter(600 ms) }`, and reads the supervisor state once so the CLI can print "Codex exited with code N; use `jackin attach --restart`" (D09). `open_shell` takes the launch ID from the sandbox labels, reads and parses the env file of that launch, and opens `TerminalKind::Shell { workdir: PROJECT, env }` with its values, so a shell sees the role `[env]` values and the `DOCKER_*` variables (D24). A sandbox whose launch never reached the Credentials step has no env file and gives `AttachError::EnvFile`. The console opens at most one `MainProcess` terminal per sandbox (D28) by holding it in its pane model, not by a lock in the session. `restart_agent` runs the hook rows `Hooks` (preflight only, with the env file) and `Start` with `next_attempt()`, and refuses with `AlreadyRunning` when the state is `running` (no silent reconnect or restart). It holds a per-sandbox lock for the whole call: `FileSystem::lock` (a try-lock) on `<cache_dir>/restart-<sandbox>.lock`. A held lock is `RestartError::Busy`, and the guard releases on every return path. It reads the supervisor state before the preflight hook, so a running agent is refused without running a hook, and reads it again after the hook; the attempt comes from the second read. A restart that finds the lock held, or finds the state changed after the hook, runs no further hook and writes nothing. Ctrl-D and Ctrl-P handling (D09, D28) is a terminal front-end concern in `jackin` and `jackin-tui`.

`AttachedTerminal` wraps the `TerminalSession` and is the only handle that `attach` returns, so the CLI pump and every pane send input through `AttachedTerminal::input`. `input` splits each chunk with the pure function `split_replies` into plain bytes and terminal replies: cursor position reports (`ESC [ <row> ; <col> R`), device attribute replies (`ESC [ ? ... c` and `ESC [ > ... c`), and OSC 10 and 11 color replies (`ESC ] 10 ;` and `ESC ] 11 ;`, ended by BEL or `ESC \`). Plain bytes always go through. Only when the chunk holds a reply does `input` read `RUN_STATE` once through the gateway: the replies go through only for `Reported(Running)`, and they are dropped for `Waiting`, `Exited`, `Unreadable`, and `Invalid`. Reading at the moment of the reply sees every change, including a restart after `Exited`. A reply split across two chunks is not recognized (terminals write a reply in one chunk; inferred). Replayed queries make the terminal answer them again on every attach, and those answers would sit in the PTY queue while `jackin-start` waits (D09, P7.4 F3).

### Export and purge (D08, ADR-010)

```rust
pub struct ExportedBranch { pub branch: BranchName, pub tip: CommitId, pub host_ref: String }
pub struct ExportedDetached { pub tip: CommitId, pub host_ref: String }
pub struct ExportedWorktree { pub worktree: BranchName, pub tip: CommitId, pub host_ref: String }
pub struct ExportReport {
    pub sandbox: SandboxName,
    pub bundle: Option<Sha256>,
    pub branches: Vec<ExportedBranch>,
    pub detached: Option<ExportedDetached>,
    pub worktrees: Vec<ExportedWorktree>,
}
pub enum UnknownReason { NotRunning(Phase), WorkspaceUnknown(WorkspaceName), NoRepository, InspectionFailed }
pub struct Unexported {
    pub uncommitted: bool,
    pub branches: Vec<BranchName>,
    pub detached: Option<CommitId>,
    pub worktrees: Vec<BranchName>,
    pub unusable: Vec<TerminalText>,
}
pub enum WorkState { Exported, Unexported(Unexported), Unknown(UnknownReason) }
pub enum PurgeMode { Guarded, Forced }
pub enum PurgeOutcome { Deleted { sandbox: SandboxName, was: WorkState }, AlreadyGone { sandbox: SandboxName } }
pub enum ExportError {
    Gateway(GatewayError),
    Process(ProcessError),
    Fs(FsError),
    Labels { sandbox: SandboxName, source: LabelError },
    NotFound(SandboxName),
    NotRunning { sandbox: SandboxName, phase: Phase },
    WorkspaceUnknown(WorkspaceName),
    Project(PathError),
    Entropy(EntropyError),
    NoRepository { sandbox: SandboxName },
    SandboxOutput { sandbox: SandboxName, what: &'static str },
    SandboxGit { exit_code: i32, output: TerminalText },
    HostGit { status: ProcessStatus, output: TerminalText },
    HostOutput { message: String },
    RefConflict { host_ref: String, output: TerminalText },
    RefCheckedOut { host_ref: String, worktree: HostPath },
    UnusableBranch { branch: TerminalText },
    BundleHeads { extra: Vec<TerminalText>, missing: Vec<String> },
    TooManyHeads { count: usize, limit: usize },
    BundleRefused(BundleRefusal),
    BundleTooLarge { size: u64, limit: u64 },
    TipMismatch { host_ref: String, expected: CommitId, found: Option<CommitId> },
    Cancelled { group: Option<GroupFault> },
}
pub enum BundleRefusal {
    Header,
    Prerequisite,
    Capability,
    ObjectFormat { found: ObjectFormat, expected: ObjectFormat },
    PackMagic,
    PackVersion(u32),
    ObjectCount { count: u32, limit: u32 },
    Truncated,
    EntryType(u8),
    Inflate,
    SizeMismatch { declared: u64, inflated: u64 },
    EntryTooLarge { kind: EntryKind, size: u64, limit: u64 },
    TooMuchContent { total: u64, limit: u64 },
    Trailer,
    TrailingBytes,
}
pub enum EntryKind { Blob, Tree, Commit, Tag, DeltaStream, DeltaBase, DeltaResult }
pub enum PurgeError {
    WorkNotExported { sandbox: SandboxName, state: Box<WorkState> },
    Gateway(GatewayError),
}
pub struct SandboxListing { pub sandboxes: Vec<SandboxSummary>, pub unreadable: Vec<UnreadableSandbox> }
pub struct UnreadableSandbox { pub name: SandboxName, pub error: LabelError }
pub enum LifecycleError {
    Gateway(GatewayError),
    Labels { sandbox: SandboxName, source: LabelError },
    NotFound(SandboxName),
    PowerTimeout { sandbox: SandboxName, wanted: Phase, after: Duration },
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
    Profile(PolicyError),
}
```

`export` steps, in order, each failing with a typed error and writing no host state before the last two:

1. The sandbox must be `Ready`. Resolve the host repository from the workspace label and `config` (`ProjectRoot::open`).
2. Read the sandbox repository with `git worktree list --porcelain -z` and `git for-each-ref refs/heads`. The heads are every branch, the detached HEAD of the main checkout (host ref `HEAD`), and the detached HEAD of each linked worktree (host ref `worktree/<id>`, where `<id>` is the final component of `git rev-parse --absolute-git-dir` for that worktree). Prunable worktrees are ignored. A linked worktree on a branch adds no head of its own. A head needs export when its tip differs from the host branch of the same name, or from the host HEAD for a detached head. With nothing to export the report has `bundle: None`. A branch name or worktree id that is not a valid `BranchName` (for example one with a control or format character) is `UnusableBranch`, and nothing is exported.
3. Refuse before any download: more than 1,024 heads is `TooManyHeads` (`MAX_HEADS`, in `export/bundle.rs`); two targets that are equal or nested, or a target nested with a host ref that exists, is `RefConflict` (`jackin/<sandbox>` against `jackin/<sandbox>/x`); a target that a host worktree has checked out is `RefCheckedOut`.
4. In the sandbox, `git bundle create` writes one bundle of the heads (`refs/heads/<branch>`, `HEAD`, `worktrees/<id>/HEAD`). No temporary ref is created in the sandbox, so the sandbox repository is the same after success, error, and cancel.
5. `download_file` to the staging directory `cache_dir/export-<sandbox>-<unique>`, created with mode 0700 (the gateway compares digests). A file longer than `limits.bundle_bytes` is `BundleTooLarge`, checked with `file_len` before anything reads it.
6. Check the bundle on the host before host git reads it, as described below.
7. Import: `git -c core.bigFileThreshold=64m -c core.deltaBaseCacheLimit=96m index-pack --stdin --strict --threads=1`, with the bundle file as stdin from the pack offset (`ProcessStdin::File`). It never uses `--fix-thin`, so a thin pack is refused, and `--strict` rejects a malformed tree (a `.git` entry, duplicate entries) whatever the repository configuration says. Then one `git update-ref --stdin` transaction writes `option no-deref` before each `update <target> <tip>`, so the targets are forced, written all or nothing, and a symbolic ref at a target is replaced and never written through. Targets are `refs/heads/jackin/<sandbox>/<name>` only. The working tree and the index are never written.
8. Read each host ref back and require it to equal the bundle head (`TipMismatch`). Remove the staging directory, and remove the sandbox bundle with a bound of 10 s (`CLEANUP`). The outcome of the removal never changes the result.

Bundle checks. The header is read through `FileSystem::read_at` in 64 KiB chunks and is bounded at 1 MiB and 1,024 heads. It accepts a v2 header, or a v3 header with at most one `@object-format` capability that equals the object format of the host repository (`git rev-parse --show-object-format`). It refuses a prerequisite line and any other capability. The heads of the header must equal the requested set exactly (`BundleHeads { extra, missing }`). The walker then reads the pack through the same chunks: the `PACK` magic, version 2, the object count (`export_objects`), every entry (blob, tree, commit, tag, and both delta forms), each zlib stream inflated into a fixed 64 KiB buffer whose length must equal the declared size, the trailer hash (SHA-1 or SHA-256, the format of the host repository), and no byte after the trailer. An entry above `export_entry_bytes`, a delta whose declared base or result is above it, or a running total of declared content above `export_total_bytes` is refused. Every refusal is `BundleRefused(BundleRefusal)` and happens before host git reads the bundle, so a refused export leaves the host refs and the host object store unchanged. The walker never loads a whole entry, and it checks the cancel token after every read.
`export` is idempotent: repeating it moves the same refs to the same tips.

`work_state` follows D08. It is `Exported` when the sandbox is `Ready`, no worktree has uncommitted work, and every head has been exported to its host ref or equals the host branch. `git status --porcelain` runs in every non-prunable worktree, and a stash counts as uncommitted work. `Unexported` lists the uncommitted flag, the branches, the detached HEAD, the detached linked worktrees, and the unusable names. Anything jackin cannot inspect (not `Ready`, git failed, workspace gone, an unreadable record or a non-UTF-8 worktree path) is `Unknown(..)` and counts as not exported. A corrupt sandbox index does not stop `export` from landing the branches, but the work state is `Unknown(InspectionFailed)`. `purge(Guarded)` deletes only for `Exported` and otherwise returns `PurgeError::WorkNotExported { state }` with nothing deleted. `purge(Forced)` deletes after computing the state for the report. A missing sandbox returns `AlreadyGone`, so repeating a purge is safe. A power command refuses an unreadable label set before it changes the phase. The supervisor state file is never read here.

`power` refuses before it calls the gateway when the sandbox phase is `Completed` or `Error`: it returns `LifecycleError::MainProcessEnded { sandbox, phase }`, and `phase` carries the exit code. OpenShell 0.1.6 cannot stop or restart such a sandbox (D09), and export needs a `Ready` sandbox, so the operator can only purge it with `--force`. `inspect` builds the D10 closure report from the live policy: for each attached OpenShell provider it passes `RuleKey::provider_rule` in `generated`, so the generated `_provider_<name>` rule is not a violation. A profile of an agent that does not parse is `LifecycleError::Profile`, not a skipped profile.

### Doctor (no gateway needed)

```rust
pub struct DoctorInput<'a> {
    pub runner: &'a dyn ProcessRunner,
    pub fs: &'a dyn FileSystem,
    pub gateway: GatewayAccess<'a>,
    pub engine_socket: HostPath,
    pub pinned: &'a Compatibility,
}
pub enum GatewayAccess<'a> {
    NotChosen,
    Connected(&'a dyn Gateway),
    Failed { name: &'a GatewayName, error: &'a GatewayError },
}
pub enum CheckId { OpenshellBinary, SshBinary, GitBinary, GatewayRegistered, GatewayHealth, PinnedVersion, EngineSocket }
pub enum CheckStatus { Pass, Warn, Fail, Skip }
pub struct Check { pub id: CheckId, pub status: CheckStatus, pub detail: String }
pub struct DoctorReport { pub checks: Vec<Check> }

impl DoctorReport {
    pub fn is_healthy(&self) -> bool;
}

pub async fn doctor(input: &DoctorInput<'_>, cancel: &CancellationToken) -> DoctorReport;
```

`doctor` never fails and never repairs (D13). It runs `openshell --version`, `ssh -V`, and `git --version` through the runner, calls `registered_gateways`, calls `Gateway::health` when the access is `Connected`, compares the versions with `Compatibility`, and checks the engine socket with `stat_following`. Only a socket, or a symlink to one, passes. A missing path, a dangling symlink, a symlink loop, and any other kind of file are `Warn` for the `EngineSocket` check, and `is_healthy` stays true. Version text from the tools and from the gateway passes through `TerminalText::sanitize`, so terminal control and bidirectional characters never reach a detail line. With `NotChosen` or `Failed`, the gateway checks report `Skip` or `Fail` and the other checks still run. One `CheckId` per D13 item, matched exhaustively by the CLI and the console doctor panel.

### Remaining error enums

```rust
pub enum RoleError {
    NoRole,
    UnknownRole(RoleName),
    Manifest { path: HostPath, source: ManifestError },
    RootMissing { path: HostPath },
    RootNotDirectory { path: HostPath },
    ManifestMissing { root: HostPath },
    FileMissing { path: RepoPath },
    FileNotRegular { path: RepoPath },
    EscapesRepository { path: RepoPath },
    Dockerfile(DockerfileError),
    Policy(PolicyError),
    ProviderHostClosure(Vec<ClosureViolation>),
    Git { status: ProcessStatus, output: TerminalText },
    Path(PathError),
    Fs(FsError),
    Process(ProcessError),
}
pub enum CredentialError { LoginUnreadable { path: HostPath }, AuthJsonInvalid { key: String }, NotAChatgptLogin, Fs(FsError) }
pub enum AccountError {
    Credential(CredentialError),
    Gateway(GatewayError),
    Policy(PolicyError),
    Lock(LockError),
    Unsupported { account: AccountName, mode: CodexAuthMode },
    AccountIdMismatch { account: AccountName },
}
```

All error enums derive `Debug` and `thiserror::Error`, and none holds a secret. Their messages follow the error text rule of section 1. `RoleError::Manifest` prints only `invalid role manifest at <path>`, and the report of the binary adds the cause once. `ExportError::BundleHeads` reads `extra a, b; missing none`. `EnvFileProblem`, `LaunchStep`, `ProjectStep`, `EnvQuestion`, `UnknownReason`, `WorkState`, `ObjectFormat`, and `EntryKind` implement `Display` and print words.

### Test plan outline (mark for `test-reviewer`)

Location: `crates/jackin-session/tests/*.rs`. Boundaries faked: the gateway (`FakeGateway`), the image engine (`FakeEngine`), the clock (`FakeClock`), and the filesystem. Host `git` runs for real on temporary repositories, so those tests use `StdFileSystem` on a temporary directory; the rest use `MemFs`. Tests call `plan_launch` and `Session` methods through `World` from `jackin-testkit` and assert the resulting fake gateway state, the host repository refs, and returned values. They never assert the order of gateway calls. They pass the launch ID, and they read the attempt ID from the fake sandbox file `run/start`, where it equals `next_attempt()` of the state they saw.

| Behavior (PLAN P5.1, P5.2; journey) | Positive | Negative |
|---|---|---|
| One launch, one sandbox (J03) | two `launch` runs with one launch ID leave one sandbox, one built final image, one upload, one setup run; the sandbox labels equal `draft.labels()` completed with the built image ID; the name is at most 19 characters | a second launch with a new launch ID makes a second sandbox; two concurrent `launch` calls with one ID end with one sandbox (name fence); a different launch whose suffix collides gives `NameCollision` |
| Plan is pure and draft-complete (fix 2) | `plan_launch` with no gateway connected returns a plan whose `SandboxDraft` has labels without an image; `complete(&built)` sets the tag and the 32-hex image label, and the spec environment is `JACKIN_LAUNCH_ID` only | `plan_launch` called twice with equal inputs returns equal drafts |
| Planned files (P7.4 F1, `[codex] model`) | `files().codex_config` is at `/sandbox/.codex/config.toml` and equals the exact text with the `[projects."/sandbox/project"]` trust entry; with `[codex] model` the text starts with `model = "<name>"`; `files().env` parses back to the role values and the `DOCKER_*` names | without `[codex] model` there is no `model` line |
| Image identity in the cache (decision 3) | a second launch of the same role with a changed construct image ID in the fake engine builds a new final image and creates a sandbox with a different image label | an unchanged construct leaves the image and its label unchanged |
| Crash and resume | fault the gateway after `create_sandbox`, then call again: the run continues from the observed row and ends `Ready`; fault after upload before the marker: the second run uploads again and ends `Ready` | a fault in `ensure_image` leaves no sandbox; a fault in the upload leaves the sandbox and `LaunchFailure.sandbox` names it |
| Observed phases | a `Stopped` sandbox is started and waited; `Provisioning` becomes `Ready` under the fake clock; a `Stopped` sandbox whose old `run/state` says `running 1` is not finished early: the run waits for `waiting` and then starts attempt 1 | `Completed { exit_code: 0 }`, `Error { exit_code: Some(3) }`, and `Suspended` stop with `MainProcessEnded` or `SandboxSuspended` and the sandbox is still there; two sandboxes with the launch label give `AmbiguousLaunch` |
| Transient create failure (P7.4 F8) | with `Fault::ProvisionFails { creates: 1 }` the run deletes the failed sandbox, creates again, and ends `Ready` with one sandbox | with `creates` above `create_attempts` the run fails with `ProvisioningFailed` naming the last sandbox, which remains; an `Error { exit_code: None }` sandbox from an earlier run is not deleted |
| Hooks and env (J06, ADR-009, D11) | the env file in the fake sandbox holds the `[env]` values; setup runs once across two launches of one ID and preflight at every start (`SandboxHandle::runs`), and each run saw the `[env]` values; a retry with a new `--env` value replaces the file | a failing preflight returns `HookFailed` with sanitized output, the sandbox stays `Ready`, `export` still works, and the retry without the fault ends in `Running`; a failing setup writes no marker and runs again |
| Supervisor protocol (J07, D09) | after `Start`, the fake state is `running 1` and `run/start` holds `1`; `restart_agent` after `exited 1 3` runs preflight and writes `2`, and the state becomes `running 2`; after a stop and a start the state is `waiting` and the next request is `1` again | `restart_agent` on `running` returns `AlreadyRunning` and writes nothing; invalid state text gives `SupervisorReading::Invalid` and the `Supervisor` row waits then times out; a request `0` and a repeated `2` start nothing (fake model) |
| Cancel | cancel during the upload ends with `Cancelled(RestoreCandidate)` naming the sandbox, and `list` shows it | cancel before `create` leaves no sandbox; cancel during a hook stops the fake program run |
| Plan rules (P5.4) | account order of D26 picks the right account in a table; Docker is off by default; role or workspace enables it; `Enabled` adds exactly one policy rule and the three `DOCKER_*` names to the env file, and changes the image request; `Prompting::Interactive` returns a question for every `[env]` entry with its default and prompt, and the answers give a plan | no account gives `Selection` naming the binding; Docker enabled with no engine gives `Docker(EngineMissing)`; a mode A account gives `Unsupported`; with `Prompting::Never` a missing `[env]` value without default gives `EnvNeeded` and one with a default does not; `--env` for an undeclared name gives `EnvUndeclared` |
| Role validation without a gateway (G-ROLE, fix 6) | `RoleCatalog::new(fs, runner, cache_dir).load_path` loads a valid role with no `Session`; a dirty role tree yields `RoleRevision::Dirty` and a changed final tag | an unknown manifest field, a final stage without `FROM construct`, a role policy rule for a Codex host (decision 14: `parse_authored` and closure), and a role policy with `landlock.compatibility` other than `hard_requirement` each give the right `RoleError` |
| Credentials (J08) | `auth.json` is written with mode 0600, the placeholder token, an empty refresh token, and the real account ID; the Codex `config.toml` in the fake sandbox equals `files().codex_config`; the Docker key file has mode 0600 | the fake gateway files, the events, every `LaunchError` `Display`, and the process args contain no secret value from the fixture login |
| Attach and shell (J11, J12) | attach opens a main terminal with the repaint policy and returns the supervisor reading; shell opens a second terminal with the env file values and the fake main process is unchanged | attach to a `Completed` sandbox gives `MainProcessEnded` with the code; shell on a sandbox with no env file gives `EnvFile { problem: Missing }` |
| Reply filter (P7.4 F3, D09) | the fake main process replays `ESC [ 6 n` while the state is `waiting`; a CPR reply, a DA1 reply, and an OSC 11 reply sent through `AttachedTerminal::input` never reach `SandboxHandle::main_input()`, and the plain bytes sent with them do; once the state is `running 1` the same replies arrive; after `exit_codex(3)` they are dropped again | a reply with an unreadable or invalid state file is dropped; typed text passes in every state |
| Export (J13) | a branch committed in the fake sandbox repository appears at `refs/heads/jackin/<sandbox>/<branch>` in the host repository with the same tip; the host working tree and index are byte-equal before and after; repeating gives the same refs | sandbox not `Ready`; workspace removed from config; ref collision gives `RefConflict` and writes nothing; a corrupted bundle (fault `CorruptDownload`) fails and leaves no ref |
| Work state and purge (J14, ADR-010) | after export `work_state` is `Exported` and `purge(Guarded)` deletes; `purge(Forced)` deletes unexported work; a repeated purge returns `AlreadyGone` | an uncommitted file gives `Unexported`; a stopped sandbox gives `Unknown(NotRunning)` and guarded purge refuses; `list` and `purge` leave other sandboxes unchanged |
| List by label | `list` returns summaries from labels only, with no hook run recorded in any fake sandbox | a sandbox without jackin labels is absent |
| Lifecycle (J14, fix 4) | `power(Stop)` then `power(Start)` keep fake files and reset the supervisor state to `waiting` | `power(Stop)` and `power(Start)` on `Error` and on `Completed` return `MainProcessEnded` with the phase and exit code, and the fake sandbox phase is unchanged afterwards (a call would have moved `Error` to `Starting`) |
| Inspect closure (decision 14) | a live policy with the generated `_provider_<name>` rule reports no violation | a live policy with an extra rule on a Codex host reports it |
| Account sync (J17) | a Codex ChatGPT account creates the profile and the provider; a changed embedded profile is replaced and reports `Updated`; another provider gives `HostOnly` and `providers()` and `profiles()` of the fake stay empty | a Codex account with an `Env` source gives `Unsupported`; a lost update race (`FailNext` with `FailedPrecondition`) leaves the old profile and the launch fails with the gateway error |
| Doctor (J01, fix 6) | all checks pass on healthy fakes; with `GatewayAccess::NotChosen` the binary and registry checks still run | a missing `openshell` binary, an unregistered gateway, `Failed` access, an unhealthy gateway, a wrong version, and a missing socket each give the right status for the right `CheckId` |

---

## 5. The usage boundary (crates/usage, D25, D26)

Role: account usage tracking for every provider, on the host. Everything in this section is built in U13 (P6.3): units U0 to U12 contain no reference to `jackin-usage`. The 34 usage crates keep their internals and merge where a crate has one caller (P6.3). The product sees one crate, `jackin-usage` (`crates/usage/jackin-usage`), and one entry type, `HostUsage`. Everything below it (`jackin-usage-provider-*`, `-coordinator`, `-snapshot-store`, `-credential-resolver`, and so on) is private to the usage subtree. There is no `UsageSource` trait: `HostUsage` is code jackin owns, and tests fake its network (`ProviderEndpoints`), not the code.

| Direction | Type | Owner |
|---|---|---|
| in | `UsageRequest { accounts, refresh }` built from `Config` | `jackin-usage`, over `jackin-core` types (`AccountName`, `Provider`, `CredentialSource`) |
| in | `UsageDeps { clock, fs, runner, env, endpoints, state_dir }` | boundary traits of `jackin-core` |
| out | `UsageReport` (also the `jackin usage --json` schema) | `jackin-usage` |
| never | `QuotaBucketView`, `ProviderError`, `jackin-protocol` types, any secret | stays inside usage |
| never | a credential into a sandbox | structural: `jackin-usage` cannot depend on `jackin-openshell` or `jackin-session` (layer table) |

### Public API of `jackin-usage`

```rust
pub struct UsageAccount { pub name: AccountName, pub provider: Provider, pub credential: CredentialSource }

impl UsageAccount {
    pub fn from_account(account: &Account) -> UsageAccount;
}

pub enum Refresh { Cached, Force }
pub struct UsageRequest { pub accounts: Vec<UsageAccount>, pub refresh: Refresh }
pub struct UsageReport { pub schema: u32, pub generated_at: Timestamp, pub accounts: Vec<AccountUsage> }
pub struct AccountUsage { pub account: AccountName, pub provider: Provider, pub freshness: Freshness, pub outcome: UsageOutcome }
pub enum Freshness { Fresh, Stale { since: Timestamp }, Refreshing }
pub enum UsageOutcome { Windows(Vec<LimitWindow>), Unavailable(UnavailableReason) }
pub struct LimitWindow {
    pub kind: WindowKind,
    pub label: String,
    pub used: Option<Percent>,
    pub remaining: Option<Remaining>,
    pub resets_at: Option<Timestamp>,
}
pub enum WindowKind { FiveHour, Weekly, Monthly, Credits, Spend, Other }
pub struct Percent(f64);
pub enum Remaining { Percent(Percent), Amount { value: String, unit: String } }
pub enum UnavailableReason {
    NoCredentials,
    ExpiredLogin,
    Unsupported,
    RateLimited { retry_at: Option<Timestamp> },
    Network(String),
    Provider(String),
}
pub struct ProviderEndpoints { }
pub struct UsageDeps {
    pub clock: Arc<dyn Clock>,
    pub fs: Arc<dyn FileSystem>,
    pub runner: Arc<dyn ProcessRunner>,
    pub env: EnvSnapshot,
    pub endpoints: ProviderEndpoints,
    pub state_dir: HostPath,
}
pub struct HostUsage { }

impl HostUsage {
    pub fn new(deps: UsageDeps) -> HostUsage;
    pub async fn report(&self, request: &UsageRequest, cancel: &CancellationToken) -> Result<UsageReport, UsageError>;
}

impl Percent {
    pub fn new(value: f64) -> Result<Percent, UsageError>;
    pub fn get(self) -> f64;
}

impl ProviderEndpoints {
    pub fn real() -> ProviderEndpoints;
    pub fn with(self, provider: Provider, base_url: String) -> ProviderEndpoints;
}

pub enum UsageError { Cancelled, InvalidPercent(f64), Store { path: HostPath, message: String }, Internal(String) }
```

Rules.

- A per-account problem is a value (`Unavailable(reason)`), never a `UsageError`. One broken account does not hide the others (D25: "shows the reason, for example an expired login"). `UsageError` is for failures of the whole request.
- `Percent` is in `0.0..=100.0` and never NaN. `Remaining::Amount.value` is a decimal string so the report round-trips without float error.
- `UsageReport` derives `Serialize` and `Deserialize`; its key sets are pinned by tests. `jackin usage --json` prints it unchanged. `schema` is `1`. `xtask e2e` parses it for J17.
- Dispatch from `Provider` to a provider crate is one exhaustive `match` in `jackin-usage/src/dispatch.rs` (decision 10). Adding a `Provider` variant to `jackin-core` fails the build until the arm exists. No second provider list exists.
- Callers build the account set from config: all accounts for `jackin usage` and the usage panel, `Config::workspace_accounts(name)` for the per-workspace view (D26).
- Credentials are read inside the usage crates from the account's `CredentialSource`: an environment variable from the snapshot, `op read` through the runner, a file, or the Codex home `auth.json`. They are never returned in a report and never logged.
- `ProviderEndpoints::with` is the network fake point. Production uses `real()`. The layer rules for this subtree are rows of the layer table in the Shape section.

### Test plan outline (mark for `test-reviewer`; written in P6.3)

Location: `crates/usage/jackin-usage/tests/*.rs`. Boundaries faked: the network (`UsageServer`, a local HTTP server from `jackin-testkit`, reached through `ProviderEndpoints::with`), the filesystem (`MemFs`), the clock, and the `op` program (`ScriptedProcessRunner`).

| Behavior | Positive | Negative |
|---|---|---|
| Codex windows (J17) | a Codex account with a ChatGPT login returns `FiveHour` and `Weekly` windows with used percent and reset time | a server 401 gives `Unavailable(ExpiredLogin)`; a malformed body gives `Unavailable(Provider(..))`, not a panic |
| Every provider has an arm | for each `Provider::ALL` the dispatcher returns a window set or a typed `Unsupported`, never a panic | none (compile-time exhaustiveness covers a missing arm) |
| One bad account does not hide the rest | two accounts, one network failure: the report has one `Windows` and one `Unavailable(Network)` | cancel returns `UsageError::Cancelled` |
| Credential sources | env var, `op://` through the scripted runner, file, and Codex home each resolve | a missing env var gives `NoCredentials`; a failing `op` gives `NoCredentials` and its stderr tail is absent from the report |
| No secret leaves | the serialized report and `Debug` output contain no fixture secret | a provider error body that echoes a token is masked before it enters `Provider(..)` |
| JSON schema | `serde_json` of a fixed report equals the golden file; the generated schema equals `rewrite/usage-schema.json` | a changed field name fails the golden test |
| Cache and freshness | a second `report(Cached)` within the TTL makes no HTTP request and returns `Fresh`; `Force` requests again | an HTTP failure after a good report returns `Stale { since }` with the old windows |
| Boundary | the xtask layer step passes on the real graph | a fixture graph with `jackin-session -> jackin-usage` fails with the rule named |

---

## 6. jackin-tui

Role: the console (D27, D28). It wires the `junie-tui` blocks (the baseline library, pinned by `rev`) to jackin data. It keeps the baseline `Application`, `Page`, `Request` loop and adds no second loop. It holds `Session` values, a `RoleCatalog`, and a `HostUsage`. The baseline data interfaces, the terminal pane block, the external-message input, and the `clap` feature are baseline work (P9.1, P9.2) and are used here, not redefined. The baseline has only `runtime::Application` today, so every item that names the terminal pane block (`junie_tui::terminal::TerminalState`) or a baseline data interface arrives with U14, and every item that names `jackin-usage` arrives with U13 (section 10).

### Module map

```text
crates/jackin-tui/src/
  lib.rs
  console.rs         Console, ConsoleDeps, SessionSet, GatewayState, ConsoleStart, ConsoleExit, ConsoleError, run_console
  bridge.rs          pub(crate) external messages into the baseline loop
  msg.rs             pub(crate) Msg
  model.rs           pub(crate) Model: config snapshot, fleet, usage, doctor, launches, tabs
  panes.rs           pub(crate) Pane, PaneKind, Link, PaneId, layout, size sync
  keys.rs            pub(crate) chord table: detach, close, focus; Ctrl-D never forwarded
  pages.rs           pub(crate) page registry
  pages/{manager,editor,role_editor,accounts,usage,cockpit,doctor,gateways}.rs
                     roster and actions; workspace editor with the Docker row; role sources and bindings;
                     accounts, defaults, bindings, 1Password flow; usage panel; launch cockpit;
                     doctor and gateway health; gateway entries
```

### Public surface

```rust
pub enum GatewayState { Connected(Session), Failed(GatewayError) }
pub struct SessionSet { }

impl SessionSet {
    pub fn new(default: GatewayName, entries: BTreeMap<GatewayName, GatewayState>) -> SessionSet;
    pub fn get(&self, name: &GatewayName) -> Option<&Session>;
    pub fn access(&self, name: &GatewayName) -> GatewayAccess<'_>;
    pub fn default_name(&self) -> &GatewayName;
}

pub struct ConsoleDeps {
    pub config: ConfigStore,
    pub sessions: SessionSet,
    pub roles: Arc<RoleCatalog>,
    pub usage: Arc<HostUsage>,
    pub host: HostPorts,
    pub env: EnvSnapshot,
    pub cwd: HostPath,
    pub engine_socket: HostPath,
}
pub enum ConsoleStart { Home, Launch(LaunchRequest) }
pub enum ConsoleExit { Quit }
pub struct Console { }

impl Console {
    pub fn new(deps: ConsoleDeps, runtime: tokio::runtime::Handle, start: ConsoleStart) -> Console;
}

impl junie_tui::runtime::Application for Console { }

pub fn run_console(deps: ConsoleDeps, runtime: tokio::runtime::Handle, start: ConsoleStart) -> Result<ConsoleExit, ConsoleError>;

pub enum ConsoleError { Terminal(std::io::Error), Config(ConfigError) }
```

`Console` implements the baseline `Application`. Tests and the identity gate drive it through `handle(Input)` and `render`, so they use the production entry point. A gateway that failed to connect stays in the set as `Failed`, so the console still opens and the doctor panel shows the error.

### Internal model (the P0.6 recommendation as types)

```rust
pub(crate) struct Model {
    config: Config,
    fleet: Vec<SandboxSummary>,
    usage: Option<UsageReport>,
    doctor: Option<DoctorReport>,
    launches: BTreeMap<LaunchId, LaunchView>,
    tabs: Vec<Tab>,
    focus: PaneId,
}
pub(crate) struct Tab { gateway: GatewayName, sandbox: SandboxName, panes: Vec<Pane> }
pub(crate) enum PaneKind { Codex, Shell }
pub(crate) struct Pane {
    id: PaneId,
    kind: PaneKind,
    generation: u64,
    link: Link,
    screen: junie_tui::terminal::TerminalState,
    forwarded: TermSize,
}
pub(crate) enum PaneTerminal { Codex(AttachedTerminal), Shell(TerminalSession) }
pub(crate) enum Link { Detached, Attaching, Attached(tokio::sync::mpsc::Sender<TerminalCommand>), Exited(Option<i32>), Failed(String) }
pub(crate) enum Msg {
    FleetRefreshed(Result<Vec<SandboxSummary>, LifecycleError>),
    UsageRefreshed(Result<UsageReport, UsageError>),
    DoctorFinished(DoctorReport),
    Launch(LaunchEvent),
    LaunchDone(Result<LaunchOutcome, LaunchFailure>),
    PaneOpened { pane: PaneId, generation: u64, result: Result<PaneTerminal, AttachError> },
    PaneEvent { pane: PaneId, generation: u64, event: TerminalEvent },
    ConfigSaved(Result<Config, ConfigError>),
    Tick,
}
```

Ownership and concurrency.

- One owner of all UI state: `Model`, mutated only inside `Application::handle` after the bridge delivers a `Msg` (the baseline external-message input). Render reads and does no I/O.
- Each pane has one forwarding task that owns the `AttachedTerminal` of its pane (a shell pane owns a `TerminalSession`). The model keeps only a `TerminalCommand` sender, so input, resize, and detach go to the task, which sends `Input` bytes through `AttachedTerminal::input` (the reply filter, section 4) and turns `next_event` into `Msg::PaneEvent`. The task exits when the session ends. A `generation` is bumped on every attach and carried by `Msg`, so late events of a closed session are dropped.
- Invariant: a `Tab` holds at most one `PaneKind::Codex` pane. `Model::open_codex(sandbox)` returns the existing pane. One function computes the pane layout for render and for the size sync; the inner size of each pane is sent as `Resize` only when it changed and is at least 20x5.
- Config writes: every edit is `ConfigStore::edit(vec![ConfigEdit..])` in a task, answered by `Msg::ConfigSaved(Ok(config))`, which replaces the snapshot. A save error shows a dialog. No write path skips `ConfigEdit`.
- Chords: detach, close, and focus switch belong to `keys.rs`. Byte `0x04` is never sent to a Codex pane. A lone Ctrl-P is held until the next key and sent with it (D09, P0.6 fact 3). Each Codex attach uses `RepaintPolicy::NudgeAfter(600 ms)` through `Session::attach`, and the pane builds a fresh `TerminalState` (D28).
- On `Exited` or `Ended` for a Codex pane, the model refreshes the fleet. The exit code shown comes from `SandboxSummary.phase`, not from the SSH status (P0.6 fact 6). No retry.

Screen to data to edit (D27, D28):

| Screen | Reads | Writes through |
|---|---|---|
| Manager roster | `fleet`, `config.workspaces()` | `Session::power`, `purge`, `attach`, `open_shell`, `inspect` |
| Launch picker and cockpit | `config`, `LaunchEvent` stream, `LaunchStep::ALL` | `plan_launch`, `Session::launch`, the cancel token |
| Workspace editor, Docker setting row | `config.workspace(..)` | `ConfigEdit::SetWorkspaceDocker`, `AddWorkspace`, `RemoveWorkspace`, `BindWorkspaceAccount` |
| Role editor and role binding | `config.roles()` | `ConfigEdit::AddRole`, `RemoveRole`, `BindRoleAccount` |
| Accounts, defaults, bindings | `config.accounts()`, defaults | `ConfigEdit::AddAccount`, `SetDefaultAccount`, `RemoveAccount`, then `Session::sync_account` or `forget_account` |
| Gateways | `config.gateways()` | `ConfigEdit::AddGateway`, `RemoveGateway`, `SetDefaultGateway` |
| Usage panel | `UsageReport` | `HostUsage::report` (all accounts, or `Config::workspace_accounts`) |
| Doctor and gateway health | `DoctorReport` | `doctor` over `SessionSet::access` |

The D24 warning is rendered from `DockerRisk::ALL` in the launch picker before the launch starts and in the cockpit. The stage rail is rendered from `LaunchStep::ALL` with one `match` per label, so a new step is a compile error until it has a label. The console creates a workspace for a launch from an unknown directory (decision 8). The 1Password flow edits `CredentialSource::OnePassword`; the usage crates resolve it on the host.

### Test plan outline (mark for `test-reviewer`; P9.3 and P9.4)

Location: `crates/jackin-tui/tests/*.rs`. Boundaries faked: the gateway (`FakeGateway`), the image engine, the clock, the filesystem (`MemFs` behind `ConfigStore`), and the network for usage (`UsageServer`). Tests build a `World` from `jackin-testkit`, construct `Console`, feed `Input::Key` values, call `render` into a buffer, and assert the config in the store, the fake gateway state, and the rendered text. `jackin-tui` dev-depends on `jackin-testkit`, which depends on `jackin-tui`; integration tests link one copy of the crate.

| Behavior | Positive | Negative |
|---|---|---|
| Launch from the roster | Enter on a workspace, pick the sentinel role, confirm: a sandbox exists in the fake gateway with the D07 labels and a Codex pane is `Attached` | Esc at the confirm step changes nothing; a plan error (no account) shows the dialog and creates no sandbox |
| Launch from an unknown directory (decision 8) | the console opens the workspace editor, saves `AddWorkspace`, and launches | Esc leaves the config unchanged |
| Cancel in the cockpit | the cancel key during upload leaves the sandbox and shows the restore candidate | cancel before create leaves none |
| Docker setting | toggling the workspace row writes `docker = true` to the stored config; the launch picker shows the four D24 risks | with the setting off no risk text is rendered |
| Accounts and bindings (J17) | add a Codex account, set default, bind to a workspace: the stored config has all three; the launch uses the workspace binding | removing a bound account shows `AccountInUse` and leaves the config unchanged |
| Usage panel | the fixture server's report renders 5-hour and weekly windows for bound accounts | an `Unavailable(ExpiredLogin)` account renders its reason, and the rest still render |
| Panes (D28) | open a Codex pane, then a shell pane: two sessions, the main process unchanged; resize sends the inner size once; detach keeps the process; re-attach builds a fresh screen and the nudge arrives | a second open of the Codex pane for the same sandbox focuses the existing one (the fake shows one main attachment); Ctrl-D is never delivered to the fake main process; a stale-generation event is ignored |
| Failed gateway | a gateway in `Failed` state shows its error in the doctor panel and the console still opens | a launch against it shows the unavailable dialog and creates nothing |
| Purge dialog (ADR-010) | purging an exported sandbox deletes it | purging an unexported sandbox shows the refusal and keeps it; the force path needs the explicit confirm |
| Identity gate (P9.4) | the `console-fixture` binary of `jackin-testkit` runs the console on fixture data in a PTY; every demo key sequence matches its baseline | a one-cell color change in a baseline file fails the gate (red run recorded by `test-reviewer`) |

---

## 7. jackin-testkit

Role: in-process fakes for the boundaries and fixtures. It is a dev-dependency of every other crate. The one non-test binary is `console-fixture`, used by the identity gate. The fakes obey the same contracts as the real implementations, proven by the shared contract suite and not by review alone. Boundaries faked here: the gateway, the image engine, the network, the clock, the filesystem, and the host programs `docker`, `openshell`, `ssh`, and `op` (decision 9). Nothing else has a fake.

### Module map

```text
crates/jackin-testkit/src/
  lib.rs
  world.rs         World, WorldBuilder
  gateway.rs       FakeGateway, Fault, SandboxHandle
  sandbox.rs       FakeSandbox: files, labels, phase, supervisor model, host-mapped git
  supervisor.rs    SupervisorModel
  engine.rs        FakeEngine
  host.rs          FakeClock, MemFs, ScriptedProcessRunner, ScriptedProgram, TestRunner
  snapshot.rs      HostSnapshot, HostState, object_store_files
  usage.rs         UsageServer (from U13)
  fixtures.rs      RoleFixture, RepoFixture, baseline-mirroring data
  bin/console-fixture.rs
```

### Public surface

```rust
pub struct World {
    pub gateway: Arc<FakeGateway>,
    pub engine: Arc<FakeEngine>,
    pub clock: Arc<FakeClock>,
    pub fs: Arc<dyn FileSystem>,
    pub runner: Arc<TestRunner>,
    pub usage: Arc<UsageServer>,
    pub session: Session,
    pub roles: Arc<RoleCatalog>,
    pub config: ConfigStore,
}
pub struct WorldBuilder { }

impl World {
    pub fn builder() -> WorldBuilder;
    pub async fn current_config(&self) -> Config;
}

impl WorldBuilder {
    pub fn role(self, fixture: RoleFixture) -> WorldBuilder;
    pub fn account(self, account: Account) -> WorldBuilder;
    pub fn workspace(self, name: WorkspaceName, repo: RepoFixture) -> WorldBuilder;
    pub fn docker_engine(self, engine: EngineConfig) -> WorldBuilder;
    pub fn supervisor(self, model: SupervisorModel) -> WorldBuilder;
    pub async fn build(self) -> World;
}

pub struct FakeGateway { }

impl FakeGateway {
    pub fn new(name: GatewayName, workspace: OpenShellWorkspace) -> FakeGateway;
    pub fn inject(&self, fault: Fault);
    pub fn script_program(&self, program: &SandboxPath, script: ProgramScript);
    pub fn sandboxes(&self) -> Vec<SandboxHandle>;
    pub fn sandbox(&self, name: &SandboxName) -> Option<SandboxHandle>;
    pub fn providers(&self) -> Vec<ProviderName>;
    pub fn profiles(&self) -> Vec<ProfileId>;
    pub fn set_version(&self, version: &str);
}

pub struct SandboxHandle { }

impl SandboxHandle {
    pub fn name(&self) -> SandboxName;
    pub fn phase(&self) -> Phase;
    pub fn labels(&self) -> Labels;
    pub fn environment(&self) -> BTreeMap<EnvName, String>;
    pub fn file(&self, path: &SandboxPath) -> Option<Vec<u8>>;
    pub fn file_mode(&self, path: &SandboxPath) -> Option<FileMode>;
    pub fn files(&self) -> BTreeMap<SandboxPath, Vec<u8>>;
    pub fn links(&self) -> BTreeMap<SandboxPath, String>;
    pub fn supervisor(&self) -> Option<SupervisorState>;
    pub fn attachments(&self) -> usize;
    pub fn main_input(&self) -> Vec<u8>;
    pub fn print_main(&self, bytes: &[u8]);
    pub fn runs(&self, program: &SandboxPath) -> Vec<RunRecord>;
    pub fn commit_in_project(&self, branch: &BranchName, file: &str, content: &str);
    pub fn write_project_file(&self, relative: &RepoPath, content: &[u8]);
    pub fn exit_codex(&self, code: i32);
    pub fn exit_main(&self, code: i32);
}

pub struct ProgramScript { pub exit_code: i32, pub stdout: Vec<u8>, pub stderr: Vec<u8> }
pub struct RunRecord { pub env: BTreeMap<EnvName, String>, pub exit_code: i32 }
pub enum Fault {
    FailNext { op: GatewayOp, error: GatewayError },
    FailAfter { op: GatewayOp, calls: u32, error: GatewayError },
    StallProgram { program: SandboxPath },
    CorruptDownload,
    ProvisionFor { polls: u32 },
    ProvisionFails { creates: u32 },
}
pub enum SupervisorModel { Cooperative, ExitsOnStart { code: i32 }, NeverWritesState }
pub struct FakeEngine { }

impl FakeEngine {
    pub fn new() -> FakeEngine;
    pub fn images(&self) -> BTreeMap<ImageTag, ImageId>;
    pub fn builds(&self) -> Vec<BuildRecord>;
    pub fn pin_id(&self, tag: &ImageTag, id: ImageId);
    pub fn fail_next(&self, stage: BuildStage);
}

pub struct BuildRecord { pub stage: BuildStage, pub tag: ImageTag }
pub struct FakeClock { }
pub struct MemFs { }
pub struct UsageServer { }
pub struct TestRunner { }
pub struct ScriptedProcessRunner { }
pub enum ScriptedProgram { Docker, Openshell, Ssh, Op }

impl FakeClock {
    pub fn new() -> FakeClock;
    pub fn advance(&self, by: Duration);
}

impl MemFs {
    pub fn new() -> MemFs;
    pub fn fail_at(&self, op: FsOp, nth: u32, error: FsError);
    pub fn read_string(&self, path: &HostPath) -> Option<String>;
}

impl ScriptedProcessRunner {
    pub fn expect(&self, program: ScriptedProgram, args_prefix: &[&str], reply: ProcessReply);
    pub fn seen(&self) -> Vec<ProcessSpec>;
}

impl UsageServer {
    pub async fn start() -> UsageServer;
    pub fn serve(&self, provider: Provider, response: HttpFixture);
    pub fn endpoints(&self) -> ProviderEndpoints;
}

pub struct RoleFixture { }
pub struct RepoFixture { }
```

`World.usage` and `UsageServer` arrive with U13. `TestRunner` is the `ProcessRunner` of a `World`. It runs `Program::Git` for real and answers `Docker`, `Openshell`, `Ssh`, and `Op` from a `ScriptedProcessRunner`; an unscripted spec is an error that names it. `ScriptedProcessRunner` alone cannot be given a `Git` expectation: `ScriptedProgram` has no such variant, so faking git does not compile. `TestRunner::expect_git_version(reply)` scripts the reply to `git --version` and nothing else; every other git call runs for real. A recorded `ProcessStdin::File` is kept as `SeenStdin::File { path, offset }`. Fixture git is `/usr/bin/git` with `GIT_CONFIG_GLOBAL=/dev/null` and `GIT_CONFIG_NOSYSTEM=1`. `World` uses `StdFileSystem` over a temporary directory, because upload and export run real `git` on real files; tests that need fault injection or no disk use `MemFs`.

Semantics the fakes must match (each is also a contract-suite case or a journey step).

- `FakeGateway` checks what the types cannot: names are unique, and a create returns before `Ready` (`ProvisionFor` sets how many polls stay `Provisioning`). With `ProvisionFails { creates: n }` the first `n` creates end in `Error` with no exit code and `Provisioning::Failed` (P7.4 F8). Profiles carry a resource version, and `ensure_profile` updates only with the version it read, as the real RPC does. The spec environment of a sandbox is `JACKIN_LAUNCH_ID` only; the role environment is a file like any other.
- Phases (fix 4): `exit_main(0)` sets `Completed { exit_code: 0 }` and `exit_main(n)` sets `Error { exit_code: Some(n) }`. After that, `run_program`, `read_files`, `write_file`, and terminals return `NotReady { phase }`, `set_power(Stop)` fails with `MainProcessFailed`, and `set_power(Start)` returns a `Starting` view that never becomes `Ready`. This is the observed 0.1.6 behavior for `Error` (D09, P7.1 F3); `Completed` uses the same model until P4.1 observes it (open question 2). Stop on a `Ready` sandbox and start afterwards keep files under `/sandbox` and reset the supervisor model to `waiting`.
- `run_program` runs a closed set of absolute programs against the sandbox state: `/usr/bin/git`, `/usr/bin/printenv`, and `/usr/bin/sha256sum`, plus programs registered with `script_program` (the hook paths). Any other program returns exit code 127. Each run is recorded with the environment it received (`SandboxHandle::runs`), which is how a test sees that a hook got the env file values. There is no shell and no argument parsing beyond one rule: an argument that starts with `/sandbox/` maps to the host directory that backs `/sandbox`. When the program exits 0 and `touch_on_success` is set, the fake creates that file in the same step. `git` is the real host `git` in a temporary directory with the working directory mapped, so bundles and refs are real.
- `upload_tree` copies the files that `git ls-files -co --exclude-standard` lists, run through the user's git configuration with `-c core.fsmonitor=false -c core.hooksPath=/dev/null`. It keeps symlinks as symlinks, archives a listed directory whole, skips a missing path, never deletes, and never copies `.git`. Before it copies anything it refuses a listed name that is not UTF-8, and any entry (also inside a listed directory) that is not a file, a directory, or a symlink; the error is `unsupported file type for upload: <path>` with `Exited(1)`, and the sandbox stays untouched, as with the live CLI. A listed path whose parent inside the sandbox is a symlink that the same upload created is the live failure of the CLI: the fake copies every other listed entry and then fails with `Transfer { op: UploadFiltered, status: Exited(1) }` and a `tar ... Cannot open: Not a directory` tail. A source link that is not listed is still followed and the bytes behind it are copied, as with the CLI. An upload to an absent sandbox fails with `sandbox not found`, and one to a sandbox that exists but does not accept exec fails with `sandbox is not ready`; both are `Transfer` errors with `Exited(1)`. `upload_file` copies one file to an exact name. `download_file` writes through a temporary file beside the destination, compares digests, renames it into place, and returns the digest. A failed or cancelled download leaves an existing destination as it was and no temporary file. `CorruptDownload` flips a byte so the digest check fails with `Transfer { Digest }`.
- `SandboxHandle::files()` returns the regular files under `/sandbox` and follows no link; `links()` returns every symlink with its target text, so a link never appears as the file it points to. `HostSnapshot::take(project)` records the exported refs and a `HostState` (index, refs, HEAD, status, config, files with their modes, links by target text, and empty directories), so a change of a retargeted link, a dangling link, or an empty directory is visible. `object_store_files(project)` lists the files of the host object store. Export tests compare snapshots to prove that a refused export changed nothing. `set_version` sets the version that `health` reports.
- `SupervisorModel` follows D09: it removes old state and writes `waiting` at start and after `power(Start)`, and resets its last handled ID to 0; when `run/start` holds an ID greater than the last handled one it writes `running <attempt>`; `ExitsOnStart { code }` then writes `exited <attempt> <code>`; a repeated, stale, or zero ID is ignored. `NeverWritesState` models a broken supervisor. The model is written against the `SupervisorState` grammar of `jackin-core`, and P7.4 evidence proves the real script against the same grammar.
- `open_terminal` returns a `TerminalSession::pair`. The fake serves the driver end from a scripted main process: replay of the retained output (capped at 1 MiB, the window may start mid-sequence when a test asks), resize recorded, detach keeps the process, a Ctrl-D byte detaches, exit delivers `Exited`, and later attaches are refused. It models the 0.1.6 input lease (section 3): the first `Owner` gets `Lease(Owner)` and its input reaches the process (`main_input`), a second attachment gets `Lease(ReadOnly)` and its input is dropped, it takes the lease on its next input after the owner left, a Ctrl-C in a read-only attachment ends only that attachment, and `Viewer` input is always ignored.
- `FakeClock::sleep` advances virtual time at once and wakes pending sleepers in order. Nothing in the fakes uses wall time.
- `MemFs` supports `lock`, `write_new` with `AlreadyExists`, and fault injection at the n-th call of an operation, so the atomic config write is tested at every step.
- `FakeEngine` is an in-memory image store. `find` returns the ID of a built tag. `build` records the stage and tag and returns a deterministic ID derived from the tag, unless `pin_id` set one, which is how a test changes the construct image ID (decision 3). It never interprets build arguments.
- `UsageServer` (from U13) serves canned HTTP bodies per provider on a local port, and `endpoints()` returns the override that points `HostUsage` at it.
- The testkit defines no error enum. Fakes return the real error types of the trait they implement (`GatewayError`, `EngineError`, `FsError`, `ProcessError`), so a test sees what production code sees.

### Test plan outline (mark for `test-reviewer`)

Location: `crates/jackin-testkit/tests/*.rs`. The fakes are code jackin owns, and AGENTS.md says not to fake code we own. They stand in for external systems only, so the testkit is tested through the contract suite and a few behaviors, not line by line.

| Behavior | Positive | Negative |
|---|---|---|
| Gateway parity | `contract::run(&FakeGateway, .., group)` passes in nextest for `Core`, `Transfer`, and `Terminal` | a fake variant that drops labels, or lets a read-only attachment write, fails the suite (kept as a mutation test with `test-reviewer`) |
| Ended main process (fix 4) | after `exit_main(3)` the phase is `Error { exit_code: Some(3) }`, `Stop` gives `MainProcessFailed`, `Start` leaves `Starting` | none |
| Supervisor model | `Cooperative` goes `waiting`, `running 1`, then ignores a stale `0` and a repeated `1`; `ExitsOnStart(3)` ends `exited 1 3`; after `power(Stop)` and `power(Start)` the state is `waiting` and `1` starts again | a request `0` or a repeated ID starts nothing |
| Fault injection | `FailAfter` triggers on exactly the n-th call; `ProvisionFor` keeps `Provisioning` for n polls | with no fault all calls pass |
| Engine | a second `build` of one tag returns the same ID; `pin_id` changes it | `find` of an unknown tag is `None` |
| Clock | two sleepers wake in time order after `advance` | none |
| Filesystem | `fail_at(Rename, 1, ..)` makes `write_atomic` fail with the old file readable | none |
| Runner | a scripted `docker` reply is returned and recorded in `seen()` | an unscripted spec errors naming it; `Program::Git` runs for real and is not recorded as scripted |

---

## 8. jackin (binary)

Role: clap commands, wiring, the CLI terminal hand-off, and human and JSON output. Thin: no launch logic, no config rules. The package has `src/lib.rs` with `run` and `src/main.rs` that calls it with system dependencies, so tests drive the exact entry point with fakes (D14 names the binary; the lib target is an implementation detail of the same package).

### Module map

```text
crates/jackin/src/
  main.rs           fn main: builds the system dependencies, runs `run`, returns ExitCode::from(class.code())
  lib.rs            run
  app.rs            AppEnv, Deps, GatewayConnector, ConnectError; pub(crate) App
  cli.rs            Cli, Command, argument structs (clap derive, help in attributes)
  commands/{account,attach,build,doctor,export,gateway,inspect,load,ls,power,progress,purge,role,shell,workspace}.rs
  error.rs          CommandError, ExitClass
  output.rs         human and JSON writers (no print_stdout)
  paths.rs          pub(crate) absolutize: resolve `.` and `..` against the working directory
  report.rs         the stderr form of an error: the message and its cause chain
  system.rs         Locations, locations, launch_limits, GrpcConnector, SystemTerminal
  term.rs           CLI terminal hand-off: raw mode, resize, pump
  terminal.rs       Terminal, TerminalInput, RawMode
  views.rs          GatewayLs, AccountLs, RoleLs, WorkspaceLs and their row types (VIEW_SCHEMA)
```

### Public surface

```rust
pub struct Deps {
    pub host: HostPorts,
    pub engine: Arc<dyn ImageEngine>,
    pub connect: Arc<dyn GatewayConnector>,
    pub limits: LaunchLimits,
}

#[async_trait]
pub trait GatewayConnector: Send + Sync + Debug {
    async fn connect(&self, name: &GatewayName, entry: &GatewayEntry, cancel: &CancellationToken) -> Result<Arc<dyn Gateway>, ConnectError>;
}

pub enum ConnectError {
    Discovery { gateway: GatewayName, source: DiscoveryError },
    Gateway(GatewayError),
}

impl ConnectError {
    pub fn is_cancelled(&self) -> bool;
}

pub struct AppEnv {
    pub args: Vec<OsString>,
    pub stdout: Box<dyn Write + Send>,
    pub stderr: Box<dyn Write + Send>,
    pub terminal: Option<Box<dyn Terminal>>,
    pub cwd: HostPath,
    pub config_path: HostPath,
    pub cache_dir: HostPath,
    pub env: EnvSnapshot,
    pub cancel: CancellationToken,
}

#[async_trait]
pub trait Terminal: Send + Debug {
    fn size(&self) -> TermSize;
    fn enter_raw(&mut self) -> io::Result<RawMode>;
    async fn read_line(&mut self) -> io::Result<Option<String>>;
    async fn next_input(&mut self) -> TerminalInput;
}

pub enum TerminalInput { Bytes(Vec<u8>), Resized(TermSize), Closed }
pub struct RawMode { }

impl RawMode {
    pub fn new(restore: impl Send + 'static) -> RawMode;
}

pub struct Locations {
    pub config_path: HostPath,
    pub cache_dir: HostPath,
    pub openshell_config: HostPath,
    pub engine_socket: HostPath,
}

pub async fn run(env: AppEnv, deps: Deps) -> ExitClass;
pub fn locations(env: &EnvSnapshot) -> Result<Locations, CommandError>;
pub fn launch_limits() -> LaunchLimits;

pub struct GrpcConnector { }

impl GrpcConnector {
    pub fn new(runner: Arc<dyn ProcessRunner>, fs: Arc<dyn FileSystem>, openshell_config: HostPath) -> GrpcConnector;
}

pub struct SystemTerminal { }

impl SystemTerminal {
    pub fn open() -> io::Result<Option<SystemTerminal>>;
}

impl std::fmt::Debug for AppEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result;
}
```

`run` is the only entry that tests use. It parses the arguments with `Cli::try_parse_from`, builds the config store, runs the command, and returns an `ExitClass`. `main` builds the system dependencies, runs `run`, and returns `ExitCode::from(class.code())`; it has no other logic. The library never reads the process environment, the working directory, or the home directory: `main` supplies them in `AppEnv`, and `locations(&EnvSnapshot)` derives every host location from `AppEnv.env`. The config is `$XDG_CONFIG_HOME/jackin/config.toml`, else `ConfigStore::default_path($HOME)`. The cache is `$XDG_CACHE_HOME/jackin`, else `$HOME/.cache/jackin`. The OpenShell registry is `$XDG_CONFIG_HOME/openshell`, else `$HOME/.config/openshell`. The engine socket is the `unix://` path of `DOCKER_HOST`, else `/var/run/docker.sock`, and a `DOCKER_HOST` with another scheme is ignored. `HOME` and the `XDG_*` variables must be absolute paths, and a missing `HOME` is an error. A relative path argument (a workspace path, a role path, `--chatgpt-from`, a `file:` credential, `role check`, `--engine-certs`) is resolved against `AppEnv.cwd`. `.` and `..` are resolved lexically first (`paths::absolutize`), because `HostPath::resolve` rejects a parent segment.

`AppEnv` writes `Debug` by hand because its writers are `dyn Write`; it prints the other fields and leaves the writers out. A terminal means a TTY, and `terminal` is `None` without one. `Terminal` is an owned trait. `size` returns the current size. `enter_raw` returns a guard that restores the mode when it drops. `read_line` returns `None` at the end of the input and serves the prompts. `next_input` serves attach and shell with bytes, resizes, or the close of the input. Attach and shell write their output to `AppEnv.stdout`, so the trait has no output method. `Deps.limits` carries the launch limits (`launch_limits()` builds the production value), so the library holds no timing constants. `Deps` holds no usage endpoints, and the binary has no `usage` or `console` command.

`GatewayConnector` is the construction seam of the gateway boundary: production returns `GrpcGateway::connect(resolve_gateway(..))` and tests return `FakeGateway`. The same holds for `Deps.engine` (`BuildxEngine` or `FakeEngine`). `connect` takes the cancel token of the command and returns `ConnectError`: a `DiscoveryError` (so a missing `openshell` binary is not reported as an unreachable gateway) or a `GatewayError`. `GrpcConnector` passes the token to `resolve_gateway`. A handler validates, plans, and prompts before it calls `connect`, and `connect` is lazy: it never runs for `role`, `workspace`, `gateway`, `build`, or `account add` of a provider other than Codex. `doctor` passes the connection error to `GatewayAccess::Failed`; it turns a `Discovery` error into the `GatewayError::Unreachable` that the session type needs, and the `gateway-registered` line carries the discovery detail.

### Commands (DESIGN.md CLI table)

```rust
#[derive(Parser)]
#[command(name = "jackin", about = "Run Codex in OpenShell sandboxes", version)]
pub struct Cli {
    #[arg(long, global = true, help = "Gateway name from the jackin config")]
    pub gateway: Option<GatewayName>,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(subcommand)]
    Gateway(GatewayCommand),
    Doctor,
    Build(BuildArgs),
    Load(LoadArgs),
    Attach(AttachArgs),
    Shell(SandboxArg),
    Ls(JsonArg),
    Inspect(InspectArgs),
    Export(SandboxArg),
    Stop(SandboxArg),
    Start(SandboxArg),
    Purge(PurgeArgs),
    #[command(subcommand)]
    Account(AccountCommand),
    Usage(JsonArg),
    #[command(subcommand)]
    Role(RoleCommand),
    #[command(subcommand)]
    Workspace(WorkspaceCommand),
}

#[derive(Subcommand)]
pub enum GatewayCommand {
    #[command(about = "Add a gateway entry")]
    Add(GatewayAddArgs),
    #[command(about = "List gateway entries")]
    Ls(JsonArg),
    #[command(about = "Remove a gateway entry")]
    Rm(GatewayNameArg),
}

#[derive(Args)]
pub struct GatewayAddArgs {
    #[arg(help = "Name of the gateway as registered with openshell gateway add")]
    pub name: GatewayName,
    #[arg(long, default_value = "default", help = "OpenShell workspace that jackin uses")]
    pub workspace: OpenShellWorkspace,
    #[arg(long, default_value = "default", help = "Docker buildx builder with the docker driver")]
    pub builder: BuilderName,
    #[arg(long, help = "Image platform; defaults to the host architecture")]
    pub platform: Option<Platform>,
    #[arg(long, help = "Port of the dedicated Docker engine on host.openshell.internal")]
    pub engine_port: Option<NonZeroU16>,
    #[arg(long, help = "Directory with the engine client ca.pem, cert.pem, and key.pem")]
    pub engine_certs: Option<PathBuf>,
    #[arg(long, help = "Make this the default gateway")]
    pub default: bool,
}

#[derive(Args)]
pub struct LoadArgs {
    #[arg(help = "Role name; defaults to the workspace default role")]
    pub role: Option<RoleName>,
    #[arg(default_value = ".", help = "Project directory")]
    pub path: PathBuf,
    #[arg(long, help = "Account to use for this launch")]
    pub account: Option<AccountName>,
    #[arg(long, help = "Build the image again")]
    pub rebuild: bool,
    #[arg(long, help = "Continue an earlier launch")]
    pub launch_id: Option<LaunchId>,
    #[arg(long = "env", value_name = "NAME=VALUE", help = "Value for a role env entry")]
    pub env: Vec<EnvAssignment>,
}

#[derive(Args)]
pub struct AccountAddArgs {
    #[arg(help = "Provider of the account")]
    pub provider: Provider,
    #[arg(long, help = "Account name")]
    pub name: AccountName,
    #[arg(long = "chatgpt-from", value_name = "CODEX_HOME", help = "Import a Codex ChatGPT login")]
    pub chatgpt_from: Option<PathBuf>,
    #[arg(long, value_name = "SOURCE", help = "Credential as env:NAME, an op reference, or file:PATH")]
    pub credential: Option<CredentialArg>,
}
#[derive(Subcommand)]
pub enum AccountCommand { Add(AccountAddArgs), Ls(JsonArg), Rm(AccountArg), Default(AccountArg), Bind(BindArgs) }
#[derive(Subcommand)]
pub enum RoleCommand { Add(RoleAddArgs), Ls(JsonArg), Rm(RoleArg), Check(RoleCheckArgs) }
#[derive(Subcommand)]
pub enum WorkspaceCommand { Add(WorkspaceAddArgs), Ls(JsonArg), Rm(WorkspaceArg) }

#[derive(Args)]
pub struct BuildArgs { pub role: Option<RoleName>, pub rebuild: bool, pub json: bool }
#[derive(Args)]
pub struct AttachArgs { pub sandbox: SandboxName, pub restart: bool }
#[derive(Args)]
pub struct PurgeArgs { pub sandbox: SandboxName, pub force: bool }
#[derive(Args)]
pub struct InspectArgs { pub sandbox: SandboxName, pub json: bool }
#[derive(Args)]
pub struct SandboxArg { pub sandbox: SandboxName }
#[derive(Args)]
pub struct JsonArg { pub json: bool }
#[derive(Args)]
pub struct GatewayNameArg { pub name: GatewayName }
#[derive(Args)]
pub struct AccountArg { pub account: AccountName }
#[derive(Args)]
pub struct RoleArg { pub name: RoleName }
#[derive(Args)]
pub struct WorkspaceArg { pub name: WorkspaceName }
#[derive(Args)]
pub struct RoleAddArgs { pub name: RoleName, pub source: RoleSourceArg, pub rev: Option<String> }
#[derive(Args)]
pub struct RoleCheckArgs { pub path: PathBuf }
#[derive(Args)]
pub struct WorkspaceAddArgs { pub name: WorkspaceName, pub path: PathBuf, pub docker: bool }
#[derive(Args)]
pub struct BindArgs { pub account: AccountName, pub role: Option<RoleName>, pub workspace: Option<WorkspaceName> }

pub struct EnvAssignment { pub name: EnvName, pub value: String }
pub enum CredentialArg { Env(EnvName), OnePassword(OpReference), File(PathBuf) }
pub enum RoleSourceArg { Path(PathBuf), Git(String) }
pub enum BindScope { Role(RoleName), Workspace(WorkspaceName) }

impl BindArgs {
    pub fn scope(&self) -> Result<BindScope, CommandError>;
}

pub enum CommandError {
    Argument(String),
    Config(ConfigError),
    Path(PathError),
    Fs(FsError),
    Compatibility(CompatibilityError),
    Plan(PlanError),
    Role(RoleError),
    RoleFindings(Vec<RoleError>),
    Launch(LaunchFailure),
    Attach(AttachError),
    Restart(RestartError),
    Lifecycle(LifecycleError),
    Export(ExportError),
    Purge(PurgeError),
    Account(AccountError),
    Login { home: HostPath, source: CredentialError },
    Image(ImageError),
    Connect(ConnectError),
    Entropy(EntropyError),
    Terminal(std::io::Error),
    Output(std::io::Error),
    Json(serde_json::Error),
    Console(ConsoleError),
    NoTerminal,
    Session(String),
    Cancelled(String),
    Environment(String),
    Unhealthy,
}

impl CommandError {
    pub fn class(&self) -> ExitClass;
}

pub enum ExitClass { Ok, Failure, Usage, Refused, Cancelled }

impl ExitClass {
    pub fn code(self) -> u8;
}
```

Every field of the argument structs has `#[arg(help = ...)]` text and no doc comment (D17); the block shows the attributes on `LoadArgs` only. A field of type `bool` is a `--long` flag and `Option` is optional; `name`, `sandbox`, `path`, and `account` are positional, except `--name` of `account add` and `gateway add`'s `name`, which follow the DESIGN table. `BindArgs` has a required, exclusive clap group over `--role` and `--workspace`, and `scope` turns the pair into the enum (`CommandError::Argument` when clap let neither or both through, which the group makes unreachable). `RoleSourceArg` parses an existing path as `Path` and anything else as a Git URL. `CredentialArg` parses `env:NAME`, an `op://` reference, or `file:PATH`.

`ExitClass::code` is 0 for `Ok`, 1 for `Failure`, 2 for `Usage`, 3 for `Refused`, and 130 for `Cancelled`. `class` is an exhaustive match. `Refused` is `PurgeError::WorkNotExported`, `PlanError::EnvNeeded` (a missing value without a terminal), and `RestartError::AlreadyRunning`. `Usage` is `CommandError::Argument`, and clap's own usage errors also exit 2. `Cancelled` is `CommandError::Cancelled` (a cancelled launch, end of input at a prompt, a cancel during attach or shell, a cancel during discovery or connection), `ImageError::Cancelled` (`build`), `ExportError::Cancelled` (`export`), and `RestartError::Gateway(GatewayError::Cancelled)` (`attach --restart`). The session turns every failure that happens while the token is cancelled into `LaunchOutcome::Cancelled`, so `load` needs no class for a failed launch. `doctor` exits `Cancelled` when the token is cancelled as its checks end, and prints no report. Every other error is `Failure`. `main` cancels the token on the first Ctrl-C and returns 130 at once on the second. While attach or shell holds the terminal in raw mode, Ctrl-C is a byte for the remote process and not a signal.

The stderr form of every failure is `error:` followed by the message and its cause chain, printed once. A launch failure prints the lowercase step name, the launch ID, the cause, and, when a sandbox remains, `sandbox <name> remains; retry with: jackin load --launch-id <id>`. A hook failure prints the lowercase hook name, the exit code, and the output as `TerminalText`.

### Command behavior

- `load` order: read the config, resolve the path against the working directory, find the workspace of that path (`ConfigError::NoWorkspaceForPath`, which names `jackin workspace add`), open the project, choose the gateway, load the role, plan, print the Docker warning, then connect and launch. `jackin load [<role>] [<path>]` takes the role first, so a path alone needs the role before it. With a terminal, a missing `[env]` value is asked on stderr as `<prompt or NAME> [<default>]: ` and read with `Terminal::read_line`; an empty answer takes the default and asks again when there is none, and end of input exits `Cancelled` with nothing created. Without a terminal, a missing value exits `Refused`.
- The Docker warning prints once, from the `LaunchEvent::Warning` event, as the first launch event: `warning: the Docker capability is enabled for this sandbox`, then one line per `DockerRisk`, indented by two spaces, with `DockerRisk::text()` (the wording of the approved baseline console screen). `load` prints nothing before the launch.
- `load` with a terminal attaches after a `Ready` outcome. Without a terminal it prints the sandbox name and the launch ID on one stdout line, separated by a tab, exits `Ok`, and leaves attaching to `jackin attach`. A `Cancelled` outcome exits `Cancelled` and names the sandbox and the launch ID. A launch failure exits `Failure`.
- `jackin` with no subcommand prints help on stdout, with or without a terminal.
- `attach` and `shell` need a terminal (`NoTerminal`) before any gateway call. `attach --restart` runs `Session::restart_agent` and attaches as the owner. Attach forwards every byte to `AttachedTerminal::input`, and the detach keys (Ctrl-D, Ctrl-P Ctrl-Q) are handled by the supervisor, not by the pump. The pump ends on `Ended`, on end of the terminal input (a detach), or on a cancel (a detach and exit `Cancelled`). The `RawMode` guard restores the terminal on every path. `shell` opens `Session::open_shell` and runs the same pump; a shell that ends with the stream exits `Ok`.
- `stop` and `start` note the result on stderr and print nothing on stdout. `export` prints one `<host ref>\t<tip>` line per exported head on stdout, and notes on stderr when there is nothing to export. `purge` notes `deleted sandbox <name>` or `sandbox <name> is already gone` on stderr and never touches the OpenShell provider.
- `ls` prints one tab-separated line per sandbox, which is not a stable format; `--json` is the stable one. A managed sandbox whose labels are unreadable is named in a warning on stderr and left out of the list, and the exit stays `Ok`. A listing with no sandbox prints `no sandboxes` on stderr after the warnings; stdout stays empty and `--json` is unchanged.
- `doctor` prints one line per check on stdout: the status (`pass`, `warn`, `fail`, `skip`), the check id (`openshell-binary`, `ssh-binary`, `git-binary`, `gateway-registered`, `gateway-health`, `pinned-version`, `engine-socket`), and the detail. It exits `Failure` when a check fails and `Ok` for warnings and skips. When no gateway can be chosen (none configured, several without a default, or an unknown `--gateway`), the checks that need no gateway still run and print, and the run then fails with the config error (`no gateway is configured; run jackin gateway add`).
- `account add` order: parse the credential, build the `Account`, check the add against a copy of the loaded config (a duplicate fails here), connect and call `sync_account` for a Codex account, then edit the config. A failed step leaves the config bytes unchanged. `account rm` order: check the removal against a copy of the config (references fail here), call `Session::forget_account` for a Codex login account, then edit the config. `account bind <account> --role <r>` binds the agent of the provider of the account, and a provider without an agent fails with `ConfigError::ProviderMismatch`. `--workspace` binds the provider of the account.
- `gateway add` takes `--engine-port` and `--engine-certs` as a pair (clap `requires`). `role add <name> <source>`: a source that resolves to an existing directory is a `Path` source, and anything else must parse as a `RoleUrl` and is a `Git` source. `--rev` with a `Path` source is a usage error.
- `build` prints only the final image ID on stdout, and the build lines go to stderr. `inspect` of a missing or unmanaged sandbox fails with empty stdout.

### Output and terminal hand-off

`output.rs` writes through the `AppEnv` writers (`print_stdout` is denied). Stdout carries data only: JSON, help and version text, the plain `build` image ID, the `ls` names, the export refs, the doctor report, and the sandbox name and launch ID that `load` prints without a terminal. Errors, warnings, prompts, progress, and the exit and detach lines go to stderr. Clap usage errors are rendered into the `AppEnv` writers, so no clap code writes to the process streams or exits. `ls --json` writes `LsOutput`, `inspect --json` writes `InspectView`, `build --json` writes `BuildOutput`, and `gateway ls`, `account ls`, `role ls`, and `workspace ls` with `--json` write `GatewayLs`, `AccountLs`, `RoleLs`, and `WorkspaceLs`. Every view carries `schema: 1` (`VIEW_SCHEMA`), derives no `JsonSchema`, prints as one compact line with a trailing newline, and has a key set that tests pin. The lists are in name order.

- `gateway ls`: `{schema, gateways: [{name, default, workspace, builder, platform, engine}]}`. `platform` is the Docker string (`linux/arm64`), and `engine` is `null` or `{port, certs}`.
- `account ls`: `{schema, accounts: [{name, provider, credential, default}]}`. `provider` is the lowercase provider. `credential` is only the kind (`env`, `op`, `file`, or `codex-login`); no variable name, reference, path, or token appears. `default` is true for the default account of its provider.
- `role ls`: `{schema, roles: [{name, source, bindings}]}`. `source` is `{kind: "path", path}` or `{kind: "git", url, rev}` with `rev` null when absent. `bindings` maps the agent name to the account name.
- `workspace ls`: `{schema, workspaces: [{name, path, docker, default_role, bindings}]}`. `bindings` maps the provider name to the account name.

`term.rs` implements the CLI hand-off for `attach` and `shell`: enter raw mode, pump input bytes to `TerminalSession::input`, resizes to `resize`, `Output` bytes to stdout, restore the terminal on every exit path, and print the exit or detach line. It forwards Ctrl-D and Ctrl-P Ctrl-Q unchanged because the supervisor detaches on them (D09), and it sends every input chunk through `AttachedTerminal::input`, so the reply filter applies. It never calls `openshell` (D03).

### Test plan outline (mark for `test-reviewer`; P6.1)

Location: `crates/jackin/tests/*.rs`. Boundaries faked: the gateway through `GatewayConnector` returning `FakeGateway`, the image engine, the clock, the filesystem, the network for usage, and the host programs `docker`, `openshell`, `ssh`, and `op`. Tests call `run(AppEnv, Deps)` with captured writers and assert the exit class, stdout, stderr, the stored config, and the fake gateway state.

| Behavior | Positive | Negative |
|---|---|---|
| `build` (J03) | prints the final image ID, equal to the ID that `FakeEngine` holds for the tag; `--json` parses into `BuildOutput`; a second `build` prints the same ID and runs no new engine build | a role whose Dockerfile lacks `FROM construct` exits `Failure` and stderr names the line |
| `load` end to end (J03) | exit `Ok`, one sandbox with the labels, config unchanged | `load` in a directory with no workspace exits `Failure` and stderr names `jackin workspace add` (decision 8); a non-TTY run with a missing env value exits `Refused` naming the variable |
| Gateway choice (decision 7) | `gateway add` stores the entry; `--gateway` overrides `default_gateway`; one configured gateway is used without a default | two gateways and no default exits `Failure` naming both; `gateway rm` of the default clears it |
| `ls --json` and `inspect --json` (J03) | stdout parses into `LsOutput` and `InspectView` and matches the fake state | `inspect` of a missing sandbox exits `Failure` with empty stdout |
| `purge` (J14) | an exported sandbox: exit `Ok`, sandbox gone | unexported: exit `Refused`, sandbox still there; `--force` deletes |
| `stop` and `start` (fix 4) | a `Ready` sandbox stops and starts | a sandbox in `Error` exits `Failure` and stderr names the phase and exit code |
| `role check` (J02, fix 6) | a valid role exits `Ok` with no gateway configured | an unknown manifest field exits `Failure` and stderr names the field; a policy rule for a Codex host is rejected naming the rule; a policy without `landlock.compatibility: hard_requirement` or with no filesystem path fails naming the setting; a missing role directory, a file in its place, and a directory without `jackin.role.toml` each fail naming the path |
| `doctor` (J01, fix 6) | with the fake gateway healthy, all checks pass | with the connector failing, exit is `Failure` and the binary checks still print |
| accounts (J17, decision 5) | `account add`, `default`, `bind` produce the stored config; `account add codex --chatgpt-from` creates the OpenShell provider; `account rm` of a bound account refuses | an unknown provider exits `Usage`; a Codex account with `--credential env:X` exits `Failure` with `Unsupported` |
| `attach --restart` | restarts after `exited`; prints the exit code line when not restarting | on `running` exits `Refused` and writes no start request |
| No terminal | `jackin` with or without a TTY prints help on stdout and exits `Ok` | `attach` with no TTY exits `Failure` with `NoTerminal` |

---

## 9. jackin-xtask

Role: repository gates and the live journey runner. Package `jackin-xtask`, alias `cargo xtask`. A binary plus a lib target, with pure check modules, so each gate step is tested on fixture repositories and also run on the repository. It depends on three product crates: `jackin-core` (typed views), `jackin-openshell` (a `Gateway` to run the sentinel probe and read files, decision 12) and `jackin-session` (`ProcessGroup`, the group stop that `RealHost` and `PortableLink` share with the host process runner). The `layers` row allows those three and the dev-dependency `jackin-testkit`, and no other product crate. A compile error in one of them stops `xtask ci` from building. It also depends on `portable-pty`.

### Module map

```text
crates/xtask/src/
  main.rs         clap entry
  lib.rs          re-exports
  ci.rs           CiArgs, CiProfile, CiStep, CiError, run_ci
  repo.rs         Repo (file listing, cargo metadata)
  violation.rs    Violation
  checks/comments.rs     lexer-based comment scan (D17)
  checks/layers.rs       LAYERS table, import rules, Secret::expose allowlist
  checks/containers.rs   forbidden container invocations (D01)
  checks/pins.rs         Rust version pins agree (U1 fix round, P3)
  checks/errors.rs       ErrorsCheck: rules for #[error] messages
  checks/errors/item.rs  private: the fields of the item that follows an #[error] attribute
  lex.rs                 Rust lexer (lex_rust)
  text.rs                private copy of the jackin-core plain helper (xtask cannot depend on jackin-core)
  e2e.rs          feature e2e (U15): Journey, JourneyStep, StepResult, JourneyReport, E2eArgs
  e2e/probe.rs    feature e2e: ProbeReport (serde, JsonSchema)
  e2e/pty.rs      feature e2e: portable-pty driver for J11 and J12
  e2e/runner.rs   feature e2e: private ProcessRunner for the journey gateway
```

### Types

```rust
#[derive(Args)]
pub struct CiArgs {
    #[arg(long, conflicts_with = "only", help = "Skip the docs build; same steps as --no-docs")]
    pub fast: bool,
    #[arg(long = "no-docs", conflicts_with = "only", help = "Skip the docs build")]
    pub no_docs: bool,
    #[arg(long, value_enum, value_delimiter = ',', help = "Run only the named steps, once each in plan order")]
    pub only: Vec<CiStep>,
}
pub enum CiProfile { Full, SkipDocs }

impl CiArgs {
    pub fn profile(&self) -> CiProfile;
    pub fn steps(&self) -> Vec<CiStep>;
}

#[derive(ValueEnum)]
pub enum CiStep { Fmt, Clippy, Nextest, Deny, Shear, Comments, Layers, Pins, Containers, Errors, Reuse, Docs }

impl CiStep {
    pub const ALL: [CiStep; 12];
    pub fn runs_in(self, profile: CiProfile) -> bool;
}

impl std::fmt::Display for CiStep { }

pub struct Violation { pub step: CiStep, pub path: Option<PathBuf>, pub line: Option<u32>, pub message: String }

pub trait Check {
    fn step(&self) -> CiStep;
    fn run(&self, repo: &Repo) -> Vec<Violation>;
}

pub fn run_ci(steps: &[CiStep], repo: &Repo) -> Result<(), CiFailure>;

pub enum CiFailure { Violations { step: CiStep, violations: Vec<Violation> }, Tool(CiError) }
pub struct Repo { }

impl Repo {
    pub fn open(path: PathBuf) -> Result<Repo, CiError>;
    pub fn root(&self) -> &Path;
    pub fn files(&self) -> &[RepoFile];
    pub fn members(&self) -> &[Member];
}

pub struct LayerRule {
    pub member: &'static str,
    pub depends: &'static [&'static str],
    pub dev_depends: &'static [&'static str],
    pub optional: &'static [&'static str],
    pub imports: &'static [&'static str],
    pub bans: &'static [&'static str],
}

pub const LAYERS: &[LayerRule];

pub enum JourneyStep { J00, J01, J02, J03, J04, J05, J06, J07, J08, J09, J10, J11, J12, J13, J14, J15, J16, J17 }

impl JourneyStep {
    pub const ALL: [JourneyStep; 18];
    pub fn id(self) -> &'static str;
}

pub enum StepResult { Pass, Fail(String), Skip(String) }

impl StepResult {
    pub fn line(&self, step: JourneyStep) -> String;
}

pub struct E2eArgs { pub gateway: GatewayName, pub role: PathBuf, pub allow_skip: Vec<JourneyStep> }
pub struct JourneyReport { }

impl JourneyReport {
    pub fn new(allow_skip: Vec<JourneyStep>) -> JourneyReport;
    pub fn push(&mut self, step: JourneyStep, result: StepResult);
    pub fn exit_code(&self) -> ExitCode;
}

pub struct Journey { }

impl Journey {
    pub async fn start(args: &E2eArgs) -> Result<Journey, E2eError>;
    pub async fn run(&mut self, step: JourneyStep) -> StepResult;
    pub async fn cleanup(&mut self);
}

pub struct ProbeReport { }
pub enum CiError {
    ToolFailed { step: CiStep, status: i32 },
    Spawn { program: String, source: std::io::Error },
    Metadata(String),
    ListFiles(String),
    Io(std::io::Error),
}
pub enum E2eError {
    NoCredential,
    JackinBinaryMissing(PathBuf),
    Gateway(GatewayError),
    Contract(ContractReport),
    Probe(String),
    Pty(String),
    Output(std::io::Error),
}
```

Every item from `JourneyStep` down to `E2eError` is behind `#[cfg(feature = "e2e")]`. `CiError` and the check types are not.

Rules.

- `CiStep::ALL` is the one ordered list of the P2.3 steps. `run_ci` runs the given steps in that order and stops at the first failing step. It runs each tool as a subprocess itself and has no seam to fake, because the tools are the repository's own and not an external boundary. `--fast` and `--no-docs` both give `CiProfile::SkipDocs`: every step but `Docs`. `--only` selects steps by name and conflicts with both flags (U1 fix round, P1). `Full` runs all twelve. `CiStep` implements `Display` with the clap names, so `xtask: errors` and the `--only` values read the same. The run tests drive the shipped binary with scripted tools, so the order, the profiles, `--only`, the stop at the first failure, and the exit codes (0, 1, and 2 for a usage error) are tested through the real entry point. A tool that cannot start is `CiError::Spawn`. Its message is `cannot start <program>`, and `main` appends the OS cause once.
- `comments` uses a Rust lexer for `.rs` and line-prefix rules for `.toml`, `.sh`, and `Dockerfile`. It allows SPDX header lines (`SPDX-FileCopyrightText:` or `SPDX-License-Identifier:` with a value), shebang lines, `# syntax=` parser directives, and files whose first line carries the Velnor generated header. A `//` inside a string literal is not a comment.
- `layers` reads `cargo metadata` and checks `LAYERS` against every workspace member. It fails for a member with no row, for a dependency or dev-dependency the row does not list, for a dependency the row lists as `optional` that is not optional in the manifest, for a banned import, and for a `Secret::expose` call outside the allowlist. It does not look for container programs: `containers` does. `LAYERS` is the table in the Shape section; the `crates/usage/` crates are rows with a `jackin-usage-*` pattern, added in U13. Adding a crate without a row fails the gate, so a new crate cannot skip the rule. One table drives the D14 order, the usage rules, and the import rules.
- `errors` lexes every `.rs` file that `Repo::files()` returns (`crates/usage/` stays skipped by `PENDING_PORT_PATHS`) and checks each `#[error(...)]` attribute: every string literal inside it and its argument tokens. It reports one violation per literal and names every offending placeholder. It refuses three things. The first is a Debug placeholder, which is a format spec that ends in `?`. The second is a placeholder or an argument (`{error}`, `{0}`, `{source:#}`, `.error`, `.0`, `.source`) that names a field marked `#[source]` or `#[from]`, or the field named `source`, of the same variant or struct. A member access such as `.inner.error` is not the field and passes. The third is a bare placeholder (`{field}` or `{0}`, with or without a spec) or an argument that is exactly `.field` between commas, for a field whose whole type is the single identifier `String`. `plain(.field)`, `plain_list(.items)`, and any other call pass, because the argument is no longer a bare field. The token reading cannot see a `String` behind a type alias or a path, or a named format argument that shadows a field. `TerminalText` is outside the third rule: `sanitize` keeps `\n` and `\t` on purpose. The step runs after `containers` and before `reuse`; `mise.toml` (`check:repository-steps`) and `hk.pkl` list it.
- `containers` is the only step that looks for container programs (D01). The subcommand rule is the `DockerSubcommand` type in `jackin-core`, so the step guards the crate boundary: it flags `Program::Docker`, `DockerSubcommand`, and the aliases, globs, and groups that reach them outside `jackin-image`, `jackin-core`, and `jackin-testkit`; any `Command::new` with a program that is not one string literal outside `COMPUTED_COMMAND_DIRS` (`crates/xtask`, `crates/jackin-session/src/host.rs`, and `crates/jackin/tests/process/main.rs`, which runs the built binary; another file of those crates is still flagged); and any literal `docker`, `podman`, or `container`. U1 fix round P4 has the exact rules.
- `Journey::start` connects a `GrpcGateway` through the private `e2e/runner.rs` runner (about 50 lines over `tokio::process`: stdin from `/dev/null`, a timeout, kill on cancel), and it stops its children through `jackin-session::ProcessGroup`. It then runs `contract::run` against the live gateway and fails with `E2eError::Contract` on a failed check. The journey runs the `jackin` binary as a subprocess, runs `/usr/local/bin/jackin-sentinel-probe` in the sandbox through `Gateway::run_program`, parses `ls --json`, `inspect --json`, and `usage --json` into the `jackin-core` and `jackin-usage` types, and compares the host observation with the probe report per step.
- J17 observes the account selection through the `jackin.dev/account` label in `ls --json` and the OpenShell provider names in `inspect --json`. The Codex accounts other than the A1 account are `CodexLogin` sources whose fixture `auth.json` holds a refresh token that does not work, written by xtask into a temporary Codex home, so they never rotate the A1 token (D10); their usage record is a typed `Unavailable` reason.
- A step without credentials returns `Skip`. There is no conversion from `Skip` to `Pass`. `JourneyReport::exit_code` is non-zero when any step is `Fail`, and also when any step is `Skip` unless `--allow-skip` names it, so a green run cannot hide a skipped credential step (D19, PLAN P8.2: every step prints `PASS`).
- `ProbeReport` is a serde type. A test generates its JSON schema and requires equality with `rewrite/probe-schema.json`, so the sentinel probe and xtask cannot drift.
- `xtask e2e` runs outside nextest and never in `ci` (D18).

### Test plan outline (mark for `test-reviewer`; P2.3, P8.1)

Location: `crates/xtask/tests/*.rs`, through the lib target. Boundary faked: the filesystem (fixture repositories in a temporary directory). The checks run for real over fixtures. The live journey is not a unit test. Tests of the journey items are `#![cfg(feature = "e2e")]` and run under `--all-features`.

| Behavior | Positive | Negative (red first) |
|---|---|---|
| Comments step | a fixture with an SPDX header, a shebang, `# syntax=`, and a `//` inside a string passes | an added `// note`, a `/* */` block, a `#` comment in a `.sh` body, and a TOML comment each give a `Violation` with path and line |
| Layers step | the real `cargo metadata` of the workspace passes; from U13 the usage rules pass | a fixture graph with `jackin-image -> jackin-openshell`, `jackin-core -> tonic`, an `openshell-sdk` import under `jackin-session`, and a member with no `LAYERS` row each fail with the rule named; `jackin-xtask -> jackin-session` fails, and so does a non-optional `jackin-xtask -> jackin-openshell`; from U13 `jackin-session -> jackin-usage` fails |
| Containers step | `Program::Docker(DockerSubcommand::BuildxBuild)` inside `jackin-image`, `jackin-core`, and `jackin-testkit` passes; the string `docker` in a log message of `jackin-session` passes | `Program::Docker`, `DockerSubcommand`, `Program as P`, and `Program::*` in `jackin-session`, a computed `Command::new` argument, `Command::new("docker")` anywhere, `podman`, and `container` each fail |
| Secret expose allowlist | `expose` in a listed file passes | `expose` in `jackin-session/src/launch.rs` fails |
| Profiles and `--only` (decision 6, U1 fix round) | a scripted run executes the eight tool calls once each in plan order; `--fast` and `--no-docs` drop only the docs build; `--only` runs the named steps once each in plan order | a failing tool stops the run with status 1; an unknown step, `--only` with `--fast`, and an unknown flag exit with status 2 before any step runs |
| Errors step | a fixture whose messages use `Display` placeholders, `plain(..)` calls, member accesses such as `.inner.error`, and non-`String` fields passes | each Debug placeholder form (`{x:?}`, `{x:#?}`, `{x:>8?}`), a `{source}` or `.0` that names a `#[source]` or `#[from]` field, and a bare `String` field in a variant, a tuple variant, or a struct gives one violation per literal at its line |
| Journey list | `JourneyStep::ALL` is J00 to J17 in order | none |
| Skip is not Pass | a `JourneyReport` with a `Skip` has a non-zero exit code unless allowed | a `Pass` line is never printed for a `Skip` |
| Probe schema | the generated schema equals the checked-in file | an extra field in `ProbeReport` fails the equality test |

---

## 10. Sequencing

Gates are the PLAN gates. G0 is `mise exec -- mbx +1.99.1 check --workspace --locked` and applies only before `xtask` exists. After P2.3 the gate is G-FAST, `mise exec -- mbx +1.99.1 xtask ci --fast`. Each unit ends with its gate green and its tests proven red then green by `test-reviewer` (AGENTS.md). Test plans in this file are approved by `test-reviewer` before any test is written. Every unit that changes the public API of an earlier unit stops and updates this file first.

| Unit | PLAN task | Content | Gate |
|---|---|---|---|
| U0 | P2.2 | Eight crates with the module maps above and every `struct`, `enum`, `trait`, and error definition whose dependencies exist today (the table below lists what waits). Bodies are not written, because `todo` and `unimplemented` are denied lints: inherent methods and free functions arrive with the unit that implements them and their tests. Workspace lints, `deny.toml`, SDK and `openshell-policy` git dependencies under `jackin-openshell` only | G0 |
| U1 | P2.3 | `xtask`: comments, layers (`LAYERS` with the import rules, the `optional` rule, and the missing-row failure), containers, `CiProfile`, `run_ci`. No product dependency. Red and green fixtures recorded | G-FAST, and a `test-reviewer` red and green run for the three steps and for the stop at the first failure |
| U2 | P3.1 | core: ids, paths, secrets, platform, provider, agent table (Codex row from `agentspec-codex.toml` plus `user_config` and `reserved_env`), labels, phase, supervisor state, `EnvFile`, boundary traits, `StdFileSystem`, `write_atomic`, `poll_until`, `EnvSnapshot` | G-FAST |
| U3 | P3.2 | core: config model with `apply`, `ConfigStore`, account selection, role manifest, Docker capability, views, `Compatibility` | G-FAST |
| U4 | P3.3 | image: Dockerfile validation, request, key, `ImagePlan`, embedded Dockerfiles and the `jackin-start` copy | G-FAST |
| U5 | P4.1 | openshell: `Gateway` trait and types, `TerminalSession::pair`, errors, discovery, the policy model over `openshell-policy`, `GrpcGateway` on the SDK and raw clients (provider material for mode B, decision 5), status mapping, CLI argv and transfer, main-process and shell terminals, the contract suite (feature `contract`), in-process tonic tests | G-FAST |
| U6 | P4.2 | testkit: `FakeGateway`, supervisor model, `MemFs`, `FakeClock`, `ScriptedProcessRunner`, `TestRunner`, `FakeEngine`; all three contract groups pass on the fake | G-FAST |
| U7 | P4.3 | image: `ensure_image`, `BuildxEngine`, progress events, cache; `evidence/P4.3-build.md` | G-FAST, live build |
| U8 | P5.1a | session: `RoleCatalog`, `LaunchRequest`, `plan_launch`, `plan_image`, `PlannedFiles`, `decide`, probe parsing | G-FAST |
| U9 | P5.1b | session: executor, events, cancel, hooks, `read_chatgpt_login`, `sync_account` (mode B) | G-FAST |
| U10 | P5.2 | session: attach with the reply filter, shell, restart, list, inspect, power, export, work state, purge, `doctor`, host implementations | G-FAST |
| U11 | P5.4 | Docker capability across core, image, and session (types exist; adds the engine file writes, the policy rule, the cache key input, the warning) | G-FAST |
| U12 | P6.1 | binary: `run`, clap, handlers (including `gateway`, without `usage`), output, terminal hand-off | G-FAST |
| U13 | P6.3 | usage crates into the workspace; `jackin-usage` facade and dispatch; the `usage` command and `Deps.usage_endpoints`; the layer rows for the usage crates; testkit `UsageServer` and `World.usage` | G-FAST with the usage crates in the workspace |
| U14 | P9.3, P9.4 | tui: model, pages, panes, identity gate, with `Pane.screen` and the pane pages on the P9.1 and P9.2 baseline blocks, and `ConsoleDeps.usage` and the usage panel; testkit `console-fixture` | G-FAST, identity gate red then green |
| U15 | P8.1 | xtask `e2e` (feature `e2e`): journey, probe schema, PTY driver, live contract preflight, J17 usage parse | live run against `jackin-local` |

What waits, and why. U0 creates only items whose dependencies exist today.

| Item | Arrives with | Reason |
|---|---|---|
| `jackin-usage` and its layer rows; testkit `UsageServer`, `World.usage`; tui `ConsoleDeps.usage`, `Model.usage`, `Msg::UsageRefreshed`, `pages/usage.rs`; binary `usage` command and `Deps.usage_endpoints`; xtask J17 usage parse | U13 (P6.3), tui parts in U14 | the usage crates are outside the workspace members until P6.3 (D25, PLAN P1.1) |
| tui `Pane.screen` (`junie_tui::terminal::TerminalState`), the pane pages, the terminal pane block, `console-fixture` | U14, after P9.1 and P9.2 | the baseline has only `runtime::Application` until P9.1 |
| xtask `e2e` items | U15 | they need `jackin-openshell` and the usage facade |

U5 comes before U6 because PLAN P4.2 depends on P4.1. The session units (U8 to U10) are built and proven against the fake. U7 follows U6 because `ensure_image` is tested over `FakeEngine`, and it also depends on P3.3, P0.3, P7.1, and P7.4. U4 and U5 are independent of each other and may run in parallel worktrees. U13 follows U12 (PLAN P6.3 depends on P6.1). U14 starts only after P9.1 and P9.2 finish in the baseline repository.

## Implementation reconciliation

### U0 (P2.2, accepted by the orchestrator)

- U0 holds only types, traits, error enums, and data constants. Parsers, `Display`, `FromStr`, and `Deserialize` impls arrive with their units, because a stub that always fails would be wrong.
- Deferred to U13 (P6.3): `ConsoleDeps.usage`, `Deps.usage_endpoints`, `CommandError::Usage`, `World.usage`, `UsageServer`.
- Deferred to U14 (P9.3): `Pane.screen`, the pane model, the `junie-tui` dependency, and `impl Application for Console`.
- Deferred to U12 (P6.1): `Cli`, `Command`, the argument structs, and `run`, because clap derive needs `FromStr` on the ids.
- Deferred to U2 (P3.1): `sandbox_layout` and `Compatibility::pinned`.
- Deferred to U1 (P2.3): the xtask `LAYERS` table, `run_ci`, and `RepoFile`. The `e2e` feature waits for U15.
- `FsOp`, `ProcessReply`, and `HttpFixture` are not defined yet. U6 (P4.2) defines them before use.
- Dependency edges in the layer table are allowed edges, not required ones. Each unit adds the edges it uses.
- `PollError<E>` carries the bound `E: std::error::Error + 'static`.
- `jackin-openshell` lists its unused SDK dependencies in `[package.metadata.cargo-shear] ignored` until U5 (P4.1) uses them. U5 removes the entry.

### U1 (P2.3)

- The pinned public surface is `Repo::open(PathBuf) -> Result<Repo, CiError>`, the unit structs `CommentsCheck`, `LayersCheck`, and `ContainersCheck` (each implements `Check`), `CiArgs::profile`, and `CiStep::runs_in`. `CiArgs::profile` takes `self`, because `CiArgs` is `Copy`. The tests use only these items, `Violation`, and `CiStep`.
- `Repo::open` canonicalizes the root, runs `cargo metadata --no-deps --offline` (`$CARGO` when set, else `cargo`), and reads the file list from `git ls-files -z --cached --others --exclude-standard`. It reads every `.rs`, `.toml`, `.sh`, `Dockerfile`, and `*.Dockerfile` file into memory, so a check is a pure function of the `Repo`. `Member` carries the name, the manifest path, and per dependency the package `name`, the `kind` (normal, dev, or build), and `optional`. A renamed dependency keeps its package name.
- `Repo::open` skips `target/`, `.git/`, git-ignored paths, files with the Velnor generated header (U1 fix round, P5; there is no `GENERATED_PATHS`), and `PENDING_PORT_PATHS` (`crates/usage/`). `crates/usage/` is not a workspace member until P6.3, and U13 deletes `PENDING_PORT_PATHS`. Every check shares these lists.
- A `Violation` path is relative to the root with forward slashes, a line is 1-based, and `step` is the step of the check. The layer step reports a manifest problem at the member's `Cargo.toml` with `line: None`, and an import problem at the source line. Every check returns its violations sorted by path (byte order), then line (`None` first), then message.
- `CiError` gains `Spawn { program, source }` for a tool that does not start and `ListFiles(String)` for a failed `git ls-files`.
- The comment step lexes `.rs` files into tokens (`lex.rs`, shared with the layer and container steps). An SPDX line comment passes only before the first token. For `.toml` and `.sh` it scans for `#` outside quotes, and a `#` inside a word (`a#b`, `$#`, `${#1}`) is not a comment. A `Dockerfile` comment is a line that starts with `#`. A shebang passes on line 1 of a `.sh` file. `# syntax=`, `# escape=`, and `# check=` pass when they form a run that starts on line 1. A shell here-document body is scanned as shell (superseded by P8: the body is skipped).
- `LayerRule` loses `bans`. `imports` lists the gateway crate families (`GATEWAY_FAMILIES`: `openshell`, `tonic`, `russh`; the fix round replaced the four exact names) that the member may name, and only `jackin-openshell` has them. A dependency is internal when its package name is `jackin` or starts with `jackin-`. A dependency of kind dev passes when `dev_depends`, `depends`, or `optional` lists it, and any other kind passes when `depends` or `optional` lists it. An `optional` entry that is not optional in the manifest fails. Gateway crates fail by package name, so a rename does not hide one, and a `use` or path that starts with a gateway crate fails in the sources of every other member.
- The `Secret::expose` allowlist, import bans beyond the four gateway crates, and the `jackin-usage-*` rows are not in U1. U13 adds the rows, and the import and `expose` rules wait for a unit that needs them.
- Superseded by the U1 fix round (P4): the argument scan is deleted. The container step scans `.rs` tokens only. The exemption for `Program::Docker` is the crate directory `crates/jackin-image/`, not only its `src`. Inside it, the step reads the leading string literals of the `args` field of the literal that holds `program: Program::Docker` and rejects a subcommand outside `buildx build`, `buildx create`, `buildx inspect`, `image inspect`, and `push`. An `args` value that is not an array of literals is not checked. `Command::new` of `docker` fails in every crate, and of `podman` or `container` in every crate. `Program` maps to its binary with `Self::Docker` inside `impl Program`.
- The tool steps run `fmt --all --check`, `clippy --workspace --all-targets --all-features --locked -- -D warnings`, `nextest run --workspace --all-features --locked`, `deny --locked check`, `shear --deny-warnings`, `reuse lint`, and in `docs/` `bun install --frozen-lockfile` and `bun run build`. Cargo subcommands run through `$CARGO`. The docs build fails today because `docs/` still describes the old crates, so G-FULL uses `--no-docs` until P12.1.
- `xtask e2e` accepts and ignores trailing arguments, prints that it is not built yet, and exits 1.

### U1 fix round (P2 review, accepted by the orchestrator)

- P1. CLI contract. `xtask ci [--fast|--no-docs] [--only STEP[,STEP...]]`. `--only` is repeatable and comma-delimited. `CiStep` implements clap `ValueEnum` with the names `fmt clippy nextest deny shear comments layers pins containers errors reuse docs`. `CiStep::ALL` is Fmt, Clippy, Nextest, Deny, Shear, Comments, Layers, Pins, Containers, Errors, Reuse, Docs. The selected steps run once each in `CiStep::ALL` order, whatever the order or duplication on the command line. An unknown or empty step name is a usage error, and so is `--only` with `--fast` or `--no-docs` (the intersection could select zero steps). A usage error exits with status 2 before any step runs. A violation or a tool failure exits 1. Success exits 0.
- P2. Tool argv and working directory stay as U1 records them: cargo subcommands through `$CARGO` (`fmt --all --check`, `clippy --workspace --all-targets --all-features --locked -- -D warnings`, `nextest run --workspace --all-features --locked`, `deny --locked check`, `shear --deny-warnings`), `reuse lint` and `bun` resolved on `PATH`, and `bun install --frozen-lockfile` and `bun run build` with the working directory `<root>/docs`.
- P3. Pins step. `PinsCheck` is a unit struct that implements `Check` with the step `CiStep::Pins`. The authority is `[workspace.package] rust-version` in the root `Cargo.toml`. It is compared with `[toolchain] channel` in `rust-toolchain.toml`, with `[tools] rust` in `mise.toml` (a string, or the `version` key of a table), and with every `+X.Y.Z` token after `mbx` in the `run` strings of `mise.toml` `[tasks.*]`. Each differing occurrence gives one violation at that file and line, and the message holds the found version and the authority version. A missing file, a missing `rust` tool, or a missing `rust-version` gives one violation at that file with `line: None`. When the authority is missing, nothing else is compared.
- P4. Container step. D01's subcommand rule is a type (`DockerSubcommand`), so the `args` scan is deleted and the step guards the crate boundary. The Docker-specific names are allowed only under `crates/jackin-image/`, `crates/jackin-core/` (the definition), and `crates/jackin-testkit/` (a fake `ProcessRunner`, D18). Elsewhere these are violations: the path `Program::Docker` whatever its qualifier, the identifier `DockerSubcommand`, `Program as X` in a `use` tree, and `use` paths `Program::Docker`, `Program::*`, and `Program::{..Docker..}`. These are not violations: plain `use ...::Program;`, `Program::Openshell`, `Git`, `Ssh`, and `Op`, `use ...::Program::{Git, Ssh}`, `ScriptedProgram::Docker`, `ProgramRun as Run`, and any other identifier. `Command::new(<anything but one string literal>)` is a violation everywhere except under `crates/xtask/`, which computes the cargo and tool programs. A literal `docker`, `podman`, or `container`, or a path whose last component is one of those, as the argument of `Command::new` is a violation in every crate, including `crates/xtask/` and `jackin-image`. The step reports one violation per offending token or per `Command::new` occurrence, at the line of the first token. A later unit that needs a computed program name (P5.2 `TokioProcessRunner`) adds its path to the allowlist and to the K6 test in the same commit.
- P5. Generated exemption. A file is exempt from the comment step when its first line contains `Generated by Velnor Actions`, at any path. There is no path rule, so `GENERATED_PATHS` is deleted. `.github/workflows/*.yml` stays an unscanned file type.
- P6. File kinds. Dockerfile rules apply to `Dockerfile`, `*.Dockerfile`, `Dockerfile.*`, `Containerfile`, and `*.Containerfile`. Shell rules apply to `*.sh` and to any other file whose first line starts with `#!`. The SPDX tags are `SPDX-FileCopyrightText:` and `SPDX-License-Identifier:`, each with a non-empty value. `Repo::open` never fails on a tracked file that is not valid UTF-8 and has no `#!` first line.
- P7. Root. `Repo::open(path)` takes the root from the `workspace_root` of `cargo metadata`, run in `path`, canonicalized. `Repo::root()` returns it. All violation paths and git paths are relative to it, so `cargo xtask ci` gives the same result from any subdirectory.
- P8. Heredocs and the backtick. The shell scan skips heredoc bodies up to their delimiter. That covers `<<-` with tab-indented terminators and quoted delimiters. `<<<` is not a heredoc. The rest of the line that opens a heredoc is lexed normally. A backtick starts a new word, and so do `<` and `>`.
- P9. Test-source rules. The real gates run over the xtask test files. The test source has no comments, every fixture is a string literal, and `Program::Docker`, `DockerSubcommand`, `Program as`, gateway-crate paths, and SPDX lines appear only inside string literals. The only real `Command::new` calls are `Command::new("git")` and `Command::new(env!("CARGO_BIN_EXE_jackin-xtask"))`. A fixture line with a bare SPDX tag with an empty or other value is built with `[..].concat()`, because `reuse lint` reads tags inside string literals. Fixture trees live in the OS temp dir. Tool scripts, logs, and captured stderr live in a second temporary directory outside the fixture.
- `GATEWAY_CRATES` is a list of families: the exact names `openshell-sdk`, `openshell-policy`, `tonic`, and `russh`, plus the prefix families `tonic-*`, `russh-*`, and `openshell-*`. The import scan matches the Rust identifier of the family (`tonic_prost`), not a substring (`tonicity`, `openshellfish`).
- The run tests drive the shipped `jackin-xtask` binary (`CARGO_BIN_EXE_jackin-xtask`) with scripted `cargo`, `reuse`, and `bun` on `PATH`, because `unsafe_code = forbid` makes `std::env::set_var` unusable in a test. The data-only profile tests are deleted.
- Deferred, not covered by a decision: `use std::process::Command as C; C::new("docker")`, `type P = Program;`, and `let f = Command::new;` evade the container step.

### U2 (P3.1)

- R1. `SupervisorState::parse` accepts the text with exactly one trailing `\n`, because `jackin-start` publishes with `printf '%s\n'`. `render` emits no newline. Any other leading or trailing whitespace is an error.
- R2. `AccountName` rejects `--` (reason `BadFormat`), so `ProviderName::for_account` is infallible and always yields a valid `ProviderName`.
- R3. `CredentialKeys` gains `id_token`, because `codex-provider.yaml` declares `CODEX_AUTH_ID_TOKEN` beside the other three credential variables. `EnvName::is_reserved` covers `access_token`, `refresh_token`, `account_id`, and `id_token` of every `Agent`, plus every `reserved_env` name. `reserved_env` stays `["CODEX_HOME"]`.
- R4. `NameSuffix::for_launch` hashes the launch ID bytes with SHA-256, reads the first 8 digest bytes as a big-endian `u64` `n`, and writes `n mod 36^4` as exactly 4 base-36 digits (`0-9a-z`, most significant first, zero padded).
- R5. `poll_until` bounds time by summing the durations it passes to `Clock::sleep` and never calls `Clock::now`. The last sleep is clamped to the time left, a zero interval counts as 1 ms, and the probe runs once more after the last sleep. `Timestamp` keeps no unit in U2. U5 R8 adds the probe bound: each probe is raced against the remaining time and against cancel.
- R6. `write_atomic` writes `.<file name>.tmp` in the target directory, removes a leftover temporary file first, removes its own temporary file on any error (best effort), and returns the original error. Callers serialize concurrent writers with `FileSystem::lock`. `FileSystem::remove_file` of a missing file is `Ok`.
- R7. `LabelKey` and `LabelValue` implement `Display`. The negative `from_labels` cases, `is_managed()` false, and `LabelSelector::render` wait for U5, which adds a `Labels` constructor when the adapter needs it.
- R8. The `ws` result of `WorkspaceName::slug` is unreachable, because a valid name starts with an alphanumeric. It stays as the internal fallback of the infallible constructor, and no public path reaches it.
- R9. A wrong length on an exact-length id (`NameSuffix`, `Sha256`, `CommitId`, `ImageId`, `ImageTag`) is `BadFormat`. `TooLong` is for max-length grammars only. An empty `OpReference` is `BadFormat`, because its format rule covers it; every other empty id is `Empty`.
- `ImageId::full` returns the 64 hex digits without the `sha256:` prefix, because `ImageId` wraps a `Sha256`. `Display` and `parse` use the `sha256:` prefix.
- The `AgentSpec` methods `install_target`, `profile_for`, and `profiles` take `&'static self`, so their `&'static` results borrow from the static row.
- `LaunchId::generate` reads 26 bytes from the OS CSPRNG through `getrandom` and maps them with `LaunchId::from_entropy`. It returns `Result<LaunchId, EntropyError>`; see U5 R10.
- `sandbox_layout::DOCKER_DIR` is `/sandbox/.jackin/docker` (D24). `BUNDLE_DIR` is `/sandbox/.jackin/bundle`, and `bundle_path` is `<BUNDLE_DIR>/<sandbox>.bundle`.
- `Redactor::apply` replaces every match of every registered pattern with `<redacted>` and merges strictly overlapping matches into one mask. A pattern is a whole secret or one non-empty line of a secret; see U5 R7.
- `StdFileSystem` runs each blocking call in `spawn_blocking`, so `jackin-core` enables the tokio feature `rt`. `lock` creates the file if it is missing and takes an exclusive `flock` that conflicts across handles in one process.
- `jackin-core` gains the dependencies `sha2` and `toml` (`Compatibility::pinned` reads `compatibility.lock.toml` embedded with `include_str!`) and the dev-dependencies `static_assertions`, `tempfile`, `serde_json`, and tokio with `macros`. `toml` is a normal dependency, so the tests use it too.

- `Program::Docker` carries a `DockerSubcommand` (`BuildxBuild`, `BuildxCreate`, `BuildxInspect`, `ImageInspect`, `Push`), as the coordinator decided after the phase P2 review. `DockerSubcommand::argv` renders the leading argv words (`buildx build`, `buildx create`, `buildx inspect`, `image inspect`, `push`), and `ProcessSpec.args` holds only the remaining arguments. `Program::binary` maps the program to its executable inside core, so no other crate names the variant to find the binary. The D01 subcommand rule is now enforced by the type system; the xtask container step only has to guard the crate boundary (a separate fix round).

- `Provider` and `Agent` serialize as their lowercase `as_str` spelling, which is the config spelling (D26).
- `EnvFile::parse` accepts exactly the `render` output: every line ends in `\n`, and names are in strictly ascending order, so a duplicate name or an unsorted file is a `Syntax` error at the offending line.
- `SupervisorState::parse` takes an exit code only in the canonical `i32` spelling (`3`, not `+3` or `03`), so `parse(render(s)) == s` and `render(parse(t)) == t`. `StartRequest::parse` accepts one optional trailing `\n`, and `render` emits none.
- `BranchName::parse` follows `git check-ref-format --branch` and also rejects a leading `-` and the name `HEAD`. A test compares it with the real `git` on a fixed candidate list, and the work for P3.1 compared it with `git` on about 8,400 generated names.
- `AttemptId` serializes as a JSON number and deserializes through the same range check as `parse`.

### P9.2 additions (accepted by the orchestrator)

- `jackin-core` gains a role manifest writer (atomic, same parser round trip) and `jackin-session` gains a check of a staged manifest directory, for the console role editor. `jackin role check` and the staged check report every finding in one pass.
- Removing an account that a workspace or role binding names asks for confirmation with the list of bindings and then clears them (D26).

### U3 (P3.2, from rewrite/reconciliation/U3.md)


Source of acceptance: the approved test plan `P3.2-test-plan.md` (preconditions Q1 to Q13), decisions D11, D15, D17, D18, D24, D26, and the P9.2 row of `decisions.tsv`. The orchestrator folds this file into `rewrite/API.md`.

## Preconditions Q1 to Q13

- Q1. `ConfigEdit::RemoveAccountClearingBindings(AccountName)` exists. It removes the account, the default that names it, every workspace binding and every role binding that names it, and nothing else (not `default_role`). An unknown account gives `MissingReference { kind: "account" }`. `ConfigEdit::RemoveAccount` stays strict (`AccountInUse`, listing defaults, then workspace bindings, then role bindings). The API.md section 6 row "shows `AccountInUse` and leaves the config unchanged" is stale; the console follows the P9.2 row.
- Q2. `ConfigError` gains `InvalidValue { key, message }` and `MissingField { key }`. `Syntax { line, column, message }` is only for TOML syntax; `line` and `column` are 1-based and come from the offending token. A `key` is the full dotted path, and a map key is the last segment. A file that is not UTF-8 is a `Syntax` error at the line of the first bad byte.
- Q3. Kinds are exactly `gateway`, `account`, `role`, `workspace` (`KIND_*` constants, crate private). An `Add*` of an existing name gives `DuplicateName`. Any edit or reference that names an absent entry gives `MissingReference`, including `default_gateway`, `default_role`, and every `Bind*`/`Unbind*`/`Set*` edit. A key under `defaults.accounts` or `workspaces.*.bindings` that is not a `Provider::as_str()` spelling, and a key under `roles.*.bindings` that is not an `Agent::as_str()` spelling, is `UnknownField` with the full path.
- Q4. `source = { git = "<url>", rev = "<rev>" }` (`rev` optional, only with `git`) or `source = { path = "<abs>" }`; giving both or neither is `InvalidValue` at `roles.<r>.source`. Credentials are `env` (`var`), `op` (`reference`), `file` (`path`), and `codex-login` (`home`, `chatgpt_account_id`). `platform` is `linux/arm64` or `linux/amd64`. Every `home`, `path`, and `client_certs` must be absolute. Messages for path errors never quote the input. Id errors quote no input text, except that the label-grammar ids keep `InvalidChar { ch, index }` (U5 R7).
- Q5. Required: gateway `openshell_workspace`, `builder`, `platform`; engine `port` and `client_certs`; account `provider` and `credential`; role `source`; workspace `path`. Optional: everything else (`docker` defaults to false). An empty file is `Config::empty()`. `render` always writes `docker`, and omits empty `bindings`, empty `defaults`, and absent options. An empty config renders as the empty string.
- Q6. A missing file loads as `Config::empty()`. `ConfigStore::edit` creates the parent directory with `create_dir_all`. A present file that fails to parse is an error from `load` and from `edit`, and `edit` never overwrites it.
- Q7. `edit` retries a `LockHeld` on a fixed interval through `Clock::sleep` and gives up with `LockHeld` after a total bound (U5 R2). The lock is a `FileLock` guard, so every exit path releases it. The lock file is `config.toml.lock` next to the config.
- Q8. `workspace_for` matches by exact `HostPath` equality. `workspace_accounts` returns the accounts of that workspace's bindings in `Provider` order, and an empty list for an unknown workspace. `resolve_gateway(Some(x))` gives `UnknownGateway` when `x` is absent and ignores `default_gateway`. `AmbiguousGateway.names` is sorted.
- Q9. Manifest: `schema` and `[role]` with `name`, `dockerfile`, `policy` are required. `[codex]`, `[docker]`, `[hooks]`, `[env]`, and every key inside them are optional. `[env.X]` with neither key is valid. `default = ""` is `Some("")`. `schema` must be the integer 1: another integer gives `UnsupportedSchema`, a non-integer gives `InvalidValue`, absence gives `MissingField`.
- Q10. `RoleManifest::findings(text) -> Vec<ManifestError>` is empty exactly when `parse` is `Ok`. It reports every unknown field, invalid value, missing field, unsupported schema, and reserved env name in one pass, in this order: schema, then field findings in section order, then reserved env names. A TOML syntax error gives exactly one `Syntax` finding. `parse` returns the first finding.
- Q11. `RoleManifest::write(&self, fs: &dyn FileSystem, path: &HostPath) -> Result<(), ManifestError>` writes `render()` through `write_atomic` with mode 0644. `ManifestError::Fs(FsError)` is added (`#[from]`).
- Q12. `Deserialize` is added to `LsOutput`, `SandboxSummary`, `InspectView`, `PolicyStatus`, `ClosureViolationView`, `SupervisorReading`, `BuildOutput`, and to the types they hold that lacked it: `Agent` (lowercase), `PlannedLabels`, `SandboxLabels`, `Phase`, and `SupervisorState`. `JsonSchema` stays out.
- Q13. `Display` of `SelectionError::NoAccount` goes through a new `Display for MissingBinding`, which prints `Provider::as_str()`. `SeveralAccountsNoDefault` lists every candidate. `AmbiguousGateway` lists every gateway name. `ProviderMismatch`, `CredentialNotAllowed`, `FlagProviderMismatch`, `AccountInUse`, and `RoleInUse` also print the lowercase provider and the `Display` of the names, not `Debug`. `ConfigReference` implements `Display`. `NoWorkspaceForPath` prints the path and `jackin workspace add`.

## Deviations and decisions the plan did not state

- Module layout. `config.rs` keeps `Config`, the data types, and `ConfigError`; `config/{parse,render,edit,select,store}.rs` hold the rest. A private `tomlkit.rs` holds the strict table reader (`Reader`), `Finding`, `parse_table`, `quote`, and `key`, and both the config and the manifest use it. Rendering is hand written, with a TOML basic-string escaper that encodes control characters as `\uXXXX`.
- `Config`, `Account` have private fields. `Account::new` stays the only way to build an account, and it enforces `CredentialNotAllowed`.
- Parse reports the first finding in section order (gateways, defaults, accounts, roles, workspaces), then the first `CredentialNotAllowed`, then the first cross-reference error. Unknown keys inside a table are reported after that table's own findings.
- The TOML message in `Syntax` is the library's `message()` only, never its `Display`, which prints the source line and can contain a pasted secret.
- A non-UTF-8 `HostPath` renders lossy. Parsing only produces UTF-8 paths, and `HostPath::parse` accepts a non-UTF-8 `Path`. A round trip of such a path is not lossless. No U3 code path builds one.
- `SchemaVersion` does not implement `Deserialize`: the manifest parser reads the integer itself, and nothing else would call it.
- The config file is written with mode 0600 and the manifest with mode 0644.
- A `git` role source is typed as `RoleUrl` and `RoleRev` (U5 R4), which reject option-like and credential-bearing text. The session layer still passes the URL after `--` when it runs `git`.
- Two workspaces may not name the same path (`ConfigError::DuplicatePath`, U5 R3), so `workspace_for` has at most one match.
- Test plan changes: CS1 is two tests (`config_parses_the_full_documented_file_and_exposes_every_value` and `config_roles_and_workspaces_keep_their_sources_and_bindings`) to stay under the function length lint. CE2 has a second test, `account_in_use_lists_every_kind_of_reference`, that pins the `by` list. M5 has a second test, `manifest_syntax_errors_never_echo_a_pasted_secret`. `settle` in `config_store.rs` waits up to 100,000 rounds and yields to the blocking pool in each round, because 10,000 bare yields end before one `fsync` finishes. The fake `GateFs` parks after it has read the file, so a missing lock or a read before the lock loses an update deterministically.

- Test fakes: `CrashFs` (68 lines) and `GateFs` exceed the plan's 45-line cap because the `FileSystem` trait has ten methods to delegate; accepted by test-reviewer.

### U4 (P3.3, from rewrite/reconciliation/U4.md)


Source of acceptance: the approved test plan `P3.3-test-plan.md` (preconditions S1 to S9), decisions D01, D04, D05, D06, D09, D11, D17, D18, D23, D24. The orchestrator folds this file into `rewrite/API.md`.

## Preconditions S1 to S9

- S1. `ImageTag::for_image(&str, &Sha256)` is deleted. `ImageTag::for_construct(&Sha256)`, `ImageTag::for_role_stage(&RoleName, &Sha256)` and `ImageTag::for_role(&RoleName, &Sha256)` replace it. Layouts: `jackin/construct:<h>`, `jackin/role/<repo>:<h>`, `jackin/<repo>:<h>`. `<h>` is the first 16 hex digits of the key. `<repo>` is the role name ASCII-lowercased, with every maximal run of characters outside `[a-z0-9]` replaced by one `-`. `check_image_tag` requires that the text before the last `:` has at most 255 characters and that it splits on `/` into components that each match the Docker path-component grammar `[a-z0-9]+(?:(?:[._]|__|-+)[a-z0-9]+)*`. Two roles may share a repository (`Foo`, `foo`, `a.b`, `a-b`); the tag hex hashes the role name and keeps their images apart. The API.md role-stage tag `jackin-role/<role>:<16 hex>` is replaced by `jackin/role/<repo>:<h>`, because `ImageTag::parse` requires the prefix `jackin/`.
- S2. U4 owns `crates/jackin-image/images/construct/Dockerfile`, `images/derived/Dockerfile` and `images/jackin-start`, byte copies of `rewrite/drafts/{construct.Dockerfile,derived.Dockerfile,jackin-start}`. The crate embeds them with `include_bytes!`. P4.3 edits `jackin-start` later. No test pins an asset digest literal.
- S3. Each buildx option is one token `--name=value`, except the bare `--load` and `--no-cache`. Order is not pinned. `--build-context` and `--build-arg` may repeat. The args never hold the context path, `--metadata-file` or `--iidfile`. Every stage carries `--builder=<b>`, `--platform=<p>` (`Platform::docker_arg`), `--provenance=false`, `--sbom=false`, `--load`, `--progress=plain`, `--tag=<PlannedBuild.tag>`, and `--no-cache` only for `Rebuild::Yes`. The role stage adds `--file=<context>/<dockerfile path>` and `--build-context=construct=docker-image://<construct tag>`. The derived stage adds `--build-context=role=docker-image://<role tag>` and S4. The construct and derived Dockerfiles are materialized as `Dockerfile` in their contexts and get no `--file`.
- S4. The derived stage carries exactly four build args, for both platforms: `--build-arg=CODEX_VERSION=<spec.install.version>`, `CODEX_BIN_DIR=<parent directory of spec.binary>`, `CODEX_SHA256_ARM64=<LinuxArm64 target sha256>`, `CODEX_SHA256_AMD64=<LinuxAmd64 target sha256>`. `spec.binary` and `spec.code_mode_host` share a parent.
- S5. `ImageRequest.docker` feeds the key only. The derived build gets no Docker-specific arg in U4. U11 (P5.4) adds the Dockerfile stage, the args, the client pins and their tests.
- S6. `field(x) = len(x) as u64 big-endian (8 bytes) ++ bytes(x)`. The key is the lowercase hex SHA-256 of these fields in order: 1 role name; 2 `agent.as_str()`; 3 `clean` or `dirty`; 4 commit; 5 dirty digest hex, or empty when clean; 6 hex SHA-256 of the role Dockerfile bytes; 7 construct `ImageId` `Display`; 8 `spec.install.version`; 9 the platform's `InstallTarget.sha256`; 10 `platform.docker_arg()`; 11 `absent` or `client`; 12 hex SHA-256 of the embedded `jackin-start`; 13 hex SHA-256 of the embedded derived Dockerfile; 14 for `setup`, then `preflight`: `field(path or empty)` and `field(hex SHA-256 of bytes, or empty)`. The construct tag hex is the first 16 hex digits of SHA-256(`field(hex SHA-256 of the embedded construct Dockerfile)` ++ `field(platform.docker_arg())`).
- S7. `validate_dockerfile` rules. (a) Lines split on `\n`, one trailing `\r` stripped. Directive comments (`syntax`, `escape`, `check`, key case-insensitive) count only in the leading block. `escape` accepts a backslash or a backtick. Comment and blank lines are skipped, also inside a continued instruction. A line whose last non-blank character is the escape character continues on the next non-comment, non-blank line. (b) Heredoc: after an instruction whose arguments contain `<<WORD`, `<<-WORD`, `<<"WORD"` or `<<'WORD'`, the lines up to the line equal to `WORD` belong to the instruction; for `<<-`, leading tabs are ignored. (c) The keyword is case-insensitive. The form is `FROM [--flag=value]... image [AS name]`; anything else is `UnreadableFrom { line }`, in any stage. (d) The final stage is the last `FROM`. Its image token must equal `construct` byte for byte; otherwise `FinalStageNotConstruct { line, found }` with `found` the token as written. No variable expansion, no stage chains. (e) A stage named `construct` (ASCII case-insensitive), in any stage, gives the new variant `ConstructStageDefined { line }`. (f) No `FROM` gives `NoStages`. (g) `line` is the 1-based physical line of the `FROM` keyword.
- S8. `context`, `builder`, `rebuild` and `dockerfile.path` are not key inputs. `ImageKey` derivation destructures `ImageRequest` exhaustively with explicit `_` for these four.
- S9. `ImagePlan::new` decodes `request.dockerfile.bytes` as UTF-8 (`ImageError::Dockerfile(NotUtf8)`), then calls `validate_dockerfile`. The "hook file order" and "platform without an `InstallTarget`" rows leave the API.md test outline. `impl From<bool> for Rebuild` is added.

## Deviations and decisions the plan did not state

- `crates/jackin-core/tests/views.rs` also called `ImageTag::for_image` (the `BuildOutput` round trip). It now calls `ImageTag::for_role(&RoleName::parse("sentinel")?, &key)`. The test plan named only I9. The asserted JSON keys do not change.
- `REUSE.toml` gets no new annotation. The existing `path = "**"` aggregate annotation already covers the two Dockerfiles, as it covers `rewrite/drafts/*.Dockerfile`, and `reuse lint` is green. `jackin-start` keeps its own SPDX header.
- `regex` joins `[workspace.dependencies]` at `1.13.1` (already in `Cargo.lock` through another crate), `default-features = false`, features `std` and `unicode-perl`. `cargo deny` passes. `jackin-image` uses it and `sha2` only as dev-dependencies.
- New private module `src/assets.rs` holds the three `include_bytes!` constants.
- `DockerfileError` gains `ConstructStageDefined { line }` (S7 e). `ImagePlan::new` and `ImageKey::derive` still return `Result<_, ImageError>`: `MissingInstallTarget` stays in the signature as in API.md, although the Codex row has both targets.
- Parser choices S7 left open. Heredocs open only after `RUN`, `COPY` or `ADD`, as in BuildKit, and a marker is a whole shell word (`<<EOF`, `3<<-"EOT"`; `echo "<<EOF"` is not one). A directive line is `#key=value` at column 0 with the key `syntax`, `escape` or `check`; the first other line ends the block. An `escape` value other than a backtick leaves the backslash in force. A `FROM` flag must be `--name=value` with a non-empty value. Several errors at once: `UnreadableFrom` at the first bad `FROM`, then `ConstructStageDefined` (first such stage), then `FinalStageNotConstruct`.
- The Dockerfile text in `UnreadableFrom` is the joined logical instruction.
- Every materialized file has mode 0644. The `COPY --chmod=0755` lines in the derived Dockerfile set the final modes.
- `--file=` is `HostPath::as_path().join(dockerfile path)` rendered with `Path::display`.
- The checksum argument name is `CODEX_SHA256_` plus the upper-cased `Platform::target_arch()`. A local constant lists the two platforms (`Platform` has no `ALL`).
- Test 2 has two rows beyond the plan: a CRLF heredoc (the stripped `\r` is what closes it) and a directive after a comment (a directive counts only in the leading block).

## Hand-offs

- U7 (P4.3). `BuildxEngine` appends the context path, `--metadata-file` and `--iidfile` per call. Put the context after `--` or keep it absolute.
- U8 (P5.1a). `plan_image` builds `HookFiles` from the manifest. The role check and `RoleCatalog` call `validate_dockerfile`. A shared `sandbox_layout::JACKIN_START` constant would replace the literal `/usr/local/bin/jackin-start` in the derived Dockerfile and the launch plan.
- U11 (P5.4). The Docker client layer: a Dockerfile stage, the build args, the client pins and their checksums, and the tests. The key already holds the layer.

### U1 P3 fix round (xtask, from rewrite/reconciliation/P3fix-xtask.md)


The orchestrator folds this record into `rewrite/API.md` section 9 (Rules and Test plan outline). It replaces nothing in the existing rules; it adds to the `containers` and `comments` bullets.

## Rules

- `containers` command aliases. `use std::process::Command as X;` and `use tokio::process::Command as X;` make `X::new(...)` checked exactly like `Command::new(...)`: a literal `docker`, `podman`, or `container` program is flagged in every crate, and a program that is not one string literal is flagged outside `crates/xtask/`. The step reads the use trees of the file, including nested groups (`use std::{process::{Command as X}};`). `as _` is ignored. The `use` line itself is never flagged. An alias defined in another file is not tracked.
- `containers` program type aliases. `type X = Program;`, `type X = jackin_core::Program;`, and `type X = crate::Program;` are flagged outside `jackin-image`, `jackin-core`, and `jackin-testkit`, because the alias hides `X::Docker(sub)`. `type X = DockerSubcommand;` is flagged once, by the existing `DockerSubcommand` rule. `type X = ProgramRun;` and `type X = Vec<ProgramSpec>;` pass.
- `containers` function values. `Command::new` (or an alias `X::new`) that is not followed by `(` is flagged outside `crates/xtask/`, in every other crate including `jackin-image`, `jackin-core`, and `jackin-testkit`. `let f = Command::new;` and `.map(Command::new)` can hide a computed program.
- `comments` shell lexer. `$((` ... `))` is arithmetic, with nested parentheses, and is never a heredoc: `x=$((1<<2))` does not open a heredoc with delimiter `2`. A real `<<EOF` heredoc still hides its body.

## Messages

- Alias or value: the message of `Command::new` is reused for an alias call. The value message is `Command::new is used as a value; a function value can hide a computed program (D01)`.
- Type alias: `` `type X = Program` hides Program::Docker `` followed by the standard `outside jackin-image, jackin-core, and jackin-testkit; only jackin-image starts docker (D01)` suffix.

## Tests

`crates/xtask/tests/repo_gates.rs`: `containers::command_aliases_are_checked_like_command_new`, `containers::type_aliases_of_program_are_reported_outside_the_allowed_crates`, `containers::command_new_used_as_a_value_is_reported`, `comments::shell_arithmetic_shift_is_not_a_heredoc`. The look-alikes in `containers::permitted_docker_use_and_look_alikes_pass` gained `use std::process::Command as Cmd;`, `Cmd::new("git")`, `type Run = ProgramRun;`, `type P2 = Program;` (image), and `pub type Alias = Program;` (core).

## Deviation from the approved plan

T-C1 listed `C::new("docker")` in `jackin-image` as ok. The existing rule flags a literal `docker` program in every crate, including `jackin-image` (`docker_started_outside_jackin_image_is_reported`), because `Program::Docker` is the only way jackin-image starts docker. An alias is checked exactly like `Command::new`, so the alias line is flagged too. The test expects `jackin-image/src/b.rs` lines 3, 4, and 5.

## Residual risks

- A Command alias defined in another file, or by `type X = Command;`, is not tracked.
- A `use` of a non-process `Command as X` (for example `clap::Command`) is not an alias.
- Bash `(( ... ))` arithmetic commands are not covered by the `$((` fix.

### U4 P3 fix round (image, from rewrite/reconciliation/P3fix-image.md)


Source of acceptance: `P3-fix-decisions.md` (section Writer B) and `P3-fix-test-plan.md` (section Writer B and the orchestrator additions), decisions D01, D05, D06, D08, D09, D17, D18. The orchestrator folds this file into `rewrite/API.md` section "Implementation reconciliation" (U4 and later).

Lines of API.md that this file rewrites: U4 S2, U4 S3, U4 S6 (fields 8 and 9), the U4 parser note "A directive line is `#key=value` at column 0", and the section 2 types `PlannedBuild`, `ContextSource`, `ImageRequest`, `EngineError` messages.

## B-R1. Directive and leading-whitespace rule

Source: BuildKit `dockerfile/1.28.0` (GitHub release of `moby/buildkit`, published 2026-10-07), files `frontend/dockerfile/parser/parser.go` and `frontend/dockerfile/parser/directives.go`, fetched from `raw.githubusercontent.com/moby/buildkit/dockerfile/1.28.0/...` on 2026-11-02.

- `Parse` calls `discardBOM` on the first physical line only. A U+FEFF anywhere else is data.
- `processLine(stripLeftWhitespace=true)` calls `trimLeadingWhitespace`, which is `bytes.TrimLeftFunc(src, unicode.IsSpace)`. It runs on the first physical line of every instruction and before the directive test. `unicode.IsSpace` is the Unicode White_Space property, so Rust `char::is_whitespace` is the same set (it includes U+00A0 and U+0085). Continuation lines keep their leading whitespace.
- `isComment` and `isEmptyContinuationLine` also use `trimLeadingWhitespace`. A line of Unicode whitespace is blank, and an indented `#` line is a comment.
- `DirectiveParser.ParseLine` removes `#`, then trims leading Unicode whitespace, then matches `^([a-zA-Z][a-zA-Z0-9]*)\s*=\s*(.+?)\s*$`. A non-matching line, or a key other than `syntax`, `escape`, `check`, ends the directive block for good. A blank first line therefore ends the block.
- Implementation: `crates/jackin-image/src/dockerfile.rs` `trim_leading`, used by `directive`, `skippable` and the first physical line of `Reader::read`; the BOM is stripped from line 1 in `Reader::new`.
- Beyond the plan: the same Unicode trim applies to the first physical line of an instruction, so `U+00A0 FROM ubuntu` is a `FROM` (BuildKit strips it; the old parser saw the word `U+00A0FROM` and skipped it, which let a final non-construct stage through). One extra row in T-B1 covers it.

## B-R2. Key encoding (replaces S6 fields 8 and 9)

`field(x) = len(x) as u64 big-endian (8 bytes) ++ bytes(x)`. The key is the lowercase hex SHA-256 of these fields in order:

1. role name
2. `agent.as_str()`
3. `clean` or `dirty`
4. commit
5. dirty digest hex, or empty when clean
6. hex SHA-256 of the role Dockerfile bytes
7. construct `ImageId` `Display`
8. the decimal count of derived build-arg tokens (`4` today)
9. one `field(token)` per `--build-arg=NAME=VALUE` token of the derived stage, in plan order: `--build-arg=CODEX_VERSION=<version>`, `--build-arg=CODEX_BIN_DIR=<dir>`, `--build-arg=CODEX_SHA256_ARM64=<sha>`, `--build-arg=CODEX_SHA256_AMD64=<sha>`
10. `platform.docker_arg()`
11. `absent` or `client`
12. hex SHA-256 of the embedded `jackin-start`
13. hex SHA-256 of the embedded derived Dockerfile
14. for `setup`, then `preflight`: `field(path or empty)` and `field(hex SHA-256 of bytes, or empty)`

The token is the whole string including `--build-arg=`. `ImagePlan` and `ImageKey::derive` take the list from one function (`key::derived_arguments`), so a build arg that P5.4 adds is hashed without another edit. `ImageRequest.context_files` is a new unused-by-key input (the destructure names it with `_`). `ImagePlan` no longer stores the `InstallTarget`; `derived_arguments` fails with `MissingInstallTarget` for either platform without a target.

## B-R3. Context and stdin plan data

- `ImageRequest.context: HostPath` stays the root. `ImageRequest.context_files: Vec<RepoPath>` is the output of `git ls-files -co --exclude-standard` in the role root (the D08 upload rule), supplied by the session layer (P5).
- `ContextSource::RoleRepository(HostPath)` is replaced by `ContextSource::Listed { root: HostPath, files: Vec<RepoPath> }`. `ImagePlan::final_builds` sorts and deduplicates `files`.
- `PlannedBuild` gains `stdin: Option<Vec<u8>>`. The role stage has `Some(request.dockerfile.bytes)`, the exact bytes that were validated and hashed. The construct and derived stages have `None`.
- S3 changes: the role stage carries the single token `--file=-` and no path (it replaces `--file=<context>/<dockerfile path>`). Nothing in any stage's args names the context path or the Dockerfile path.
- Engine contract for P4.3: `build` must copy exactly `files` (no symlink that leaves `root`) into a fresh directory under `work_dir`, run `docker buildx build ... --file=- <context dir>` with `stdin` as the process stdin, and not read the Dockerfile from disk.

## B-R4. `--pull`

`docker buildx build --help` (buildx v0.37.2, Docker 29.8.2, run 2026-11-02): `--pull    Always attempt to pull all referenced images`. It is a boolean flag. The buildx source (`build/opt.go` at tag `v0.37.2`, lines 580 to 585): `--pull` sets `image-resolve-mode=pull` (force pull); without it the docker driver sets `image-resolve-mode=local` preference (use the local image, pull only when it is absent). So `--pull=false` is the default and changes nothing, and no buildx flag stops a pull of a missing `docker-image://` named context. Decision: no `--pull` flag is added, and `FLAG_PREFIXES` in test 10 stays closed, so any `--pull=` fails the test. P4.3 proves live that a role build with an absent construct tag fails without registry contact (or records the exact behavior). The statement about the missing-image pull is from the buildx source and BuildKit's resolve-mode names; it is not run.

## B-R5. Syntax pin

Both Dockerfiles carry line 1:

`# syntax=docker/dockerfile:1.28.0@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b`

Source and date, 2026-11-02:
- `docker buildx imagetools inspect docker/dockerfile:1.28.0` printed `MediaType: application/vnd.oci.image.index.v1+json` and `Digest: sha256:bb22d981...e5b`. The same digest is in the Docker Hub tag list (`hub.docker.com/v2/repositories/docker/dockerfile/tags`, tag `1.28.0`, pushed 2026-10-07T23:50Z).
- `1.28.0` is the newest tag without `-labs` or `-rc`. The GitHub release `dockerfile/1.28.0` is not a pre-release.
- The moving tags `1` and `latest` still pointed at `1.27.1` (`sha256:4edf897a3ffa55b89f906fc8cc78afdb3f1834cc9c7083565e611a8a7d5fe99e`, pushed 2026-09-30) on this date. The pin follows the newest stable tag, not the moving tag. If P4.3 finds that the Docker 29.8.2 engine cannot run the 1.28.0 frontend, the fallback is the `1.27.1` tag with that digest; both lines must change together (T-B5 enforces equal lines).
- Parser directive lines are the only comment allowed in these files (AGENTS.md Code rules).

## B-R6. Construct

- The apt list gains `libcap2-bin` (it holds `getcap`; the NVIDIA base does not).
- The last root `RUN` before `USER sandbox` keeps the forbidden-tool loop and the setuid strip (`find / -xdev -type f -perm /6000 -print -exec chmod a-s {} +`, then a `test -z` re-scan) and ends with `command -v getcap >/dev/null && test -z "$(getcap -r / 2>/dev/null)"`. Any file capability, or a missing `getcap`, fails the build. `getcap` stderr is silenced; the `command -v` guard keeps a missing binary from passing as an empty result. The check has not run in a build (no image build in this task); P4.3 or P7.1 proves it live, including its run time over `/`.

## B-R7. Derived re-assertion

After the last `COPY` and before the final `USER sandbox`, still as root, one `RUN`:

```text
chown root:root /usr/local/bin /usr/local/lib/jackin /usr/local/lib/jackin/hooks
chmod 0755 the same three paths
for tool in sudo doas su-exec gosu docker podman iptables nft: command -v hit -> exit 1
test -z "$(find / -xdev -type f -perm /6000 -print)"
```

The scan fails the build; it does not strip. Residual, for the orchestrator: P5.4 installs the Docker client in the derived stage when the capability is enabled, so P5.4 must place its stage after this scan or make the tool list depend on `DockerLayer`. The derived stage has no `getcap` scan (file capabilities are inert under NoNewPrivs, per P0.5).

## B-R8. Drafts deletion and references

Deleted with `git rm`: `rewrite/drafts/construct.Dockerfile`, `rewrite/drafts/derived.Dockerfile`, `rewrite/drafts/jackin-start`, `rewrite/drafts/codex-provider.yaml`. Before deletion each was byte-identical to its crate copy (`crates/jackin-image/images/...` and `crates/jackin-core/src/assets/codex-provider.yaml`).

Fixed here: `rewrite/drafts/agentspec-codex.toml` line 34 (`profile_file` now `crates/jackin-core/src/assets/codex-provider.yaml`); a one-line pointer bullet after the `Task:` line of `rewrite/evidence/P0.3-codex.md`, `P7.1-image.md`, `P7.4-supervisor.md` (the history text is unchanged); `rewrite/README.md` line 17 (the `drafts/` row).

Left for the orchestrator (orchestrator-owned files):
- `rewrite/PLAN.md` line 162 (P0.3 scope names `drafts/construct.Dockerfile`, `derived.Dockerfile`, `codex-provider.yaml`), line 430 (P7.1 scope), line 431 (steps: "from the P0.3 drafts ... P4.3 moves them into `jackin-image`" is now past tense), line 456 (P7.4 scope). Line 289 names `agentspec-codex.toml`, which still exists; no change needed.
- `rewrite/API.md` line 319 (finding 1: "The P7.4 draft (`drafts/jackin-start`)": the script is now `crates/jackin-image/images/jackin-start`), line 3106 (U4 S2: "byte copies of `rewrite/drafts/{...}`" is replaced by "the crate files are the single source"), line 3034 (R3 names `codex-provider.yaml` without a path; no path change needed).
- `rewrite/DESIGN.md` has no path reference to the deleted files. D05 step 3 and the image paragraph may need two sentences: the role Dockerfile reaches `docker buildx build` on stdin from the validated bytes (`--file=-`), and the role context is the `git ls-files -co --exclude-standard` set; also the key now hashes the derived build-arg list (B-R2).
- `rewrite/evidence/P4.1-adapter-live.md` has no reference to the deleted files (checked with `grep`).
- `rewrite/decisions.tsv` mentions the draft names only as history.

## Other decisions the plan did not state

- `jackin-start` read line is `IFS= read -r attempt`. T-B7 found that `read -r` strips leading and trailing blanks, so `1 ` and ` 1` were accepted by the shell and rejected by `AttemptId::parse`. The script now rejects them (checked with `/bin/dash`, the image shell, and macOS `sh`).
- `BuildStage` implements `Display` (`construct`, `role`, `derived`). `EngineError` messages use Display for `BuilderName`, `ImageTag`, `BuildStage`, and `TerminalText::as_str()` (the tail starts on a new line). `ProcessStatus` implements `Display` in `jackin-core` (`exit code N` or `signal N`), and the engine messages use it.
- T-B6 and T-B7 use `std::env::temp_dir()` plus a `Drop` guard instead of the `tempfile` crate, so no dev-dependency is added. They spawn `Command::new("sh")` with `spawn` plus `wait_with_output` and `LC_ALL=C`.
- T-B3 and T-B4 read the Dockerfiles as text. The behavior itself (hostile role with `sudo`, a setuid binary, a file capability) is a P4.3 or P7.1 live hand-off.

- The image drafts (construct.Dockerfile, derived.Dockerfile, jackin-start, codex-provider.yaml) were deleted from rewrite/drafts in the P3 fix round. The single sources are crates/jackin-image/images/ and crates/jackin-core/src/assets/codex-provider.yaml. Older task scopes in PLAN.md that name rewrite/drafts files are historical.


### U5 (P3 fix round)

Source of acceptance: the approved test addendum `P3-fix-test-plan.md` (Writer A), the decisions D10, D11, D15, D17, D18, D26, and the orchestrator additions. Each precondition is recorded before the test that uses it.

- R1. Process boundary types. `pub enum EnvValue { Plain(String), Secret(Secret) }`; `ProcessSpec.env` is `Vec<(String, EnvValue)>`; arguments stay plain strings, because a secret never goes in argv. `ProcessSpec` gains `env_remove_prefixes: Vec<String>` (plain data, empty by default in callers): the runner removes every inherited variable whose name starts with any listed prefix, then applies `env`. The P4.1 fix round uses prefix `OPENSHELL_` for the `openshell` transfer and gateway-list calls, and the P5 `TokioProcessRunner` tests prove the runner honours it. `Debug` is hand written where a value could leak: `EnvSnapshot` prints the variable names only (even non-secret values stay hidden); `ProcessOutput` prints the status and the byte lengths of stdout and stderr; `OutputLine` prints the stream and the text length. `EnvValue::Plain` prints, `EnvValue::Secret` redacts. `ProcessRunner` gains the required method `run_secret(&self, spec, cancel) -> Result<SecretOutput, ProcessError>` for `op read` and similar calls. `SecretOutput { status, stdout: SecretBytes, stderr: Vec<u8> }` has no `Clone`, `Serialize` or `Display`; its `Debug` redacts stdout and prints stderr as a length. A runner must not keep a plain copy of the stdout of `run_secret`; the `TokioProcessRunner` tests in `jackin-session` (P4/P5) must run a real child that prints a known value through `run_secret` and check the `Debug` output.
- R2. `ConfigStore::new(fs: Arc<dyn FileSystem>, clock: Arc<dyn Clock>, path: HostPath)`. The retry interval is 20 ms and the total wait is 10 s. `edit` attempts the lock; on `LockHeld` it calls `clock.sleep(20 ms)` and retries. It does not sleep before the first attempt or after a success. Once the summed sleeps reach 10 s (500 sleeps), the next `LockHeld` is returned as `ConfigError::Fs(FsError::LockHeld(<lock path>))`. The lock file is `<config file name>.lock`.
- R3. `ConfigError::DuplicatePath { path: HostPath, workspaces: Vec<WorkspaceName> }`. `workspaces` is sorted ascending and holds every colliding name (for `add_workspace`, the owners plus the new name). `Config::parse` raises it after all other reference checks, for the first colliding path in `HostPath` order; `add_workspace` raises it after the name and reference checks. The comparison uses normalized `HostPath`, so `/p/x`, `/p/x/`, `/p/./x`, and `/p//x` collide.
- R4. `RoleUrl` and `RoleRev` are `FromStr<Err = IdError>`, `Display`, serde as a string, with `as_str`. `RoleSource::Git { url: RoleUrl, rev: Option<RoleRev> }`. Config error keys are `roles.<r>.source.git` and `roles.<r>.source.rev`. Errors never echo the input: only `Empty` and `BadFormat` occur. `RoleUrl` accepts `https://host[:port]/...` with no userinfo at all (a token as the user name is a real pattern), `ssh://[user@]host[:port]/...` with a user that has no password, and scp style `user@host:path` with a non-empty user and path. It rejects: an empty text; a leading `-`; a host or user starting with `-` (CVE-2017-1000117); an empty host; `ext::`, `fd::`, `file:` and every other scheme; a local path or a bare name; `host:path` without a user; a password in userinfo; and any `char::is_whitespace` or `char::is_control` character. An IPv6 host in brackets is rejected. `RoleRev` accepts letters, digits, `.`, `_`, `-` and `/`, and rejects an empty text, a leading `-`, and `..`. The session layer still passes the URL after `--`.
- R5. Manifest `env.X.default` and `env.X.prompt` reject `\n` and `\0` at parse, with the keys `env.X.default` and `env.X.prompt`, exactly the values `EnvFile::new` rejects. `\r` and tab stay valid.
- R6. `OpenShellWorkspace` and `SandboxName` use the rule `DNS_19` with `single_hyphen: true`, because OpenShell 0.1.6 rejects `--`. The duplicate `--` branch in `check_sandbox_name` is deleted. The error class stays `BadFormat`.
- R7. Redactor. A secret registers itself and each non-empty line from `str::lines` (CRLF is stripped), and empty lines register nothing. `apply` finds every occurrence of every pattern (also overlapping ones), merges strictly overlapping spans into one mask, and keeps adjacent spans as two masks. Callers redact whole lines; a secret split across two calls, and encoded forms (base64, URL encoding, JSON escapes), are not matched. A short line of a multi-line secret masks every occurrence of that line (fail closed). `TerminalText`: a newline ends an unterminated OSC, DCS, APC, PM or SOS string and is kept; the bidi controls U+202A to U+202E and U+2066 to U+2069 are removed. `IdError`: `EnvName`, `ChatgptAccountId` and `OpReference` never echo a character or an index: a bad character gives `BadFormat`. This is the floor. The label-grammar ids (`WorkspaceName`, `RoleName`, `LaunchId`, `ImageLabel`, `AccountName`, `OpenShellWorkspace`, `SandboxName`, `SandboxSlug`, `NameSuffix`, `ProviderName`, `ProfileId`, `GatewayName`, `BuilderName`, `ModelName`, `CommitId`, `Sha256`, `ImageTag`, `ImageId`, `BranchName`, `AttemptId`) keep `InvalidChar { ch, index }`, because their text is a name, not a credential. `Secret::new` and `SecretBytes::new` copy the value into an exactly sized box and zeroize the source `String` or `Vec` before it drops, so no un-zeroed copy of the value stays on the heap (`into_boxed_str` and `into_boxed_slice` are not used).
- R8. `poll_until` races each probe against `cancel.cancelled()` and against `clock.sleep(remaining)`, with a fixed priority: cancel first, then the probe, then the sleep. A probe cut by the bound gives `Timeout { after: plan.timeout }`. Elapsed accounting stays the R5 sum of the sleeps between probes. Known limits: a probe that returns on its own is not counted, and the probe that runs after the last sleep has no time left, so it gets a single poll.
- R9. `EnvName::is_reserved` is true for the prefixes `JACKIN_`, `OPENAI_`, `CODEX_`, `DOCKER_`, `BUILDX_`, `LD_`, `OPENSHELL_`, for the exact names `PATH`, `HOME`, `BASH_ENV`, `ENV`, `SSL_CERT_FILE`, `SSL_CERT_DIR`, and for the credential keys of every agent row. `EnvName::parse` still accepts reserved names; reservation applies only to the manifest `[env]` table.
- R10. `LaunchId::generate() -> Result<LaunchId, EntropyError>` reads 26 bytes with `getrandom::fill` (getrandom 0.4.3, the latest stable on 2026-11-02, added to `[workspace.dependencies]`). `EntropyError` wraps the OS error and has a `Display`. `LaunchId::from_entropy(&[u8; 26])` is pure: character i is `BASE32[entropy[i] & 31]` with `BASE32 = abcdefghijklmnopqrstuvwxyz234567`, which is uniform because 256 is a multiple of 32.
- R11. `HostPath::parse` and `HostPath::resolve` return `PathError::ParentSegment` for any `..` component and no longer fold it lexically (a symlink makes the lexical result differ from the OS result). A name such as `..b` is a normal component. `.`, `//` and a trailing `/` are still folded. Config parse maps the error to the static message "expected a path without .. segments".
- R12. `DockerCapability::environment()` stays infallible. It builds its three names with a crate-private `EnvName::from_static`, and the `docker_capability` test compares the result with names built by `EnvName::parse` and checks that each is reserved.
- R13. `write_atomic` calls `fs.stat(path)` first and returns `FsError::SymlinkTarget(path)` when the final component is a symlink (also a dangling one or one to a directory), before it creates the temporary file. A symlinked parent directory stays allowed. The check and the rename are not atomic with respect to another process; callers hold the file lock.
- R14. Core types are public to the other crates, so the gate runs `cargo check --workspace --all-targets` as well as the crate tests. Other crates needed no source change for these signatures.

### U5 (P4.1, from rewrite/reconciliation/U5.md)


This file records where U5 differs from `rewrite/API.md` section 3, and how each precondition R1 to R15 of the approved test plan was resolved. The sources are the OpenShell v0.1.6 code, `evidence/P4.1-live.md`, and the C1 spike. `API.md` is not edited here.

## Preconditions R1 to R15

- R1. `jackin-core` gets `Labels::from_pairs(impl IntoIterator<Item = (String, String)>) -> Result<Labels, LabelError>`. It uses the Kubernetes grammar that the gateway enforces (`validation.rs:772`, `:882`): key name of 63 characters at most, optional DNS prefix, value of 63 characters at most, empty value allowed. `LabelError` gains `InvalidKey` and `InvalidValue`. `LabelKey` and `LabelValue` already had `Display` (U2 R7).
- R2. `Code::Aborted` and `Code::FailedPrecondition` both map to `GatewayError::FailedPrecondition { message }`.
- R3. `put_provider` calls `GetProviderProfile` first and returns `NotFound { Missing::Profile(id) }` when it is absent. Nothing is created. Live evidence confirms that `CreateProvider` with an unknown profile is a plain `InvalidArgument` and that the provider needs `profile_workspace` set to find a workspace-scoped profile. The adapter imports the profile in the configured workspace and sets `profile_workspace` to that workspace.
- R4. Captured live (`P4.1-live.md`). `StopSandbox` on an `Error` or `Completed` sandbox returns OK with the sandbox unchanged. The adapter returns `MainProcessFailed { sandbox }` when `Stop` returns a view whose phase has an ended main process (`Phase::main_process_ended`). `StartSandbox` on either phase returns `FailedPrecondition` ("VM sandbox is already running generation ...") and leaves the sandbox in `STARTING`. The adapter maps it to plain `FailedPrecondition { message }` and never returns a `Starting` view. It adds no new error variant. A second `Stop` on the `STARTING` sandbox is also `FailedPrecondition`. API.md "Ended main process (fix 4)" and the `set_power` contract are wrong on `Start`.
- R5. `Provisioning::Failed` is derived from a status condition with `type == "Ready"` and `reason == "ProvisioningFailed"`, in any list position and whatever the phase. The live phase is `Error` with no exit code. The adapter reads the conditions from the raw `GetSandbox` reply, because the SDK `SandboxRef` has none. The message goes through `TerminalText::sanitize`.
- R6. The detail comes from these calls: `GetSandbox` (the view and the conditions), `GetSandboxConfig` (the effective policy including the generated `_provider_` rule, its `version`, and its `policy_hash`), `GetSandboxPolicyStatus` (the stored revision), and `ListSandboxProviders` (the attached providers, all pages). `PolicyReport.version` and `PolicyReport.hash` are the effective values from `GetSandboxConfig`. The stored revision hash is a different value and is not reported. `PolicyReport.loaded` is `revision.status == POLICY_STATUS_LOADED && active_version == revision.version`. A `NotFound` from `GetSandboxPolicyStatus` gives `loaded == false`.
- R7. The shell terminal and the main-process terminal follow the grammar in the plan. Details: the shell stream yields `Attached`, then `Output` events (stdout and stderr merged as `Output`, raw bytes), then `Exited(Some(code))`, then `Ended(StreamClosed)`. A stream that ends without an exit event yields `Ended(Failed(_))` and no `Exited`. A refusal on the start stream is `Err(NotReady { phase })` when the sandbox cannot exec, or the plain mapped error otherwise. The main-process task sends `Detach` on drop when the queue has room, and the shell session closes the request stream. A `Detach` command on a shell session ends it with `Ended(Detached)`.
- R8. The exec helper scripts use only POSIX `sh` and `cat chmod mv mkdir dirname touch rm test`. Sandbox paths and modes are positional arguments. File content travels on stdin only. The scripts are in `grpc/program.rs`.
- R9. `GrpcGateway::connect` needs no explicit crypto installation. The workspace `tonic` dependency enables `tls-aws-lc`, and tonic then builds its rustls config with the aws-lc provider. C1 passed in process (mTLS with a client certificate, `Health`, `GatewayHealth { version: "0.1.6", serving: true }`) on the first run. There is no plaintext branch.
- R10. `download_file` hashes the downloaded file in streaming chunks with `tokio::fs` and `sha2` on the real filesystem. `GrpcGatewayConfig` has no filesystem field. The tests use a real `TempDir`.
- R11. `ProcessError::Timeout` maps to `GatewayError::Timeout { op: Upload | Download, after }`, `Cancelled` to `Cancelled { op }`, and `Spawn` to `Process(Spawn)`. A non-zero exit maps to `Transfer { op, status, tail }`, where `tail` is the sanitized last 4096 bytes of stderr (stdout when stderr is empty). A failed digest call maps to `Transfer { op: Digest }`.
- R12. The `ChatgptLogin` fields land as follows. `CreateProvider`: `type = profile id`, `profile_workspace = workspace`, `credentials = { keys.access_token: access token, keys.account_id: account ID }`. The refresh token is never a plain credential. `ConfigureProviderRefresh`: `credential_key = keys.access_token`, `strategy = OAUTH2_REFRESH_TOKEN`, `material = { client_id, refresh_token }`, `secret_material_keys = ["refresh_token"]`. CORRECTION to API.md and the plan (test P3): the token URL is not sent. The server rejects `token_url` in the material ("refresh token endpoints must be defined by the provider profile", `provider.rs:4891`); the profile asset carries it. `CredentialKeys.token_url` is unused by the adapter.
  - Update path (new finding). `UpdateProvider` of a credential that a refresh configuration owns fails with `FailedPrecondition` (`provider.rs:369`). For an existing provider, the adapter therefore runs `DeleteProviderRefresh` (`allow_missing`), then `UpdateProvider` with the credentials, then `ConfigureProviderRefresh`. Each step is idempotent, so a retry after a failed step converges. The test server mirrors the rule. Proven live (`evidence/P4.1-adapter-live.md` (b)): the plain `UpdateProvider` fails with `FailedPrecondition`, the three-step update works, also while the provider is attached to a running sandbox, and a retry after a failure between step 1 and step 3 converges. A failure after step 2 leaves a non-refreshing token until the next `put_provider`.
  - Error strings of `put_provider` pass through a `Redactor` over the access and refresh tokens.
- R13. `InvalidArgument.field` is the first `BadRequest` violation field when present, else `""`. The tests are one gateway target (`tests/gateway`) plus `tests/policy.rs` and `tests/terminal_pair.rs`. The discovery tests (D1 to D3) moved into the gateway target (deviation below). `raw_names.rs` does not exist. The published Core check names are `contract::CORE_CHECKS`.
- R14. `delete_sandbox` and `delete_provider` return `Ok(())` for an absent object. They send `allow_missing = true`, and they also turn a `NotFound` reply into `Ok(())`, because the live gateway answers an absent object with `NotFound`.
- R15. A refused or closed port is `Unreachable { endpoint }` with the registered endpoint string. The adapter opens a plain TCP connection first, because the SDK flattens every connect failure into one `Connect` message. When the TCP connect works and the SDK connect fails, the error is `Tls` (an untrusted server certificate or a failed handshake). A server that accepts TCP and then fails for another reason is also reported as `Tls`; this is a known coarse spot.

## Corrections to API.md

- Status table (R2): `Aborted` is `FailedPrecondition`. A `FailedPrecondition` from a program or terminal open becomes `NotReady { phase }` only when `GetSandbox` shows a phase that cannot exec; otherwise it stays `FailedPrecondition`. The `MainProcessFailed` row is replaced by the R4 text.
- `ensure_profile` and the Provider-flows row: a stale resource version is `FailedPrecondition` (R2). `Unchanged` compares the stored profile with the embedded one after clearing `resource_version`, `source`, and `scope`. Proven live (`evidence/P4.1-adapter-live.md` (a)): the live server returns the stored profile in a form that is equal after the three fields are cleared, so a second call reports `Unchanged`, a changed asset reports `Updated`, and the restored asset reports `Updated` once and then `Unchanged`.
- `RuleKey::provider_rule(provider)` uses `openshell_policy::provider_rule_name`, which lowercases the name and turns every character outside `[A-Za-z0-9_]` into `_`. The key for `jackin-work-codex` is `_provider_jackin_work_codex`, not `_provider_jackin-work-codex`. The live key for `p03-codex` was `_provider_p03_codex`. Test Y2 builds the key through `RuleKey::provider_rule`.
- `sandbox_detail` also calls `GetSandboxPolicyStatus` (R6). It needs four reads, not three.
- The test plan lists `serde_yaml` as a test dependency. `serde_yaml` and `unsafe-libyaml` fail `cargo deny` (RUSTSEC-2024-0449, unmaintained). U5 uses `noyalib` instead, the YAML crate that OpenShell itself uses (`serde_yml` is an alias for it in the OpenShell workspace).

## Deviations and decisions

- Dependencies. Workspace additions: `openshell-providers` (git tag v0.1.6, to parse the profile YAML into the proto profile; the CLI does the same), `noyalib` 0.0.53 (policy YAML values), `tokio-stream` (request streams), `tonic-types` (test server details), `futures` and `rcgen` (tests). `tonic` gets the feature `tls-aws-lc`. `tokio` in this crate uses `fs io-util macros net rt sync time` (dev: `process rt-multi-thread test-util`). `sha2` is a normal dependency for the streaming digest. The `cargo-shear` ignore metadata is removed.
- `russh` finding. `russh = { default-features = false, features = ["aws-lc-rs", "flate2"] }` drops `rsa`, `pkcs1`, and `crypto-primes` from `Cargo.lock`. `ssh-key 0.7.0-rc.11` stays: russh 0.65.0 pins `ssh-key = "=0.7.0-rc.11"` unconditionally, and 0.65.0 is the latest release (crates.io, 2026-10-25). The gateway host key is Ed25519 (`openshell-supervisor-process` `ssh.rs:89`), which `ssh-key` supports without `rsa`. Proven live (`evidence/P4.1-adapter-live.md` (c)): russh with `aws-lc-rs` and `flate2` completes key exchange with the supervisor, the host key is `ssh-ed25519`, the pinned fingerprint equals the `CreateSshSessionResponse` fingerprint, and a wrong fingerprint ends with `Unknown server key`. The `deny.toml` ignore for RUSTSEC-2023-0071 matched no crate and is deleted in the P4.1 fix round.
- Module map. The status mapper is in `grpc/status.rs`, not `grpc/sandbox.rs`. `policy/yaml.rs` holds the YAML work. The CLI argv builders are in `cli.rs` and the transfer in `transfer.rs` as planned.
- `GatewayError::MainProcessFailed` is built from the returned phase of `Stop`; no RPC returns it (R4).
- Sandbox views from a label listing skip a sandbox whose name is not a valid `SandboxName` or whose labels do not parse, because such a sandbox cannot be a jackin sandbox. `find_sandboxes(Name)`, `sandbox_detail`, `set_power`, and `create_sandbox` report `Protocol` instead.
- `gateway list` entries whose name is not a valid `GatewayName` are skipped. A missing string field in an entry is `ListUnparsable`.
- `Phase::Completed` with no exit code in the reply is read as exit code 0.
- `PolicyDocument::network_rules` maps the protocols `""` and `tcp` to `EndpointProtocol::Tcp` and every L7 protocol to `Rest`. It drops binaries that are not absolute paths, and it expands `ports` into one `RuleEndpoint` each. Closure checks ignore ports and binaries (D10).
- `HostPattern::overlaps` treats `*` inside a label as one label and `**` as one or more labels. A label with a partial `*` (for example `a*`) counts as one wildcard label, which can flag more than the real matcher would.
- Main-process terminal. The lease is decided in a 250 ms window after the subsystem succeeds: an `ExtendedData` notice with "attached read-only" gives `Lease(ReadOnly)`, "cannot connect to the canonical main process" gives `Ended(Refused)`, otherwise `Lease(Owner)`. `InputMode::Viewer` is always `ReadOnly`. A later "input enabled" notice emits `Lease(Owner)`. All input is forwarded; the server ignores what it must. An empty `host_key_fingerprint` is treated as a mismatch. The lease window and the end reasons are refined by A4 below (fix round items 4 and 6).
- Shell terminal input is forwarded through a bounded request channel of 1024 chunks. A caller that writes more than that while the remote does not read can block the pump.
- The contract suite implements all three groups. The Transfer and Terminal groups, and the main-process attach itself, have no in-process test (no SSH server in the plan). The fix round grows the Core group to 13 checks and the Terminal group to 5 (A3); the in-process tests reach the attach only up to the relay steps (A1).
- The tests install a test-local `RegistryFs`, `ScriptedRunner`, exec backends, and a generated service impl (`tests/gateway/service.rs`, 86 methods, generated from the pinned proto trait). They move to `jackin-testkit` when P4.2 provides its fakes.
- Test changes against the approved plan: P3 asserts that no `token_url` is sent (R12); P3's "second put" is the three-step update path; Y2 builds the generated key through `RuleKey::provider_rule`; M1 and the contract stop/start check follow the live capture (R4); D1 to D3 live in the gateway target; B8 checks `loaded` from the revision status.

## Red and green

Every test was run red against a deliberate break of the adapter (or the contract suite) and green on the real code. See the return message of P4.1 for the per-test list.

## P4.1 fix round: preconditions A1 to A9

The fix round follows `evidence/P4.1-adapter-live.md` (findings M1 to M3, L1 to L7, I5) and the orchestrator decisions 1 to 14. Each record is written before the test that depends on it.

- A1 (items 2, 5, 9). Upstream facts, `OS@v0.1.6:crates/openshell-server/src/grpc/sandbox.rs`, re-checked at the pinned tag. `resolve_and_authorize_sandbox_name` (`:197`) answers an absent sandbox with `NotFound "sandbox not found"` (`:229`) for `ExecSandbox`, `ExecSandboxInteractive`, `CreateSshSession` and `ForwardTcp`. `ForwardTcp` (`:2783`) and `CreateSshSession` (`:3319`) answer `FailedPrecondition "sandbox is not ready"` when the relay is not reachable; the exec calls answer it through `require_ready_sandbox` (`:279`). `Unavailable "supervisor relay failed: ..."` is the initial status of `ExecSandbox` (`:2661`), `ForwardTcp` (`:2796`) and `ExecSandboxInteractive` (`:3237`). `await_relay_stream` (`:2716`) sends the relay failures as the first item of the response stream, and `ForwardTcp` uses it: a failed relay target open passes the target status through (`:2726`, here `FailedPrecondition "relay target open failed: ..."`), `Unavailable "relay channel dropped"` (`:2733`), and `DeadlineExceeded "relay open timed out"` (`:2740`).
- A2 (item 9). `GatewayError::SandboxUnreachable { sandbox: SandboxName, message: String }`, retryable. The rule in `map_status`: `Code::Unavailable`, a message that starts with `supervisor relay failed`, and a known `Missing::Sandbox` give the new variant; every other `Unavailable` stays `Unreachable { endpoint }`. The in-stream `relay channel dropped` is therefore `Unreachable`, because the supervisor session was lost, not refused by a named supervisor. `redact` leaves the new variant alone (it carries no secret).
- A3 (item 3). `Gateway::delete_profile(&ProfileId) -> Result<(), GatewayError>` sends `DeleteProviderProfile` with `allow_missing = true` in the configured workspace, and turns `NotFound` into `Ok(())`. A profile that a provider still uses, or that a source manages, stays `FailedPrecondition` (`provider.rs:3248`, `:3265`); it is never swallowed. The Core checks `profile_import_unchanged_updated` and `provider_put_and_delete` first delete the contract provider and then the contract profile, assert `Imported` on the first sync, and delete the provider and then the profile again at the end, so a crashed earlier run cannot poison a later one and a finished run leaves nothing. `CORE_CHECKS` gains `absent_sandbox_is_not_found` (13 names): `run_program`, `read_files`, `write_file`, `open_terminal(Shell)` and `open_terminal(MainProcess)` on an absent sandbox are `NotFound { Sandbox(that name) }`. `TERMINAL_CHECKS` has 5 names: `shell_exit_code`, `main_attach_detach_replay`, `main_second_attachment_read_only`, `main_viewer_read_only` and `main_read_only_takes_lease_after_owner_leaves`. The lease checks are event based: a second attachment and a `Viewer` get `Lease(ReadOnly)`; a viewer's input does not reach the process; after the owner leaves, the next input of the read-only `Owner` attachment brings `Lease(Owner)` and the input reaches the process.
- A4 (items 4 and 6). The wire does not tell a detach key from a process exit: `close_main_attachment` (`ssh.rs:1380`) and a process exit both send `eof`, `exit-status`, `close`, and both can carry status 0. The adapter therefore watches the bytes it forwards, with the three keys of `filter_main_detach_sequence` (`ssh.rs:50`): Ctrl-D (`0x04`), and Ctrl-P then Ctrl-Q (`0x10 0x11`, also split over two chunks) give a pending `Ended(Detached)`; Ctrl-C (`0x03`) in an attachment whose current lease is `ReadOnly` gives a pending `Ended(StreamClosed)` (`ssh.rs:953`). While such an end is pending, the exit status 0 that follows is dropped, and the close ends the session with the pending reason. A local `Detach` command still ends with `Ended(Detached)`. Any other exit status, and `ExitSignal`, still yields `Exited`; a real exit with code 7 is `Exited(Some(7))` then `Ended(StreamClosed)` (live e.2). A real exit with code 0 in the same instant as a detach key is not told apart; the `Phase` stays the authority for the exit code (API.md). The lease follows the "attached read-only" notice (`Lease(ReadOnly)`, whenever it arrives) and the "input enabled" notice (`Lease(Owner)`); an event is emitted only when the lease changes, so the initial decision of the 250 ms window and the early notice do not repeat. Items 4 and 6 have no in-process test (no SSH server); they stand on review and on the live Terminal group (`main_attach_detach_replay` asserts Ctrl-D gives `Ended(Detached)` without `Exited` and `Phase::Ready`; `main_viewer_read_only` asserts Ctrl-C gives `Ended(StreamClosed)` without `Exited`). API.md says a read-only Ctrl-C gives `Ended(Detached)`; the adapter reports `Ended(StreamClosed)`, because the server closes the attachment and the consumer did not ask for it.
- A5 (item 7). `write_file` writes to `<dir>/.<basename>.jackin-<16 hex digits>`, a new random name per call, and moves it over the target. If that name would exceed 255 bytes, the name is `.jackin-<16 hex digits>` in the same directory, so the move stays atomic. Concurrent writers to one target each succeed, and the last move wins. A write killed before the move leaves a unique stale file (mode 0600 under `umask 077`) instead of one shared file that the next write overwrites. This is an accepted trade, with no test.
- A6 (item 8). `jackin-core` `ProcessSpec.env_remove_prefixes` (P3 fix round, API.md U5 R1) tells the runner to remove every inherited variable that starts with a listed prefix. The three transfer calls and `gateway list` list `OPENSHELL_`. The three transfer calls also name the registered endpoint with `--gateway-endpoint <endpoint>`, which `openshell` 0.1.6 accepts (`main.rs:453`) and which wins over `OPENSHELL_GATEWAY_ENDPOINT` (`resolve_gateway`, `main.rs:79`). `OPENSHELL_GATEWAY_INSECURE` is a flag without negation (`main.rs:462`), so only the removal neutralizes it. The runner contract (P5, and P4.2 `seen()`) must honour the field.
- A7 (item 10 and the port suggestion). `ClosureViolation` is an enum: `Overlap { rule, host, provider_host }` (the old struct), `UnparsableHost { rule, host: String }` (the raw text of a host that `HostPattern::parse` rejects), and `InvalidPort { rule, port: u32 }` (a port of 0, which the YAML model accepts and the proto carries as `port: 0`; a port above 65535 is refused by `openshell-policy` as a `Syntax` error in both `parse` and the proto round trip, so it cannot reach the check). `network_rules` still returns only the readable endpoints; `closure_violations` reports every unreadable endpoint of a rule that is not in `generated`, so nothing is skipped silently.
- A8 (item 11). Upstream reports a duplicate profile import as `imported = false` with a diagnostic (`provider.rs:3715`), not as `AlreadyExists`; a duplicate provider create is the real `AlreadyExists` status (`provider.rs:266`). `ensure_profile` reads the profile, imports when it is absent, and on a rejected import (`InvalidArgument { field, message }`) reads again once: a profile that is now present continues as `Unchanged` or `Updated`; a profile that is still absent surfaces the rejection. `put_provider` creates when `GetProvider` says absent, and on `AlreadyExists { Provider }` reads again once: a provider that is now present continues with the three-step update; a provider that is still absent surfaces `AlreadyExists { Provider }`. The retry is once, never unbounded.
- A9 (item 14). The L1 check is kept, not dropped. It calls `ensure_profile(codex)` and `put_provider` with the longest provider name the types allow, `jackin-<32-character account>-codex` (45 characters), in the 19-character workspace, then creates a sandbox that lists the provider, and asserts that the stored key, `metadata.name` and the stored sandbox providers equal the name. A rewrite of the name (a prefix or a truncation) changes the stored key and fails the check.

Other fix round decisions:

- Item 1. The in-process `HostShell` fake runs only a literal `Command::new("/bin/sh")`; any other program exits 127, as a missing binary does in the real image. The Containers gate is the guard.
- Item 2. `exec_collect` (so `run_program`, `read_files`, `write_file`), `open_shell`, `create_ssh_session` and the `ForwardTcp` open map a `NotFound` to `NotFound { Sandbox(name) }`.
- Item 5. A `ForwardTcp` relay-open error that arrives as the first stream item is kept and reported with its mapped cause (`Unreachable`, `SandboxUnreachable`, `FailedPrecondition`, `Timeout`), in place of the later `Protocol "ssh: ..."`.
- Item 12 and item 13 are the three edits above and the `deny.toml` change.

### U6 (P4.2, from rewrite/reconciliation/U6.md)


This file records where U6 (`jackin-testkit`) differs from `rewrite/API.md` sections 3 and 7, and how each precondition G1 to G12 of the approved test plan (`P4.2-test-plan.md`, with the orchestrator additions after P4.1) was resolved. The sources are DESIGN D07 to D10, D17, D18, `evidence/P4.1-live.md`, `evidence/P4.1-adapter-live.md`, `evidence/P7.4-supervisor.md`, API.md "U5 (P4.1)" (A1 to A9), and the shipped `contract.rs` of `jackin-openshell`. `API.md` is not edited here. Each record was written before the test that uses it.

## Preconditions G1 to G12

- G1. Exec model. The filesystem of a sandbox is a real `TempDir`. `/sandbox` is its `sandbox` subdirectory. `run_program` knows a closed set of programs: `/bin/sh`, `/usr/bin/git`, `/usr/bin/printenv`, `/usr/bin/sha256sum`, and the paths registered with `script_program` (a scripted program wins over the closed set). `/bin/sh`, `/usr/bin/git` and `/usr/bin/printenv` are the real host programs, started with `env_clear`, the environment of the run, and `PATH=/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin`; the host environment never reaches them. `/usr/bin/sha256sum` runs in process (it does not exist on macOS) and prints `<hex>  <argument>\n`, exit 0, or exit 1 with a message for a file that does not exist. Any other program is exit code 127 with `Ok`. An argument that equals `/sandbox` or starts with `/sandbox/`, the `workdir`, and `touch_on_success` map to the backing directory; a `workdir` outside `/sandbox` is `InvalidArgument { field: "workdir" }`. The default directory is `/sandbox`. A `touch_on_success` that cannot be created makes the exit code 1, as the adapter's marker script does (`touch -- "$marker" || exit 1`). API.md "There is no shell" is deleted.
- G2. Images. `FakeGateway::add_image(&ImageTag)` registers an image the fake can run. A create with an unregistered tag succeeds with a `Provisioning` view and ends in `Error { exit_code: None }` with `Provisioning::Failed { message }` on the first observation after the poll count. `World::build` registering what `FakeEngine` built arrives with U7.
- G3. Main process. A main whose program is `/usr/local/bin/jackin-start` runs the supervisor model (G7). `/bin/sh -c "<script>"` runs a script of `;`-separated statements: `echo <words>` (output `<words>\r\n` into the replay buffer, as a PTY writes it), `sleep <n>` (the process never ends by itself), `exit [<n>]` (ends at once), and `exec cat` (decision below). A script that ends without `sleep`, `exit` or `exec cat` exits 0. Every other main, and any statement the grammar does not know, makes `create_sandbox` return `InvalidArgument { field: "command" }`. An ended script gives `Completed { exit_code: 0 }` or `Error { exit_code: Some(n) }` on the first observation after the provisioning polls. The script does not parse quotes; a `;` inside quotes is a separator.
  - Decision: `exec cat` is added to the grammar because the shipped contract checks `main_second_attachment_read_only`, `main_viewer_read_only` and `main_read_only_takes_lease_after_owner_leaves` use the main `echo contract-banner; exec cat`. A main in the `exec cat` state appends each forwarded input byte to its retained output and sends it to every attachment, as the PTY echo of a real `cat` does.
- G4. Faults. `FailNext { op }` fails the next call of `op` once. `FailAfter { op, calls }` lets the first `calls` calls of `op` succeed, fails call number `calls + 1` once, and then succeeds. A fault applies to any sandbox, is checked before everything else in the call, and a failed call changes no state. When several faults name one op, the first armed fault of that op sees the call. `ProvisionFor { polls }` sets the number of observations (`find_sandboxes` and `sandbox_detail`) of each sandbox created afterwards that return `Provisioning`; the next returns the settled phase. `create_sandbox` itself returns `Provisioning` and does not count. The latest `ProvisionFor` wins. `ProvisionFails { creates: n }` makes the next `n` creates of the fake (counted from the injection) end in `Error { exit_code: None }` with `Provisioning::Failed`. `StallProgram { program }` stalls every run of that program: it returns `Timeout { op: Program, after: run.timeout }` on the tokio timer, or `Cancelled { op: Program }` when the token fires first, and changes no state. `CorruptDownload` corrupts the next `download_file` once (decision: once, like `FailNext`). The `GatewayOp` of each trait method is the one in `error.rs`: `Health`, `Profile` (`ensure_profile`, `delete_profile`), `Provider` (`provider_state`, `put_provider`, `delete_provider`), `Create`, `Find`, `Detail`, `Power`, `Delete`, `Program`, `ReadFiles`, `WriteFile`, `Upload`, `Download`, `Terminal`. API.md "triggers on exactly the n-th call" is replaced by this text.
- G5. Power. `Stop` on `Ready` returns a `Stopped` view at once and ends every main attachment with `Ended(StreamClosed)` (decision: the plan is silent; the process is gone). `Start` on `Stopped` returns a `Starting` view and the first later observation is `Ready`; the main process starts again from the beginning (retained output and `main_input` reset). Files under `/sandbox` persist. Ended main process, per `evidence/P4.1-live.md`: `Stop` returns `Err(MainProcessFailed { sandbox })` and changes nothing; `Start` returns `Err(FailedPrecondition)` and moves the phase to `Starting` with the exit code cleared; the phase stays `Starting` for good; a second `Stop` or `Start` is `FailedPrecondition`. `Completed` behaves like `Error`. Only `delete_sandbox` leaves that state. `Stop` on `Provisioning`, `Stopped` or `Starting`, and `Start` on `Ready`, `Provisioning` or `Starting`, are `FailedPrecondition`. API.md section 7 "Phases (fix 4)" and the `set_power` text are replaced by this.
- G6. Terminals. A main-process session emits `Attached`, one `Lease`, then the retained output as one `Output` (when it is not empty), then live output. The first `Owner` gets `Lease(Owner)`. A second attachment, and every `Viewer`, gets `Lease(ReadOnly)` and its input is dropped. After the owner left, the next input of a read-only `Owner` attachment emits `Lease(Owner)` and is forwarded; a `Viewer` never takes the lease. A chunk that holds `0x04` ends that attachment with `Ended(Detached)` and nothing of the chunk is forwarded. In an attachment whose lease is `ReadOnly`, a chunk with `0x03` ends only that attachment with `Ended(StreamClosed)` (U5 A4); in the owner `0x03` is forwarded. A detach key and a `Detach` command produce no `Exited`. A `Detach` command ends with `Ended(Detached)`; the process survives. Dropping a session detaches it. When the main process exits, every attachment gets `Exited(Some(code))` then `Ended(StreamClosed)`. A terminal on a sandbox that does not accept exec is `Err(NotReady { phase })`; an absent sandbox is `NotFound { Sandbox }` on all five entry points (U5 A3). A shell session runs the real `/bin/sh` without arguments, with pipes and no PTY, in the backing directory of `workdir`, with the environment of the request; output is raw `Output` (stdout and stderr merged); at exit it emits `Exited(Some(code))` then `Ended(StreamClosed)`. A shell never touches the lease or `main_input`. `SandboxHandle::pty_size() -> Option<TermSize>` is the last size set, by the attach request or by `resize`. Ctrl-P Ctrl-Q (D09) is not modelled; the plan lists only `0x04`. Events are queued per attachment and a pump task applies the commands, so `main_input()` and `pty_size()` change after the command was received; a test waits for the effect.
- G7. Supervisor. `FakeGateway::set_supervisor(SupervisorModel)` sets the model of the sandboxes created afterwards; the default is `Cooperative`. The model acts when `write_file` stores `/sandbox/.jackin/run/start` and when the main process exits through `SandboxHandle::exit_codex`. A new sandbox shows `waiting` on its first `Ready` observation; so does a sandbox that was stopped and started: the restart removes `run/state` and `run/start`, resets the last handled ID, and writes `waiting`. A valid ID greater than the last handled one, in the file while the state is `waiting` or `exited`, starts an attempt: `running n`, or for `ExitsOnStart { code }` `exited n code`. A zero, repeated, stale or malformed ID is ignored. An ID that arrives while `running` stays in `run/start`; at `exit_codex(c)` the state goes `exited n c`, and a greater pending ID then starts at once, so the state is `running m` with no `waiting` (P7.4 F5). `NeverWritesState` never creates `run/state`. A main that is not `jackin-start` has no supervisor. State and request are real files in the backing directory, so `read_files` sees them.
- G8. Clock. `FakeClock::sleep(d)` advances the clock by `d`, records `d`, and yields once with `tokio::task::yield_now`. There are no pending sleepers. `advance(by)` moves the clock. The clock keeps a precise elapsed time; `Timestamp` is whole seconds of `start + elapsed`, so sleeps below one second add up. `FakeClock::slept() -> Vec<Duration>` lists the sleeps. "wakes pending sleepers in order" and the outline row "two sleepers wake in time order" are deleted.
- G9. `MemFs`. `FsOp` has one variant per `FileSystem` method: `Read Stat List CreateDirAll WriteNew Rename RemoveFile RemoveDirAll Mode SyncDir Lock` (the last two methods of the trait, `remove_dir_all` and `mode`, come from U7-A2 below). `fail_at(op, nth, error)` fails the `nth` call of `op` counted from the moment `fail_at` runs (decision: the plan's `fail_at(WriteNew, 1)` in H3 needs it, because the test seeds the old file with `write_new` first), starting at 1, once; several faults may coexist; the call is counted even when the fault fires. `jackin-core` gets `FileLock::from_guard`, a public constructor from an arbitrary guard value that the lock keeps until it drops. `MemFs` follows `StdFileSystem`: `list` is sorted by name; `write_new` needs an existing parent directory (`Io { kind: NotFound }`) and gives `AlreadyExists`; `rename` replaces a file target and needs an existing source; `remove_file` of a missing path is `Ok`; `lock` creates the file, needs the parent, and gives `LockHeld` until the guard drops. The root `/` always exists. `MemFs` has files and directories only; `rename` of a directory is not supported (`Io { kind: Unsupported }`). `MemFs` keeps the mode of each file (masked with `0o7777`); a directory reports `0o755`. `remove_dir_all` of a missing path is `Ok`, of a file is `Io { kind: NotADirectory }`.
- G10. Runner. `ProcessReply` is built with `exited(code, stdout, stderr)`, `timeout()`, `cancelled()` and `spawn(message)`. `ScriptedProcessRunner::new()` and `TestRunner::new()` exist; `TestRunner::scripted()` returns the scripted runner. `seen()` returns `Vec<SeenCall>` (G10b renames the plan's `RecordedProcess`), a clonable view of the received `ProcessSpec`: program, args, cwd, env (`SeenEnv::Plain(value)` or `SeenEnv::Secret`), `env_remove_prefixes`, stdin (`SeenStdin::Null` or `SeenStdin::Bytes`), timeout. Decision: no change to `ProcessSpec` in `jackin-core`, because its secret fields are deliberately not `Clone`. The view never holds a secret env value, and its `Debug` prints only the length of stdin bytes. `args_prefix` matches the start of `ProcessSpec.args`. `expect(ScriptedProgram::Docker, ..)` matches every Docker subcommand; `expect_docker(DockerSubcommand, ..)` matches one (the subcommand is the program discriminant, never part of `args`). Among the matching expectations the first unused one answers; when all are used, the last one repeats. No match, or `Program::Git`, gives `Err(Spawn { program, message })` naming the program. `seen()` records every received spec, rejected ones included. `run_secret` is answered the same way; the output is not recorded. `TestRunner` runs `Program::Git` for real, never forwards it to the scripted runner, and honours `cwd`, `env`, `env_remove_prefixes`, `stdin`, the timeout and the token.
- G11. Done in P4.1 (`TERMINAL_CHECKS` has 5 checks, `CORE_CHECKS` 13). The testkit manifest depends on `jackin-openshell` with the feature `contract` for its tests (a dev-dependency); the dev-dependencies are `jackin-openshell` (feature `contract`), `tokio` (`macros rt test-util`). No `tonic` and no `russh`. `tempfile` is a normal dependency because sandboxes are `TempDir`s.
- G12. The fake never uses wall time for its own decisions. Only the stall test runs on paused tokio time, and it starts no process. Contract and exec tests run on real time with a 10 s `tokio::time::timeout` hang guard. A real child process is bounded by `ProgramRun.timeout` on the tokio timer.

## Orchestrator additions (U7-A2 and U7-A10, from the approved P4.3 test plan)

- U7-A2. `jackin-core` `FileSystem` gains `remove_dir_all(&HostPath)` (a missing path is `Ok`; a symbolic link, inside the tree or given as the path, is removed and never followed) and `mode(&HostPath) -> Result<Option<FileMode>, FsError>` (`symlink_metadata` permissions masked with `0o7777`; a missing path is `None`; a dangling link has a mode). `StdFileSystem` and `MemFs` implement both. `StdFileSystem::write_new` cannot create a file with a setuid bit, because the kernel clears the bit on the write that follows `set_permissions`; the core test C1 therefore sets `0o4755` with `set_permissions`, and the shared `fs_contract` of H1 uses `0o755` and `0o600`. The test-local `FileSystem` fakes of `jackin-core/tests` (`CrashFs`, `GateFs`, `FaultFs`) delegate the two methods to their inner filesystem. The `RegistryFs` fake of `jackin-openshell/tests/gateway/fakes.rs` gets two stub methods; this is the one edit under `jackin-openshell`, forced by the new required trait methods, and it changes no behavior.
- G10b. `ScriptedProcessRunner` additions: `ProcessReply::lines(status, Vec<(OutputStream, String)>)` streams the lines to the `LineSink` in order and derives `ProcessOutput.stdout` and `stderr` as the per-stream concatenation (each line plus `\n`); `ProcessReply::until_cancelled(lines)` sends the lines, waits for the token, then returns `Err(Cancelled { program })`; `ProcessReply::with_effect(ProcessEffect)` attaches effects that run through `StdFileSystem` while the call runs, before any line: `WriteArgFile { prefix, bytes }` writes the file named in the argument that starts with `prefix` (atomically, mode 0644), and `CaptureContext` snapshots the directory named by the last argument into `captured_contexts()` as `CapturedContext { path, files: relative path to CapturedFile { bytes, mode } }` (regular files only; symbolic links are skipped). `expect_fn(program, args_prefix, Fn(&SeenCall) -> ProcessReply)` computes the reply from the call; it counts as used like a fixed reply. `FakeEngine` is not built here; P4.3 builds it.

## Facts of P4.1 that the fake mirrors

- `Gateway::delete_profile` is `Ok` when the profile is absent and `FailedPrecondition` when a provider uses it.
- `NotFound { Missing::Sandbox }` on `run_program`, `read_files`, `write_file`, `open_terminal(Shell)` and `open_terminal(MainProcess)` for an absent sandbox. `set_power` on an absent sandbox is the same; `delete_sandbox` of an absent sandbox is `Ok`.
- `put_provider` returns `NotFound { Missing::Profile }` when the profile was not ensured, and drops the secret material: the fake keeps only the provider name and the profile ID. `create_sandbox` with a provider that does not exist is `FailedPrecondition` ("provider '<name>' not found", `OS@v0.1.6:crates/openshell-server/src/grpc/sandbox.rs:557`).
- The generated rule key is `RuleKey::provider_rule(provider)`, so `_provider_jackin_work_codex`. `sandbox_detail` adds that rule to the effective policy for each attached provider whose profile is known; the endpoints and binaries come from the profile.
- `SandboxCreate::environment()` is the environment of the sandbox (`JACKIN_LAUNCH_ID` only).
- `GatewayError::SandboxUnreachable` exists; no fault is needed, because `FailNext` takes any error.
- `ProcessSpec.env_remove_prefixes` is carried in `seen()`. `ClosureViolation` is an enum; the fake does not evaluate closure.

## Deviations and decisions the plan did not state

- Not built, and left for the unit that needs them: `FakeEngine` (U7 and P4.3), `World` and `WorldBuilder` (U7), `UsageServer` (U13), `SandboxHandle::commit_in_project` and `SandboxHandle::write_project_file` (the export unit; no test in this plan uses them). The skeleton types in `engine.rs`, `fixtures.rs` and `world.rs` stay as they were.
- A `SandboxHandle` keeps the state of its sandbox after `delete_sandbox`; the sandbox is only gone from `FakeGateway::sandboxes()`. The directory is removed when the last handle drops.
- `write_file` outside `/sandbox` is `PermissionDenied`; `read_files` outside `/sandbox` gives `None`.
- `upload_project` and `download_file` on an absent sandbox are `NotFound`, and on a sandbox that does not accept exec `NotReady`. `upload_project` runs the host `git ls-files -co --exclude-standard`, copies the listed files that exist, then copies `.git`; a second call overwrites. `download_file` reads the sandbox file, writes the local file, and compares the SHA-256 digests of both; a mismatch is `Transfer { op: Digest }`.
- `exit_main` and `exit_codex` do nothing when the main process is not alive.
- `Fault` has no `Clone`; `inject` takes it by value.
- `ensure_profile` keeps no resource version. The optimistic lock failure between the read and the update is not modelled (no hook exists between the two reads).

### U7 (P4.3, from rewrite/reconciliation/U7.md)

This file records where U7 (`jackin-image` engine and `ensure_image`) and the moved-forward `TokioProcessRunner` (`jackin-session`, from P5.2) differ from `rewrite/API.md` sections 2 and 4, and the preconditions U7-A1 to U7-A14, U7-B1 to U7-B12 and U7-C1 to U7-C6 of the approved test plans (`P4.3-test-plan.md` and `P4.3-runner-test-plan.md`, with the orchestrator decisions on the open questions of the runner plan). The sources are DESIGN D01, D05, D06, D09, D11, D14, D17, D18, D23, API.md "U4 (P3.3)" and the P3 fix round for the image (B-R1 to B-R8), "U5 R1", and `rewrite/reconciliation/U6.md`. `API.md` is not edited here. Each record was written before the test that uses it.

## Preconditions U7-A1 to U7-A14 (image)

- U7-A1. The image ID comes from `--iidfile`. This replaces the "build metadata" wording of PLAN step 2 and API.md section 2. The iidfile is one `sha256:` line and works the same in both image stores; the metadata key set differs by store, and the file needs no JSON dependency. `--metadata-file` is not passed. The argv of one build call is the plan args verbatim, then `--iidfile=<path>`, then the context directory as the last token. The context path is absolute, so no `--` is needed.
- U7-A2. `FileSystem::remove_dir_all` and `FileSystem::mode` exist in `jackin-core` since P4.2 (U6 record, "Orchestrator additions"). The engine uses `remove_dir_all` for the per-call directory and `mode` for the Listed files.
- U7-A3. Listed copy rules. Every listed file is checked with `stat` on the file and on each ancestor directory below the root (the root itself is not checked). A symbolic link is refused, whether at the file or at an ancestor: the trait has no `readlink`, so "escapes the root" cannot be told from "stays inside", and refusing every link is the safe decision. A directory or other non-regular entry is refused (`NotARegularFile`). A missing file, or a file below an ancestor that is missing or is not a directory, is refused (`Missing`). The copy mode is normalized: `0o755` when any execute bit is set in the source, else `0o644`; this keeps the exec bit of role scripts and strips setuid bits. Materialized files keep their `MaterializedFile.mode`. A materialized `name` that is empty, absolute, or has a `.`, `..` or empty segment is refused with `Escapes`, because `PlannedBuild` fields are public. New types: `EngineError::ContextEntry { path: String, problem: ContextProblem }` and `enum ContextProblem { Symlink, NotARegularFile, Missing, Escapes }`.
- U7-A4. Per-call directory. The layout is `work_dir/<pid>-<counter>/` with the context in `context/` and the ID file at `iid`. `work_dir` is created on demand; a stale directory of the same name is removed before use. Uniqueness holds between concurrent calls (a process-wide counter) and between processes (the pid). The directory is removed on success, build failure, timeout, cancel, unreadable ID and context refusal. A cleanup failure never replaces a build error and never fails a successful build.
- U7-A5. Constants. The tail is the last 40 streamed lines, both streams in arrival order, each sanitized with `TerminalText::sanitize`. The build timeout is 3600 s; the `find` and `check_builder` timeouts are 60 s. Tests assert bounds only, not these literals.
- U7-A6. `find` runs `image inspect --format {{.Id}} <tag>` (the tag is the last argument). Exit 0 parses through `ImageId::parse` on the trimmed stdout. Exit 1 with stderr containing `No such image` is `None`. Any other failure, or unparsable output, is `DigestUnreadable { tag, output }` with stderr then stdout as output, so a dead daemon never reads as a cache miss. `check_builder` runs `buildx inspect <builder>` and reads the first top-level `Driver:` line (column 0), not a substring. `docker` passes; another driver is `BuilderDriver`; stderr with `no builder` and a nonzero exit is `BuilderMissing`; any other failure, or no `Driver:` line, is the new `EngineError::BuilderUnreadable { builder, status, output }`.
- U7-A7. `ensure_image` order and events. (1) `ImagePlan::new`. (2) `find` of the construct tag. (3) When the construct is absent or `Rebuild::Yes`: `check_builder` once, then build the construct. (4) `find` of the final tag; a hit without rebuild returns `Cache`. (5) Otherwise `check_builder` when it has not run, then build the role stage, then the derived stage. (6) Verify the delivery. The builder check therefore runs once, before the first build of the call, and never on a full cache hit; this fixes the API.md prose, which could build a construct with a `docker-container` builder before checking it. `ensure_image` emits `StageStarted`, `StageCached` and `StageFinished`; the engine emits only `Line`, with `stage = build.stage`. Built stage: `StageStarted`, `Line*`, `StageFinished`. Failed stage: no `StageFinished`. Construct found: `StageCached(Construct)`. Final hit: `StageCached(Derived)` only, nothing for Role. `Rebuild::Yes`: no Cached events and no `find` of the final tag. A closed `ProgressSink` receiver never fails a build. A cancel anywhere returns `ImageError::Cancelled`, not `Engine(Cancelled)`; a token that is already cancelled stops the pipeline before the next stage.
- U7-A8. Delivery under `docker-local` (D23). After the derived build, `find` of the final tag must equal the built ID. Otherwise the result is the new `ImageError::NotDelivered { tag, built: ImageId, found: Option<ImageId> }`. This covers `--load` dropped and a tag moved by a race. There is no delivery-method type: D23 has one method, the gateway config has no field for it, and "a registry method is not offered" is proven by the local-only audit of U7-A9.
- U7-A9. Local-only audit (`assert_local_only`), run at the end of T1 and T10 to T17. Every recorded spec has program `Docker(BuildxBuild | BuildxInspect | ImageInspect)`; no argument equals `--push` or `-o`, starts with `--output`, or contains `type=registry`; every `--tag=` value starts with `jackin.invalid/` (the literal of D05, not a constant); no env value is `Secret`. "Secrets redacted" does not apply: no secret enters an image build (`ImageRequest` has none, `BuildxEngine::new` takes no `Redactor`, `run_secret` is unused).
- U7-A10. G10b of U6 (`ScriptedProcessRunner`: `lines`, `until_cancelled`, `with_effect(WriteArgFile | CaptureContext)`, `expect_fn`, stdin bytes in `SeenCall`) was delivered in P4.2 and is recorded in `U6.md`. The R4 test is already green there.
- U7-A11. `FakeEngine` (U6 in API.md section 10, absent from the approved P4.2 plan). Contract: `new()`, `images()` (a map from tag to ID), `builds()` (successful builds in order, as `BuildRecord`), `pin_id(tag, id)`, `fail_next(stage)`. `build` stores `sha256(tag)` as the ID unless the tag is pinned; it never interprets args and emits no `Line`. `pin_id` overwrites the store entry at once, and later builds of that tag return the pinned ID. `fail_next(stage)` makes the next build of that stage return `BuildFailed { stage, Exited(1), tail }` once and record nothing. `check_builder` is `Ok`.
- U7-A12. `jackin-start` contract. The segments `publish() {` to `}` and `attempt=` to `esac` stay unchanged; T-B6 and T-B7 pass unmodified. Top-level line `envfile=/sandbox/.jackin/env-$JACKIN_LAUNCH_ID`, unquoted. Top-level functions `check_env` and `run_attempt`, each opened by `name() {` and closed by a `}` alone at column 0 with no other such line in the body. `check_env FILE`: status 0 when the file is absent or valid; otherwise status 1 and the 1-based number of the first bad line on stdout. Valid means the `EnvFile::render` grammar: `export NAME='value'` per line, `'` written as `'\''`, `EnvName` rules, every line ends in `\n`; an empty file is valid. Ordering and duplicate names are not checked by the shell (jackin never writes them; this is the deliberate looser area). A NUL byte is not checked either: `sh` cannot hold one, `EnvFile::new` refuses it, and jackin never writes it. `run_attempt N CMD...`: (1) `drain`; (2) `check_env`; when invalid, print `jackin-start: <envfile> is malformed at line <n>` to stderr, publish `exited N 65`, run `park` and return, so the agent never starts and `running` is never published; (3) otherwise publish `running N`; (4) in a subshell source the file when present, `cd` to the project (exit 66 on failure) and `exec "$@"`; (5) publish `exited N <rc>`, run `park` and print the exit line. A missing file means no extra variables and the attempt starts. The load happens inside the subshell of the attempt, so a variable removed from a replaced file cannot persist from an earlier attempt and the shell of `jackin-start` never holds the values. `check_env` runs in a command substitution, so its working variables do not reach the shell either. The main loop calls `run_attempt "$attempt" "$@"` after `last=$attempt`; a malformed attempt consumes its ID and jackin retries with a greater ID, as after any `exited`. The grammar uses explicit character lists, not ranges, so the locale cannot change it.
- U7-A13. Dev-dependencies of `jackin-image`: `jackin-testkit` (allowed by `LAYERS`), `tempfile`, `tokio` with `macros rt time`. The tests are one target, `tests/image_io/main.rs`, with modules `support`, `buildx`, `ensure` and `env_file`. No comments, `Result<(), Box<dyn Error>>`, a 10 s `tokio::time::timeout` hang guard on every async test, each test under 150 lines. `jackin-testkit` joins `[workspace.dependencies]` as a path dependency, which the dev-dependency needs.
- U7-A14. The extended `EngineError` and `ImageError` variants (`ContextEntry`, `BuilderUnreadable`, `NotDelivered`, `SourceMoved`) have rows in `engine_failure_messages_show_names_and_build_output_as_plain_text` (`tests/engine_errors.rs`): no `Debug` leakage, no `\n` escapes, names present.

## Preconditions U7-C1 to U7-C6 (P4.3 fix round: image host, source checks, derived Dockerfile)

- U7-C1. Order inside a dependent build. `Pipeline::build` runs `live`, then `check_builder` (once per call), then the source check, then emits `StageStarted`, then the engine build, then the source check again, then emits `StageFinished`. A stage whose check fails has no `StageFinished`, which matches U7-A7 "failed stage". The role and derived builds are the dependent builds; the construct build has no source.
- U7-C2. Expected IDs. The role build checks the construct tag against the construct ID used for the key (the cached ID or the one the construct build returned). The derived build checks the role-stage tag against the ID the role build returned, not a fresh `find`. The derived build never checks the construct tag. The `SourceMoved` check of the derived stage runs before the delivery check of U7-A8, so it wins over `NotDelivered`.
- U7-C3. Fail closed. A `find` error inside a check propagates as `ImageError::Engine(..)` or `ImageError::Cancelled`. It never becomes `SourceMoved` and never counts as a pass. A source that is absent (`None`) is a mismatch.
- U7-C4. Orchestrator decision: the state after `SourceMoved` on the derived stage is accepted. The tags written before the error stay. Every final and role-stage key includes the construct ID, so after a construct move no later call computes a key from the old ID, and a role-stage tag moves only to a build of the same key. Untagging would need `docker image rm`, which D01 does not permit. `ImageEngine` gains no `remove`, and there is no row T20-4.
- U7-C5. Derived Dockerfile. The single `RUN` between the last `COPY` and the last `USER` holds, joined by `&&`: `command -v getcap >/dev/null` and `test -z "$(getcap -r / 2>/dev/null)"` (the two construct segments), and one setuid and setgid segment that contains `find / -xdev -type f -perm /6000`, prints each offending path on stderr behind a message and exits 1 when any is found. The file keeps no comment except the `# syntax=` line. The test `derived_dockerfile_checks_fail_on_file_capabilities_and_setuid_files` extracts the segments from the shipped text and runs them under `/bin/sh` with `env_clear`, `LC_ALL=C` and stubbed `getcap` and `find` binaries (written with a shebang and no `#` line).
- U7-C6. `FakeEngine` script, an external-boundary seam. `at(moment, effect)` registers a one-shot entry; the entry is consumed the first time its moment arrives. Moments: `EngineMoment::AfterFind(tag)` and `EngineMoment::BuildFinished(stage)`. Effects: `EngineEffect::Retag { tag, to: Option<ImageId> }` and `EngineEffect::Cancel`. `BuildFinished` runs inside `build`, after the image and its record are stored and before the return; the returned `Ok(id)` is the ID from before the move; `Cancel` cancels the token that `build` received. A failed build (`fail_next`) runs no `BuildFinished` entry. `AfterFind` runs after the `find` of that tag computed its answer, so the answer shows the state before the move; there is no build token at that moment, so a `Cancel` registered for `AfterFind` has no effect. `Retag(None)` removes the tag from `images`. Neither effect touches the pinned map.

## Records of the P4.3 fix round

- Registry host. Every image tag starts with `jackin.invalid/` (D05; `.invalid` is reserved by RFC 6761 and never resolves). `ImageTag::parse` rejects `jackin/...` and every other host; the limit of 255 characters covers the full name including the host. The `BranchName` rule that refuses the `jackin/` prefix is the Git branch namespace and is unchanged. `MISSING_IMAGE` in `jackin-openshell` and all test literals take the new host; the live contract run of `jackin-openshell` must be repeated because an unresolvable host may fail provisioning differently.
- `ImageError::SourceMoved { tag: ImageTag, expected: ImageId, found: Option<ImageId> }`. Its message names the tag, the expected ID and the found ID (or "no image"), with no `Debug` text.
- `FakeEngine::at(moment, effect)` with `EngineMoment` and `EngineEffect` (U7-C6) joins the `FakeEngine` contract of U7-A11.
- Test rows: T18 (`grammar::<ImageTag>` in `jackin-core/tests/ids.rs`), T19 (`build_plan.rs`), T20, T21, T22 and T23 (`image_io/ensure.rs`), the TR9 row 2 amendment (`lifecycle.rs`), and the rows L1 to L7 that change literals or messages only. T23 row 1 runs for `Rebuild::No` and `Rebuild::Yes`: with `Rebuild::No` the first `find` stops a pre-cancelled call, so only `Rebuild::Yes` reaches the `live` check in `Pipeline::build` for the construct stage. The `DockerModel` of `support.rs` gained `after_build(stage, AfterBuild::Retag | AfterBuild::FailInspect)` for T22.
- The companion edit of `build_plan.rs` relaxes the setuid assertion of `derived_dockerfile_re_asserts_root_ownership_and_forbidden_tools_after_the_role_layer` to any segment that contains the `find` command, because T19 owns the behavior.

## Preconditions U7-B1 to U7-B12 (host process runner, RP1 to RP12)

- U7-B1. `Program::leading_args(self) -> &'static [&'static str]` in `jackin-core`: the `Docker(sub)` arm returns `sub.argv()`, the others return `&[]`. `jackin-session/src/host.rs` runs `Command::new(spec.program.binary()).args(spec.program.leading_args()).args(&spec.args)`. The file `crates/jackin-session/src/host.rs` (not the crate) joins `COMPUTED_COMMAND_DIRS`, and the K6 test in `crates/xtask/tests/repo_gates.rs` is amended in the same commit: `host.rs` may compute a program; another session file is still flagged; a literal `docker` in `host.rs` is still flagged.
- U7-B2. Scripted child. The program is `Program::Git` with args `["-c", "alias.t=!sh", "t", <script path>, <script args>]`. The helper writes each script from a Rust `const &str` into the `TempDir` of the test. Scripts contain no `#`. Every spec sets `env_remove_prefixes = ["GIT_"]` and `GIT_CEILING_DIRECTORIES` to the canonical parent of the `TempDir`. All paths are canonicalized. There is no test-only `Program` variant.
- U7-B3. `PATH` is overridden only through `ProcessSpec.env` (TR11 and TD1); `set_var` is never used. All other tests inherit `PATH`. Contract: the runner passes `spec.env` to the child, so the program lookup uses the `PATH` of the spec.
- U7-B4. No `set_var`. TR7 uses the variables that cargo and nextest set for every test; its first phase is a positive control that shows the child sees them.
- U7-B5. Contracts of the runner. (a) A line is the bytes up to `\n`; the `\n` is stripped; `\r` is kept; a trailing partial line at EOF is delivered; invalid UTF-8 is decoded lossily. (b) The runner awaits `send` and never drops a line; a closed sink is not an error, and the runner keeps draining and discards. (c) With a sink, `ProcessOutput.stdout` and `stderr` are each at most 65536 raw bytes, the true suffix of that stream (the runner keeps exactly the last 65536 bytes); without a sink all output is returned, uncapped (orchestrator decision 4). (d) A nonzero exit or a signal is `Ok(status)`. (e) `Null` stdin is `/dev/null`; `Bytes` is written while output is read, then stdin is closed; a broken pipe is ignored. (f) `Timeout { program, after: spec.timeout }`; the clock starts at spawn and covers the whole call. (g) `Cancelled { program }`, including for a token that is already cancelled (the child is not started then). (h) Timeout and cancel kill the process group of the child (`process_group(0)` and `ProcessGroup::stop`), then reap the direct child before returning; the runner returns even when a descendant holds the pipes. (i) `run_secret` shares one spawn, timeout, cancel and kill core with `run`, takes no sink, puts stdout only into `SecretBytes` (the buffer grows by allocating the next buffer and zeroizing the old one), and returns stderr as plain bytes. (j) A spawn failure is `Spawn { program, message }` where the message is the OS error only. (k) The runner removes inherited variables that match a non-empty prefix first, then applies `spec.env`; `EnvValue::Secret` is exposed only at the spawn call.
- Orchestrator decisions on the open questions of the runner plan (binding). (1) A descendant that keeps the pipes open after the direct child exited cleanly: the runner reaps the child, drains for a grace of 2 s, then kills the process group, drops the pipes and returns `Ok(status)` with what it read. (2) Dropping the `run` future kills the process group (a drop guard); a call that completes normally disarms the guard, so a daemon that detached its output survives a clean exit. (3) An empty string in `env_remove_prefixes` matches nothing. (4) Unstreamed output is uncapped.
- U7-B6. Dependencies of `jackin-session`: runtime `async-trait`, `rustix` (feature `process`, `1.1.6`, already in `Cargo.lock`), `tokio` (`io-util macros process rt sync time`), `tokio-util`, `zeroize`; dev-dependencies `tempfile` and `tokio` (`macros rt time`). `rustix` joins `[workspace.dependencies]`.
- U7-B7. Layout. The target is `crates/jackin-session/tests/process_runner/main.rs` with modules `support`, `streaming`, `exit`, `stdin`, `env`, `lifecycle` and `secret`. No comments. Tests return `Result<(), Box<dyn Error>>`, every async test has a 30 s guard, every `recv` its own 10 s guard with a message that names the behavior, and each test is under 150 lines. `OutputLine` and `ProcessOutput` hide their content in `Debug`, so the tests compare `(OutputStream, String)` tuples and byte slices; large vectors are compared by length, then by the first diverging index.
- U7-B8. Leftover-process probe `assert_no_leftover(dir)`: the grandchild appends `x` to `dir/beat` every 50 ms for at most 600 iterations; the helper asserts that `beat` exists and is non-empty, sleeps 300 ms, reads the length, sleeps 600 ms and reads again; the lengths must be equal. It never probes pids.
- U7-B9. Timing. Timeout tests use real time and a 3 s `spec.timeout`; they fail loudly if `ready` never arrived. No `start_paused`.
- U7-B10. `SystemClock` and `capture_env` are not part of this unit.
- U7-B11. `ProcessStdin` has only `Null` and `Bytes`. `EnvValue::Secret` is covered in TR7.
- U7-B12. Companion test TD1 lives in `crates/jackin-testkit/tests/host_runner.rs`, because session code and tests may not name `Program::Docker`. D18 lets a test stand in for `docker`: a stand-in script in a `TempDir/bin`, with `PATH` set through `spec.env`.

## Deviations and decisions the plans did not state

- Module layout. `BuildxEngine` is in `crates/jackin-image/src/buildx.rs` and the context staging in `src/context.rs`; `ImageEngine`, `EngineError` and the new `ContextProblem` stay in `engine.rs`; `ensure_image` and its event types stay in `ensure.rs`. PLAN names `build.rs` and `deliver.rs`; there is no separate delivery module because the delivery check is one `find` inside `ensure_image` (U7-A8).
- TR3 arithmetic. The runner plan expects 30008 sink messages. The script prints 30000 numbers, one invalid-UTF-8 line and `done` on stdout (30002 lines) and 5 lines on stderr, which is 30007. The test asserts 30007.
- T3 allows the empty `work` directory. The engine creates `work_dir` on demand (U7-A4), so the "nothing outside work_dir" check compares the parent directory against its earlier listing plus `work`.
- The T6 cancel row and the T1 argv checks run against the faithful docker model of `support.rs`. Because the model applies `CaptureContext` to the last argument, a context path placed before `--iidfile=` fails inside the fake before the test assertion; the position check in T1 stays as the assertion of record.
- `Pipeline::build` in `ensure.rs` runs the builder check lazily before its first build, which gives "once, before the first build, never on a hit" without a separate flag at the call sites. A token that is already cancelled stops the pipeline before the next engine call (no engine call is made).
- `tests/dockerfile.rs` (U4) matches `ImageError` exhaustively and gained `NotDelivered` in the `Other` arm; this is the only edit to an earlier test body and changes no assertion.
- `tokio` features of `jackin-image`: `macros` (for `select!` and `join!`) and `sync`.
- Not provable by the plans' observables, and kept: the check that refuses to start a child for a token that was cancelled before the call (TR9 row 2 cannot tell it from a start-then-kill, because `Cancelled` is returned either way).
- Live-evidence driver: `cargo run -p jackin-session --example build_images -- [--role-dir DIR] [--role-name NAME] [--builder NAME] [--work-dir DIR] [--calls N] [--rebuild]`. Without `--role-dir` it creates a one-file git role (`FROM construct AS role`). It calls `ensure_image` with `BuildxEngine` over `TokioProcessRunner` and prints every event and the `BuiltImage`.

### U8 (P5.1, from rewrite/reconciliation/U8.md)

This file records where U8 (`jackin-session` planner, role catalog, account sync, launch executor, and the `jackin-testkit` additions TK-1 to TK-10) differs from `rewrite/API.md` sections 4 and 7, and the preconditions U8-A1 to U8-A9 and U8-B1 to U8-B7 of the approved test plans (`P5.1-test-plan.md` and `P5.1-gap-test-plan.md`, with the orchestrator decisions at the end of the gap plan). The sources are DESIGN D05, D07 to D11, D15, D18, D22, D24, D26, API.md "U4", "U5", "U6" and "U7". `API.md` is not edited here. Each record is written before the test that uses it.

## Preconditions U8-A1 to U8-A9 (launch)

- U8-A1. Scope split. P5.1 builds `RoleCatalog::{new, load, load_path}`, `LoadedRole`, `plan_launch`, `plan_image`, `files.rs`, `decide`, the probe, the hooks, the executor, the events and the cancel path. `RoleCatalog` is the only way to a `LoadedRole` (private fields). The planner builds the whole Docker branch: resolve, `with_rule`, the three `DOCKER_*` env names, `DockerLayer::Client` in the image request, and `warnings() == [DockerCapability]`. P5.4 keeps only the executor writes of `ca.pem`, `cert.pem` and `key.pem` and the `Warning` event.
- U8-A2. `LaunchFailure.step` by error: `Image` gives `Image`; `Gateway` from `provider_state` and `ProviderUnavailable` give `Provider`; `AmbiguousLaunch`, `NameCollision`, `ProvisioningFailed` and `Gateway` from create give `Sandbox`; `MainProcessEnded`, `SandboxSuspended` and `ReadyTimeout` give `Ready`; the credential writes and the missing placeholder give `Credentials`; the upload gives `Upload`; `SupervisorTimeout` gives `Supervisor`; `HookFailed` gives `Hooks`; `AttemptsExhausted` and `AttemptTimeout` give `Start`. `sandbox` is `Some(name)` whenever the run observed or created a sandbox with the launch label, else `None`. `ProviderUnavailable` wraps only `sync_account` failures; a failing `provider_state` is `Gateway`.
- U8-A3. Event contract. Every run that returns `Ready` emits exactly nine `StepStarted` and nine `StepFinished` in `LaunchStep::ALL` order, each Finished before the next Started. A step that did no work reports `Skipped(reason)`: `Image` gives `ImageCached`, `Provider` gives `ProviderPresent`, `Sandbox` gives `SandboxExists`, `Upload` gives `AlreadyUploaded`, `Hooks` gives `SetupAlreadyDone` when only setup was skipped. When the first supervisor reading is `Running`, `Supervisor`, `Hooks` and `Start` report `Skipped(AlreadyRunning)`. `Ready` and `Credentials` always report `Done`. `Image(ImageEvent)` events fall between `StepStarted(Image)` and `StepFinished(Image)`. `SandboxKnown(name)` is emitted exactly once, after the sandbox is found or created and before `StepStarted(Credentials)`. A failed or cancelled run emits `StepStarted(x)` for the step it stopped in, with no `StepFinished(x)`, and nothing after. Exception (P5.1b, O5): a host-side project refusal or a host git version failure happens before any gateway call and before `StepStarted(Image)`; it fails with `step: Upload, sandbox: None` and emits no step event. `Warning(DockerCapability)` appears only for Docker. `Session::launch` emits one per element of `plan.warnings()` as the first event of every call, after the project and certificate prechecks pass and before `StepStarted(Image)`, so a refused precheck or an early cancel emits none, and a second call with the same launch ID emits it again. P5.4 exception: a missing, linked, empty, oversized or wrongly shaped engine certificate file fails with `step: Credentials, sandbox: None` before any gateway call, emits no event, and the key is read again at the write. A retry whose sandbox labels differ from the plan in `jackin.dev/image` or `jackin.dev/docker` (the label holds `off` or the engine port) fails at `Sandbox` with `LaunchError::StaleSandbox` after the image step and changes nothing.
- U8-A4. Credentials step. It runs `/usr/bin/printenv` with `args == [keys.access_token]`. A non-zero exit or empty stdout fails at `Credentials` with the new variant `LaunchError::PlaceholderMissing { sandbox, key }`; no `auth.json` is written then. `auth.json` has mode `0o600` from `spec.auth_file`. Its top-level keys are exactly `auth_mode` (`chatgptAuthTokens`), `OPENAI_API_KEY` (null), `tokens`, `last_refresh`. `last_refresh` is the `Clock` time as RFC 3339 UTC in whole seconds (`2026-11-02T10:11:12Z`, 20 characters, as P0.3 shows). `tokens` holds exactly `id_token` (the P0.3 non-secret placeholder JWT: header `{"alg":"none","typ":"JWT"}`, payload `{"iss":"https://auth.openai.com","aud":"codex","sub":"openshell-app-server"}`, signature part `placeholder`), `access_token` (the printenv output without its trailing newline), `refresh_token` (`""`) and `account_id` (the real ID of the account's `CredentialSource::CodexLogin`). The step writes `auth.json`, `files().codex_config` and `files().env`; the env file is written even when the role has no `[env]`. `read_chatgpt_login` reads `auth_mode`, `tokens.access_token`, `tokens.refresh_token`, `tokens.oauth_provenance.client_id` and `tokens.account_id` (U8-B3).
- U8-A5. `HookPresence`. A hook counts as present only when `read_files(hook_path)` returns non-empty bytes. An undeclared hook is an empty file in the image and is not run.
- U8-A6. Hook output. `HookFailed.output == TerminalText::sanitize(stdout ++ stderr)`. Setup and preflight run with `workdir == sandbox_layout::PROJECT`, the env file values as env, and `ProgramStdin::Closed`. A non-zero exit stops the launch. `touch_on_success` is set only for setup.
- U8-A7. Cancel path. On cancel the executor does one final `find_sandboxes(launch label)` and returns `Cancelled(RestoreCandidate { launch_id, sandbox })`. `sandbox` is `Some(name)` exactly when the gateway holds one sandbox with that label, even when the token was cancelled before the call. A cancel never calls `delete_sandbox` and never returns `Err`. A token that is cancelled while an act runs turns any error of that act into the cancel outcome.
- U8-A8. Concurrency. Two concurrent `launch` calls with one ID both return `Ok(Ready)`. The loser of the create race gets `AlreadyExists`, reads by name and joins. Setup and preflight may run twice. The attempt write is fenced by the attempt ID.
- U8-A9. Test limits and harness rules. `LaunchLimits { ready: 30 s, supervisor: 20 s, attempt: 20 s, hook: 600 s, upload: 600 s, create_attempts: 3 }`. Timeout assertions use `>=` on `FakeClock::slept()` sums and on the error's `after`; they do not pin the 500 ms poll interval. The integration targets are `tests/plan/main.rs`, `tests/launch/main.rs` and `tests/accounts/main.rs` (U8-B7), each with modules. No comments. `type TestResult = Result<(), Box<dyn std::error::Error>>`. Every async test sits under a 10 s `tokio::time::timeout` hang guard. The default `#[tokio::test]` runtime. Each test is under 150 lines. Dev-dependencies: `jackin-testkit`, `tempfile`, `serde_json`, `toml`, `tokio` (`macros rt time`).

## Preconditions TK-1 to TK-7 (testkit)

- TK-1. `FakeGateway::force_phase(&SandboxName, Phase)` sets the phase of an existing sandbox and ends its settling. It is the only way to reach `Suspended`, `Deleting` and `Unknown`.
- TK-2. `Fault::StaleSupervisor { observations: u32 }`. For each sandbox started by `set_power(Start)` after the injection, the first `observations` observations that show `Ready` leave the pre-stop `run/state` and `run/start` in place. The restart rewrite (`waiting`) happens at the next observation. The fault stays armed for later starts.
- TK-3. `enum Trigger { Op(GatewayOp), Write(SandboxPath), Program(SandboxPath) }`. `Fault::FailAt { trigger, error }` fails the first matching call once, at entry, with no state change. `Fault::CancelAt { trigger, token }` cancels the token at entry of the first matching call; the call itself runs normally. Neither counts calls. A `Write` trigger matches `write_file` of that path, and a `Program` trigger matches `run_program` of that program; an `Op` trigger matches every call of that op. Amplification: `run_program` returns `Cancelled { op: Program }` with no state change when its own token is already cancelled at entry, as the real gateway ends the exec stream; `upload_project` already behaves so.
- TK-4. `FakeGateway::install_file(&SandboxPath, Vec<u8>)` makes a path outside `/sandbox` readable through `read_files`, for image content such as hooks. `script_program` stays the execution side.
- TK-5. `World`, `WorldBuilder`, `RoleFixture` and `RepoFixture` get bodies. `RoleFixture` writes a real git role repository in a temp dir. `RepoFixture` writes a real project repository. `WorldBuilder::limits(LaunchLimits)`, `World::cache_dir()`. `World.fs` is `StdFileSystem` on a temp dir, because upload and the role catalog run real `git` on real files. `WorldBuilder::build` links `FakeEngine` to `FakeGateway`: a tag that `FakeEngine` built is runnable (`add_image`), and any other tag ends `Error { None }` with `Provisioning::Failed` (G2). `FakeEngine` follows U7-A11 and U7-C6.
- TK-6. `SandboxHandle::files() -> BTreeMap<SandboxPath, Vec<u8>>` is a snapshot of every regular file under `/sandbox`.
- TK-7. `RunRecord` gains `args: Vec<String>` and `workdir: Option<SandboxPath>`.

## Preconditions U8-B1 to U8-B7 and TK-8 to TK-10 (gap plan)

- U8-B1. One-pass role check. `RoleCatalog::findings(&self, root: &HostPath) -> Vec<RoleError>`. `jackin role check` and the staged check of P9.2 call it. Order: manifest findings first, one `RoleError::Manifest { path: <root>/jackin.role.toml, source }` per item of `RoleManifest::findings(text)`; if there is any, the list ends there. Otherwise these groups follow, each independent of the others' failures: Dockerfile (the file check, else at most one `RoleError::Dockerfile`), Policy (the file check, else at most one `RoleError::Policy` from `parse_authored`), setup hook then preflight hook (the file check for each hook the manifest names), Closure (only if the policy parsed; at most one `RoleError::ProviderHostClosure(Vec)` that lists every violation across every `Agent` profile). File check: absent gives `FileMissing { path }`; a symlink at the file or at any ancestor directory below the root gives `EscapesRepository { path }` (the trait has no `readlink`, so every link is refused, as in U7-A3); a directory or other non-regular entry gives `FileNotRegular { path }`; `path` is the manifest's `RepoPath`. An ancestor that is absent or not a directory gives `FileMissing`. `load_path` runs this pass and returns `findings[0]` when the list is non-empty; only after an empty list does it compute the revision with git. An absent root is `RootMissing { path }`, a root that is not a directory is `RootNotDirectory { path }`, and an existing root without the manifest is `ManifestMissing { root }`; other absent files keep `FileMissing`. Orchestrator decision: the manifest-first stop is accepted.
- U8-B2. Revision digest. It uses `git ls-files -z -co --exclude-standard`. A tracked path that is absent from the work tree is skipped. The digest depends neither on the root path nor on the commit ID. Definition: SHA-256 over, for each listed name in byte order, `u64 big-endian length ++ name ++ u64 big-endian length ++ content`; an entry that is a directory or a link hashes an empty content.
- U8-B3. `read_chatgpt_login` reads only `auth_mode`, `tokens.access_token`, `tokens.refresh_token`, `tokens.oauth_provenance.client_id` and `tokens.account_id` and ignores every other key. Check order: a missing file gives `LoginUnreadable { path: <home>/auth.json }`; a read error gives `Fs(_)`; invalid JSON gives `AuthJsonInvalid { key: "auth.json" }`; a missing or non-string `auth_mode` gives `AuthJsonInvalid { key: "auth_mode" }`; `auth_mode != "chatgpt"` gives `NotAChatgptLogin`; then `tokens` (an object), `tokens.access_token`, `tokens.refresh_token`, `tokens.oauth_provenance.client_id`, `tokens.account_id` in that order, each failing with `AuthJsonInvalid { key }` for a missing field, a wrong type or an invalid grammar. No error text holds file content or a serde message.
- U8-B4. `sync_account`. Order: mode check, read the login, account ID check, `ensure_profile`, `put_provider`. A Codex account with a non-`CodexLogin` source returns `Unsupported { account, mode: ApiKey }` before any file read or gateway call. Any other provider returns `HostOnly` and touches nothing. The `ProviderSpec` has the name `ProviderName::for_account`, the profile `codex`, the `keys` of the `AgentSpec` row, and `ChatgptLogin` material from the login file. `put_provider` is always called. Orchestrator decision: when the login file's `tokens.account_id` differs from the config's `chatgpt_account_id`, `sync_account` fails with `AccountError::AccountIdMismatch { account }` and writes nothing to the gateway.
- U8-B5. `LoadedRole::dockerfile().path`, `hooks().setup.path` and `hooks().preflight.path` equal the manifest paths. `RoleCatalog::new` takes no gateway, engine or config and starts no `docker`, `openshell`, `ssh` or `op`.
- U8-B6. `LaunchEvents` contract. `pub const LAUNCH_EVENT_CAPACITY: usize = 64`. `launch_events` makes a bounded channel of exactly that size. `pub async fn emit(&self, event: LaunchEvent)`. Only `LaunchEvent::Image(ImageEvent::Line { .. })` can be dropped, and only under `BestEffort` when the channel is full. Every other event waits for room in both modes. Under `Reliable`, lines wait for room too. If the `LaunchFeed` is dropped, `emit` returns at once and discards the event, in both modes, including an `emit` that is already waiting. `recv` returns `None` once every `LaunchEvents` clone is dropped and the queue is empty. The executor forwards the engine's progress through this `emit`; there is no other drop path.
- U8-B7. Harness amendments. A third integration target `tests/accounts/main.rs` with modules `support`, `login` and `sync`. The role rows are in `tests/plan/roles.rs`. The delivery tests are in `tests/launch/delivery.rs`. No tokio paused time. Helper `accepted(events, event) -> bool` polls the `emit` future once and drops it.
- TK-8. `FakeGateway::provider(&ProviderName) -> Option<ProviderRecord>`. The record is `{ profile, access_key, account_id_key, access_token, refresh_token, client_id, chatgpt_account_id }`. Tokens are plain `String`s copied from the `ProviderSpec` by `put_provider`. A later `put_provider` of the same name replaces the record. `ProviderRecord` has a `Debug` that prints no token. `providers()` is unchanged.
- TK-9. `FakeEngine::script_lines(stage: BuildStage, lines: Vec<String>)`. Each successful `build` of that stage first sends `ImageEvent::Line { stage, text: TerminalText::sanitize(line) }` per line, in order, awaiting `send` on the sink (a closed sink is ignored). It then records the build. An unscripted stage emits nothing.
- TK-10. `RoleFixture` additions, on real git: `root()`, `file(path, bytes)` and `remove(path)` (write without committing), `symlink(path, outside_target)`, `dockerfile_path`, `policy_path` and `hook_path(kind, path)`, `manifest_text(String)`, `gitignore(text)`, `commit() -> CommitId`. Fixture content has no path or time inside it.

## Landed code that differs from the plans

- The plans were written before P4.3. `FakeEngine`, `ensure_image`, `BuildxEngine` and `ImageError::SourceMoved` have landed, so TK-5 builds `World` over them. Every image tag has the host `jackin.invalid/` (U7-C records); the tests take tags from `BuiltImage.tag` and never write a tag literal.
- `API.md` names `launch/{decide,probe,hooks}.rs` and `files.rs`. The module layout is recorded under "Deviations and decisions".

## Deviations and decisions the plans did not state

- Module layout. `launch.rs` holds the public event, outcome and error types and `LAUNCH_EVENT_CAPACITY`; `launch/{decide,probe,hooks,tracker,run}.rs` hold the pure decision table, the sandbox probe, the hook runner, the step event tracker and the executor. `files.rs` is crate private. `RoleCatalog` and `LoadedRole` are in `roles.rs`, `read_chatgpt_login` in `credentials.rs`, `Session::sync_account` in `accounts.rs`, `plan_launch` and `plan_image` in `plan.rs`.
- `decide` returns `Decision::Run(Act)` with a typed act instead of `Run(LaunchStep)`, and `Observed` is an enum (`Absent`, `Ambiguous`, `Settling`, `Ready` with the probe) so that a ready sandbox always carries its probe. `Done.waited` holds the time spent in each wait. Every wait is one clock sleep of 500 ms followed by a new observation, so the fake, which settles only on an observation, behaves as the real supervisor does in time.
- `LaunchFailure.error` is `Box<LaunchError>` (clippy `result_large_err`), and `LaunchFailure` implements `Display` and `Error`. The message is `launch <id> failed at <step>`, and the cause is left to `source()`.
- New error variants: `LaunchError::PlaceholderMissing { sandbox, key }` (U8-A4), `PlanError::RoleMismatch { requested, loaded }`, `RoleError::Path(PathError)`, `AccountError::Policy(PolicyError)` and `AccountError::AccountIdMismatch { account }` (orchestrator decision of the gap plan).
- `LaunchPlan` also exposes `account_entry()`, `chatgpt_account()` and `project()`, and `SandboxDraft` exposes `provider()`, because a plan builds exactly one provider; the executor needs them.
- `render_codex_config` serializes a typed table with the `toml` crate (the workspace dependency has the `display` feature). The text is `[projects."/sandbox/project"]` with `trust_level = "trusted"`, preceded by `model = "<name>"` and a blank line when the manifest names a model.
- A `Powered` boot accepts only a `waiting` reading. The API.md table names only `Running`; a stale `exited` file of an earlier boot has the same race (the start request would be removed by the restart), so it waits too.
- The step event tracker moves forward only. A step that the run has already passed is not announced again, for example the recreate of a failed provisioning, so a run that returns `Ready` still emits nine pairs. A failure whose step lies before the open step adds no event.
- Quick execs (`printenv`) use a 30 s timeout constant; `limits.hook` bounds the hooks and `limits.upload` bounds the upload (a real `tokio` timeout, not the injected clock, because the fake clock sleeps instantly).
- The plan R5 row B names `hooks.setpu` as the first finding. The landed `RoleManifest::findings` reports section by section (`docker` before `hooks`), so the test compares the three finding kinds and compares each item with `RoleManifest::findings(text)` in order instead of naming the first key.
- The `RoleSource::Git` branch of `RoleCatalog::load` (`git clone`, `git fetch`, `git checkout --detach`) is untested, as the gap plan declares.
- `FakeGateway::release_stall(&SandboxPath)` is a testkit addition (the plan L7 retry needs the stall removed on the same gateway).
- Plan L3 row 1 expects three builds after a failed derived build and its retry. The landed `ensure_image` checks only the final tag for a cache hit and builds the role stage again after a failed derived stage (U7-A7). The test asserts one construct build and one derived build and that the construct is not rebuilt; the role stage builds twice.
- Plan E1 run 2 names `StageCached(Derived)` only. The landed `ensure_image` also emits `StageCached(Construct)` when the construct image is found (U7-A7), so run 2 shows both.
- `LaunchStep` skip reasons for the cached image, the present provider, the existing sandbox, the present upload marker and a running supervisor follow U8-A3. A step that the run did not enter reports its implied result when a later step is entered.
- The foreign sandboxes of L9 are observed once before the snapshot, because the fake settles a sandbox on its first observation.
- P4.4 fix-round decisions built early. (1) Any `Err` from `upload_project` fails the launch at `Upload`, and the marker is written only after `Ok` (already the executor's behavior). (2) The planner builds the launch policy in one function, `launch_policy`, so a `ClosedPolicy` swap is local. (3) `RoleCatalog` lists no tracked file that is absent from the work tree and no directory entry (a submodule path) in `LoadedRole::files()`; the revision digest still covers the name of a submodule path. A link stays listed, so that `stage_listed` refuses it. Test `context_files_skip_deleted_tracked_files_and_submodule_paths` in `tests/plan/roles.rs`.
- Red and green. Mutations applied to the executor and run against the launch and accounts targets: setup always run (4 tests fail), the hook token replaced (L6 and L7 fail), the stale `Powered` check removed (L11 fails), unbounded recreate (L12 fails), join on a foreign label (L9 hangs into the guard), delete on cancel (L6 and L7 fail), upload marker ignored (many fail).

## Fix round after the independent red/green review of 64ab4542

Decisions.

- The cancel check at the head of the executor loop is deleted. An observation is read-only and the check after it covers every cancel. A token that is cancelled before `launch` now costs one `find_sandboxes` and still returns `Cancelled` with the restore candidate.
- `Tracker.known_sent` is deleted. The tracker moves forward only, so `StepFinished(Sandbox)` is emitted once and `SandboxKnown` follows it once.
- The `quote` escape arms of `files.rs` are deleted. `ModelName` allows only alphanumerics and `. _ : / -` and the work directory is the constant `/sandbox/project`, so no input needs an escape. The renderer wraps the two values in plain quotes.
- `PlanError::RoleMismatch` stays. `LaunchRequest::role` is part of API.md, so the planner keeps checking it against the `LoadedRole` and a row in P8 pins the check.
- `decide` waits after a power start. `Phase::Stopped` with `boot == Powered` is a settling phase and is waited for under `limits.ready` like `Starting`; it no longer issues `Power` again without a wait or a bound.
- `LaunchError::AttemptsExhausted` reads "the attempt IDs of {sandbox:?} are exhausted".
- Testkit TK-11. Every async call of `FakeGateway` yields once to the scheduler before it acts, so a loop that never waits is cut by the 10 s hang guard instead of freezing the runtime. Two runs of one launch ID in `join!` now interleave.
- Testkit TK-12. `Fault::HideUntilCreateConflict` hides every sandbox from label queries until a `create_sandbox` meets an existing name and returns `AlreadyExists`; the by-name query is not hidden. It models the create race of U8-A8 without scheduling.
- Testkit TK-13. `Fault::StartIgnored` makes `set_power(Start)` on a stopped sandbox succeed and leave the phase `Stopped`, for every later start.
- TK-3 amendment. The cancelled-token row scripts `/bin/sh` through `script_program`, so only the entry check of `run_program` can produce `Cancelled`; an unscripted `/bin/sh` honours the token in `run_command` and hid a removed check.
- `delivery.rs` loses the vacuous `assert_ne!(Skipped(ImageCached), Done)`.

Red and green of the new and changed rows. Each mutation was applied alone, the named test failed with the line shown, and the test passed after the revert.

| Row | Test | Mutation | Red line |
|---|---|---|---|
| K1 cancelled token | `tests/gateway/triggers.rs` `a_program_run_with_a_cancelled_token_ends_cancelled_and_records_nothing` | entry check removed from `gateway/exec.rs` | `assertion failed: matches!(result, Err(GatewayError::Cancelled { op: GatewayOp::Program }))` at `triggers.rs:159` |
| TK-11 yield | `lifecycle::every_gateway_call_yields_to_the_scheduler_before_it_acts` | yield removed from `find_sandboxes`, `create_sandbox`, `provider_state` in turn | `assertion failed: pending_once(fake.find_sandboxes(&selector))` at `lifecycle.rs:201`, then `create_sandbox` at `:202`, `provider_state` at `:207` |
| TK-12 hide | `faults::hide_until_create_conflict_hides_label_finds_until_a_create_meets_the_existing_name` | label find not hidden; flag not cleared on conflict | `assertion failed: fake.find_sandboxes(&selector).await?.is_empty()` at `faults.rs:283`; `left: 0 right: 1` at `faults.rs:291` |
| TK-13 start ignored | `faults::start_ignored_accepts_the_start_and_leaves_the_sandbox_stopped` | flag ignored | `left: Ready right: Stopped` at `faults.rs:308` |
| L17 join success | `proofs::a_run_that_loses_the_create_race_joins_the_sandbox_of_its_own_launch` | join accepts no sandbox | `expected a ready launch, saw Err(... step: Sandbox ... NameCollision { other: Some(LaunchId("bbbb...")) })` |
| L9 hang guard | `adr008::a_name_collision_or_an_ambiguous_label_is_a_typed_stop_that_changes_nothing` | join on any foreign label | `the test hung for 10 s` (earlier: nextest kill at 180 s) |
| L10c start ignored | `phases::a_start_the_gateway_ignores_is_waited_out_on_the_clock_to_a_ready_timeout` | the `Stopped` wait arm of `decide` removed | `the test hung for 10 s` |
| L10c start not issued | `phases::a_stopped_sandbox_is_started_and_restarts_at_attempt_one` | the `set_power(Start)` call removed | `expected a ready launch, saw Err(... ReadyTimeout { phase: Stopped, after: 30s })` (earlier: hang) |
| L11 stale exited | `phases::a_stale_exited_state_after_power_start_is_waited_out_and_attempt_one_starts` | only a `Running` reading counts as stale | `expected a ready launch, saw Err(... step: Start ... AttemptTimeout { attempt: AttemptId(2), last: Reported(Waiting) })` |
| L12 events | `events::a_recreated_sandbox_keeps_the_events_forward_only` | the forward-only guard of `Tracker::enter` removed | `Sandbox started out of order` |
| E2 decide-time failure | `events::an_ended_sandbox_enters_the_ready_step_before_the_run_fails` | `stopped` does not enter the failing step | `left: Some(StepStarted(Provider)) right: Some(StepStarted(Ready))` |
| U8-A5 empty hook | `hooks::an_installed_hook_with_no_bytes_is_absent_and_never_runs` | presence is `is_some()` | `assertion failed: handle.runs(&sandbox_layout::hook_path(HookKind::Setup)).is_empty()` at `hooks.rs:184` |
| U8-A6 preflight only | `hooks::a_role_with_only_a_preflight_hook_never_writes_the_setup_marker` | every hook gets `touch_on_success` | `assertion failed: !has_file(&handle, &sandbox_layout::setup_done_marker(&launch_id(1)))` at `hooks.rs:224` |
| P8 role mismatch | `errors::plan_errors_are_typed_for_unsupported_modes_unknown_gateways_and_unregistered_projects` | role check replaced by `false` | panic at `errors.rs:45` with `None` for the plan error |
| R5 row C closure | `roles::role_findings_come_in_one_pass_and_load_path_returns_the_first` | one `ProviderHostClosure` per violation | panic at `roles.rs:434`; the findings list holds two `ProviderHostClosure([Overlap ...])` items |

Row C now uses two overlapping rules (`api.openai.com` and `chatgpt.com`) and expects one `ProviderHostClosure` with at least two violations that name both hosts. Dead code with no mutation to kill (the head-of-loop check, `known_sent`, the `quote` arms) is deleted instead of tested. With the yields in place all 109 tests of `jackin-session` and `jackin-testkit` pass.

## Rebase onto P4.4

The five P5.1 commits were rebased onto `feature/openshell-native` at 210ef3c6, which holds the P4.4 fix batches F3 (55be1a3d) and F1 (31d0a2d4). Base before: 204aa309.

Conflicts. Git reported no textual conflict. The semantic conflicts were found by the compiler, in `cargo check -p jackin-session -p jackin-testkit --all-targets`:

- `crates/jackin-session/src/plan.rs`. `SandboxCreate::new` takes a `ClosedPolicy` (F1-10), and the planner passed a `PolicyDocument`. Resolution below under "Adaptations".
- `crates/jackin-session/tests/launch/adr008.rs`. The `foreign` helper built a `SandboxCreate` from `rig.loaded.policy().clone()`. It now builds `ClosedPolicy::parse(&rig.loaded.policy().render()?)?`, the same path as the planner.

No other P5.1 file conflicts with the F1 and F3 changes. Checked: the testkit already used `ClosedPolicy` and `PolicyDocument::render() -> Result` on the P4.4 side (`crates/jackin-testkit/src/gateway.rs`); `MemFs::read_regular`, `ProcessError::OutputTooLarge` and `Read`, and `GatewayError::OutputTooLarge` are not matched by P5.1 code; `upload_project` is called only through `Gateway::upload_project`, so the `.git/config` sanitizer and `UnsupportedGitDirectory` need no change in the executor (any `Err` of the upload already fails at `Upload`, U8 decision 1 of the P4.4 fix-round list).

Adaptations.

- `launch_policy` now returns `(ClosedPolicy, Vec<LaunchWarning>)`. It builds the document (the role policy, plus the `docker_engine` rule when Docker is enabled), renders it, and parses the text with `ClosedPolicy::parse`, the only constructor. `SandboxDraft` stores the `ClosedPolicy`, and `SandboxDraft::complete` passes it to `SandboxCreate::new`.
- `PlanError::Closure(ClosedPolicyError)` is new (`#[from]`). A closure violation of the final policy, or a policy the authored grammar refuses, now fails `plan_launch` with a typed error. `render` errors map to the existing `PlanError::Policy`.
- `ClosedPolicy::parse` re-parses with `parse_authored`, so a generated rule name must not be reserved. `docker_engine` is not (`is_provider_rule_name` matches only the `_provider_` prefix), and the Docker plan tests pass.
- A closure violation cannot reach `plan_launch` through a `LoadedRole` today, because `RoleCatalog` already refuses it at load (U8-B1, group Closure) and the `docker_engine` host `host.openshell.internal` overlaps no provider host. The new error is therefore a type-level guard with no test; a test would need a `LoadedRole` that the catalog refuses.
- The assignments `self.known = Some(...)` in the executor's `create` and `join` (`launch/run.rs`) stay. They look dead, because the loop observes after both and `observe` sets `known` from the label query. They are dead only when that observation succeeds. If the `find_sandboxes` right after a successful create or join fails, `observe` returns before it sets `known`, and without the assignment `LaunchFailure.sandbox` is `None`. U8-A2 says `sandbox` is `Some` whenever the run created or joined a sandbox, and a `None` there drops the sandbox from the restore candidates. The first rebase deleted them and a review found the edge; the deletion is reverted and the rows under "Fix round after the rebase review" pin it.

Test expectations that changed. None. The 109 tests of `jackin-session` and `jackin-testkit` before the rebase pass unchanged, together with the P4.4 tests that now live in the same two crates (115 in all). The only test edit is the `adr008.rs` helper above, which changes how the fixture builds its input, not an assertion.

## Fix round after the rebase review

Decision. The `known` assignments in `create` and `join` stay (see the last bullet of "Adaptations"). Two rows pin them in `tests/launch/proofs.rs`. Both fail one `find_sandboxes` with the testkit fault `Fault::FailAfter { op: GatewayOp::Find, .. }` and assert `step == Sandbox` and `sandbox == Some(<planned name>)`. The executor calls `find_sandboxes` once before the image step, once after the image, once after the provider, and once after each later act, so the create row skips `FINDS_BEFORE_CREATE` (3) finds and fails the 4th, which is the observation right after the create. `Fault::HideUntilCreateConflict` makes the join path deterministic: the second launch of one ID sees no sandbox by label, meets `AlreadyExists`, joins by name (the 4th find), and the 5th find, the observation after the join, is the one that fails. The join row therefore skips `FINDS_BEFORE_CREATE + 1`.

| Row | Test | Mutation | Red line |
|---|---|---|---|
| U8-A2 create | `proofs::a_failure_right_after_create_still_reports_the_created_sandbox` | `self.known = Some(view.name)` removed from `create` | `left: None right: Some(SandboxName("jackin-platfor-ij4f"))` at `proofs.rs:152` |
| U8-A2 join | `proofs::a_failure_right_after_a_join_still_reports_the_joined_sandbox` | `self.known = Some(view.name.clone())` removed from `join` | `left: None right: Some(SandboxName("jackin-platfor-ij4f"))` at `proofs.rs:176` |

Each mutation was applied alone: the other row stayed green, and both rows pass with the assignments restored. The rows count finds, so a change to the observe-after-act shape of the loop moves `FINDS_BEFORE_CREATE`; the `step == Sandbox` assertion fails first in that case (the failure then lands at `Image` or `Provider`), so a stale count is not silent.

### U9-F3 (P4.4 fix batch F3), from rewrite/reconciliation/U9-F3.md

This file records where batch F3 of the P4.4 fix round differs from `rewrite/API.md`, and the preconditions P1 to P13 of the approved test plan (`P4.4-F3-test-plan.md`, with the orchestrator decisions on P2, P4, P9, P10 and P12). The orchestrator folds it into API.md at merge.

## Preconditions

- P1. Row order. The rows that need a cross-crate decision were written after the decision: P2 (rows 05), P9c (row 12), P10 (rows 13 and 14).
- P2. `ProcessError` gains two variants in `jackin-core`: `OutputTooLarge { program: Program, stream: OutputStream, limit: usize }` and `Read { program: Program, stream: OutputStream, message: String }`. `Read` is a failed read of a pipe of the child; it is not `Spawn`. The only exhaustive matches in the workspace are in `jackin-openshell/src/transfer.rs` (`process_error`, both variants map to `GatewayError::Process`). The testkit runner builds `ProcessError` values and never matches on them, so it needs no arm.
- P10. Option A. `FileSystem::read_regular(&self, root: &HostPath, file: &RepoPath) -> Result<Result<RegularFile, RegularFileProblem>, FsError>` in `jackin-core`. `RegularFile { bytes: Vec<u8>, mode: FileMode }` carries the raw permission bits (`mode & 0o7777`) of the one opened descriptor. `RegularFileProblem` is `Symlink`, `NotARegularFile` or `Missing`; a refusal is the inner `Err`, an I/O failure is the outer `Err`. `StdFileSystem` walks the components with `openat` below an open root descriptor: every ancestor with `O_DIRECTORY | O_NOFOLLOW`, the leaf with `O_NOFOLLOW | O_NONBLOCK`, and takes the type and mode from `fstat` of that descriptor (`File::metadata`). A failed open is classified with `fstatat(AT_SYMLINK_NOFOLLOW)`: a symbolic link is `Symlink`, a missing entry is `Missing`, a non-directory ancestor is `Missing`, a special file at the leaf is `NotARegularFile`. The root itself is trusted and followed. The rustix `fs` feature joins the root `Cargo.toml` and `jackin-core` depends on `rustix`. `MemFs::read_regular` reuses `FsOp::Read` for fault injection and call counting, because adding an `FsOp` variant breaks the exhaustive match in `jackin-testkit/tests/host.rs`. `jackin-image` exports `read_regular_file(fs, root, file) -> Result<RegularFile, EngineError>`, which maps the problem to `ContextProblem` and normalizes the mode to 0o755 (any execute bit) or 0o644. Callers: `stage_listed`, and the example for the Dockerfile. The U8 hook reader will call the same function.
- P3. Line cap. `LINE_LIMIT` is 65536 raw bytes per line, counted before the lossy decoding. A longer line is delivered as its first 65536 bytes decoded lossily, followed by the ASCII mark ` [truncated]` (a leading space); the rest of the line is discarded up to the next newline. A line of exactly 65536 bytes gets no mark. The line state belongs to one pump, so it is per stream. The tail buffer still takes the raw chunks and keeps the true raw suffix of 65536 bytes.
- P4. Whole bound. `WHOLE_LIMIT` is 16777216 bytes per stream. The bound is checked on every chunk: `==` passes and `>` fails. On overflow the pump ends, the runner kills the process group, reaps the child and returns `ProcessError::OutputTooLarge { program, stream, limit }`; the error carries no output. It applies to the stdout and the stderr of `run` without a sink and to both streams of `run_secret` (orchestrator decision: `Buffer::Secret` is bounded as well). Sink mode keeps the 65536-byte tail and no other bound.
- P5. Zeroization. `Buffer::Secret` holds `Zeroizing<Vec<u8>>`, the pump chunk is `Zeroizing<[u8; 8192]>` and the pump line is `Zeroizing<Vec<u8>>`, so drop glue covers success, error, timeout, cancel and a dropped future. No test is kept (impossible under `forbid(unsafe_code)`); the red/green reviewer runs the allocator probe from the plan.
- Read errors. `pump` returns `Result<(), StreamFault>`; a failed pipe read ends the run with `ProcessError::Read { program, stream, message }` instead of a short output with `Exited(0)`. No test is kept.
- P6. Success-path kill. After the leader is reaped and both pipes are closed, the runner kills the process group (ESRCH is ignored). A descendant that left the group with `setsid` escapes. Callers of `ProcessRunner::run` and `run_secret` in the tree: `openshell gateway list` (`jackin-openshell/src/discovery.rs`), the `openshell sandbox upload/download` calls (`jackin-openshell/src/transfer.rs`), and `docker buildx build`, `buildx inspect` and `image inspect` (`jackin-image/src/buildx.rs`). All are foreground commands; none relies on a surviving descendant.

## Line loss under load (found by the coordinator, root-caused in F3)

`streamed_output_is_lossless_ordered_per_stream_lossy_decoded_and_tailed_to_64_kib` once received 29104 of 30002 lines. Cause: after the child exited, `drive` started a 2 s wall-clock grace timer that ran while the pumps were still delivering buffered lines to a bounded sink. A consumer that is slower than the pipe (capacity-1 sink, loaded machine) kept the pumps busy for more than 2 s, so the timer fired, killed the group and returned with the unread pipe data and undelivered lines lost. The timer measured delivery time, not pipe idleness. Fix: the grace is now per read. Once the child has exited (a `CancellationToken` set by `drive`), each pump races its pipe read against `idle()`, which waits for the exit and then sleeps `DRAIN_GRACE`; the sleep restarts for every read, and time spent in `sink.send` is outside the race. A pipe that stays silent for 2 s after the exit still ends the pump (a descendant that holds the pipes), and the success path then kills the group. The `expire` branch and the `Sleep` state of `drive` are deleted. Reproduction before the fix (a throwaway test, not kept: `seq 1 30000`, capacity-1 sink, a collector that sleeps 1 ms per line after line 22000): 22766 of 30000 lines on the old runner, 30000 of 30000 on the fixed one. A kept test of this needs a plan row; it would be slow (20 s) and is proposed to the orchestrator as an open question.

- Row P44-F3-17 keeps that test: `a_slow_sink_after_the_child_exits_loses_no_line_and_the_idle_rule_still_ends_the_run` (`tests/process_runner/streaming.rs`). A 200-line script (12000 bytes) finishes writing and touches a done marker; a capacity-1 consumer waits for the marker, then stalls 3 s, longer than the 2 s grace. Phase A expects all 200 lines in order; phase B adds a silent pipe-holding descendant and expects the same lines, `Ok`, and no leftover process, so the idle rule still ends the run. Restoring the pre-8bf4e60e exit-anchored timer turns it red with `1 of 200 lines; first difference at index 1` (3 of 3 runs, 3.0 s); the fixed code passes in 5.95 s.

## Image records

- P7. Per-call directory guard and sweep. `BuildxEngine::build` holds a `CallDir` guard whose `Drop` runs the synchronous `std::fs::remove_dir_all` (a `Drop` cannot await), so a dropped `ensure_image` future removes the copy of the context. `BuildxEngine::new` sweeps `work_dir` once with std fs: an entry is a per-call directory only when its name is `<digits>-<digits>` and it is a directory or a symbolic link; it is removed when `kill(pid, 0)` returns ESRCH (EPERM counts as alive; pid 0 and unparsable pids are never swept). A symbolic link is removed as a link and never followed. `locks/`, lock files and every other entry are never deleted. A missing `work_dir` is not an error. `jackin-image` depends on `rustix` (workspace, features `fs` and `process`) for the liveness probe. The sweep and the guard use std fs, not the `FileSystem` boundary, as P7 says; an engine over `MemFs` therefore does not clean its in-memory call directory on drop.
- P8. Unreadable env file. `check_env` tests `-r` after `-f` and prints `0`, so `run_attempt` publishes `exited <attempt> 65` and writes `jackin-start: <path> is not readable` on stderr (any other bad file prints `is malformed at line <n>`). The test fails loudly when the file stays readable at mode 000, which is the case for root. Whether CI runs as root: not verified.
- P9. Key lock. (a) `BuildxEngine::hold` uses `FileSystem::lock` on `<work_dir>/locks/<sha256 of the tag>.lock`, which never matches the P7 pattern. The locks directory is created on demand and its files are never deleted. (b) `ensure_image` takes the construct-tag lock first, finds or builds the construct, releases it, then takes the lock of the final tag and keeps it to the end of the call, so the span covers the first `find` of the final tag, both builds and the delivery check. The wait polls every 100 ms, stops at once on cancel (`ImageError::Cancelled`) and gives up after 3600 s (`BUILD_TIMEOUT`) with `EngineError::LockTimeout { key }`, mapped to `ImageError::LockTimeout { key }` by `from_engine`. The maximum wait behind a live peer is therefore 3600 s; a hung peer that holds the lock for longer makes the waiter fail, not hang. `hold` uses `tokio::time`, so a `start_paused` test covers the limit (row below). (c) The construct lock is released before the final lock is taken, so two roles do not wait for each other during their role builds, and two roles that both miss the construct do not race on its tag. (d) `ImageEngine::hold(&self, key: &ImageTag, cancel: &CancellationToken) -> Result<Option<FileLock>, EngineError>` has a default body that returns `Ok(None)`, so `FakeEngine` compiles unchanged and takes no lock.
- P11. Derived getcap clause. One segment without ` && ` inside: `{ caps="$(getcap -r /)" || { echo "getcap failed in role image" >&2; exit 1; }; test -z "$caps" || { printf '%s\n' "$caps" | sed 's/^/file capability in role image: /' >&2; exit 1; }; }`. It captures stdout only. A failing `getcap` fails the build; stderr noise with exit 0 passes; each file with a capability is named on stderr before the build fails. Open risk for the live check: if real `getcap -r /` exits non-zero on harmless unreadable paths, every role build fails. The live evidence (exit code and stderr of real `getcap -r /` on a clean construct, a clean-role build, a hostile-role build) is the orchestrator's item P13.
- Test support. `work_entries()` in `tests/image_io/support.rs` ignores the `locks` directory; the assertions that use it still say that no per-call directory is left. `DockerModel::fresh_ids()` makes every build of a tag yield a new image ID.

## Example

- P12. `build_images` refuses a role tree that is dirty before any docker call. Dirty means `git status --porcelain -z --untracked-files=all` prints anything: tracked edits, staged changes, deletions and untracked files that `git ls-files -co --exclude-standard` would list (ignored files are not listed by either). The request keeps `RoleRevision::Clean`. The Dockerfile is read with `read_regular_file`, so a symbolic link is refused. The `--minimal` flag is deleted. The default work directory is `$XDG_STATE_HOME/jackin/build-images`, or `~/.local/state/jackin/build-images` when the variable is unset or empty; `--work-dir` overrides it. The directory is created with mode 0700 (parents with the default mode). An existing path that is not a directory, is a symbolic link, or has any other mode than 0700 is refused with an error naming the mode; nothing changes its mode.
- F3-16 manual checks (macOS, a temporary `XDG_STATE_HOME`, `PATH` holding only `git`): a tracked edit exits 1 with ` M a.txt` and an untracked file exits 1 with `?? new.txt`, both before any docker call; `--minimal` exits 1 with `unknown argument --minimal`; the clean role reaches the first docker call (`Spawn { program: Docker(ImageInspect) ... }` because docker is absent); the created directory has mode 700; after `chmod 755` the run exits 1 with `must be a directory with mode 0700, found mode 755`.

## Red/green addendum (round 2, reviewer verdict CHANGES)

Each row below was run red against the named mutant and green on the fixed tree.

- Row 05, per stream: `unstreamed_output_beyond_16_mib_per_stream_fails_with_output_too_large_and_stops_the_child` gained a run with 10 MiB on stdout and 10 MiB on stderr, no sink, expecting `Ok` with both streams whole. Mutant: the byte count shared by both streams of one call. Red: `Error: OutputTooLarge { program: Git, stream: Stderr, limit: 16777216 }`.
- Row 03, raw tail: `a_single_output_line_is_cut_at_64_kib_with_a_mark_and_the_next_line_arrives_intact` ends the long line with `z` and compares the whole stdout tail with the raw suffix (`...az\nnext\nlast`). Mutant: the tail fed with the capped bytes of each line. Red: `stdout tail is the raw suffix: 65536 bytes against 65536 expected; first difference at index 65525`.
- `LockTimeout`: `waiting_behind_a_live_peer_gives_up_after_3600_seconds_with_the_construct_key` (`start_paused`, `tokio` `test-util` added to the `jackin-image` dev-dependencies). A peer engine holds the construct tag lock; `ensure_image` returns `LockTimeout { key }` with the construct tag after at least 3600 s and less than 3700 s of paused time, and starts no build. The limit is a `timeout` of 3700 s of paused time, not `guarded` (its 10 s limit would fire first under auto-advance). Mutant: limit times 1000. Red: `Error: "no LockTimeout within 3700 s of paused time"` after 1.7 s. Mutant: limit divided by 60. Red: the `waited` assertion. Mutant: the mapped error loses the key. Red: `expected LockTimeout, got Err(Cancelled)`.
- Construct lock (P9c): `two_roles_that_both_miss_the_construct_build_it_once_and_neither_sees_a_moved_source`. A gate holds the first construct build; the second role (a different role name, same construct tag) must report lock contention and not reach its own construct build; after the release both calls are `Built`, the construct was built once (1 of 5 builds) and neither returns `SourceMoved`. The gate takes a tag prefix, so the existing role-build gate rows are unchanged. Mutant: the construct-tag lock removed. Red: `Error: "the second call also reached the construct build jackin.invalid/construct:abda10c023014b79"`.
- `MemFs::read_regular`: `memfs_read_regular_returns_bytes_and_mode_and_gives_the_typed_refusal_otherwise` (`jackin-testkit/tests/host.rs`): a regular file and a nested file return bytes and mode; a directory is `NotARegularFile`; a missing entry, a missing parent and a non-directory ancestor are `Missing`. Mutant: always `Symlink`. Red: `left: Err(Symlink)`, `right: Ok(RegularFile { len: 10, mode: FileMode(493) })`. Mutant: mode fixed at 0o644. Red: `left: ... mode: FileMode(420)`, `right: ... mode: FileMode(493)`.
- `RegistryFs::read_regular` (`jackin-openshell/tests/gateway/fakes.rs`) returns `FsError::Io` with kind `Unsupported` and the message `read_regular is unsupported by RegistryFs`. Nothing calls it.
- Unreadable env file message: `jackin_start_treats_an_unreadable_env_file_as_malformed` also asserts `is not readable` and no `malformed` on stderr. Against the old script it is red (stderr said `is malformed at line 0`).

## Residuals

- Row 13 reads the mode from the one opened descriptor. A mutant that reads the mode by path in a second call survives rows 13 and 14, because the swap in row 14 happens after `read_regular` returns, so no test can race the mode read. The single-descriptor rule is proven by reading `StdFileSystem::read_regular`, not by a test.

### U9-F1 (P4.4 fix batch F1), from rewrite/reconciliation/U9-F1.md

Records of the decisions and preconditions for batch F1 (`jackin-openshell`, `jackin-core` ids, the root manifest). Each precondition is written before the test that uses it. Source plan: the P4.4 F1 test plan and the orchestrator decisions on it.

## Closure matcher (F1-1, F1-4, F1-10, F1-11, F1-12)

### F1-1-P1. The matcher

- The matcher is `openshell_core::host_pattern`, from `openshell-core` at the git tag `v0.1.6`, the same source as `openshell-sdk`. The root manifest has a new workspace dependency `openshell-core`, and `jackin-openshell` is the only crate that names it.
- The xtask layers rule needs no change. `gateway_family("openshell-core")` matches the prefix `openshell`, and `repo_gates.rs` already has the manifest and source rows for `openshell-core`.
- `HostPattern::overlaps` calls `host_patterns_overlap`. The hand-written `labels`, `label_overlap` and `sequences_overlap` are deleted. An upstream error maps to "overlaps", which is fail-closed. It cannot happen for a `HostPattern`, because `HostPattern::parse` accepted the text through the same upstream parser.

### F1-1-P2. The host grammar

- `HostPattern::parse` first requires every label to use only ASCII letters, digits, `_`, `-`, and `*` (or to be exactly `**`) and refuses the wildcard shapes that the gateway refuses (P7.3). Then `openshell_core::host_pattern::HostPattern::new` decides, and the text is lowercased.
- Rejected and therefore a parse error: the empty text, any character outside the alphabet (whitespace, `{`, `}`, `[`, `]`, `^`, `!`, `?`, `\`, `/`, `:`, and non-ASCII characters), and an empty label (`a..b`, and a trailing dot as in `chatgpt.com.`).
- `api.openai.com/v1` is a parse error, because `/` is outside the alphabet.
- The U5 deviation "a partial `*` counts as a whole label" is replaced by the upstream rule: `x*.com` no longer overlaps `chatgpt.com`, while `*gpt.com` and `chat*.com` do.
- Bracket classes and `?` are outside the alphabet, so `[c]hatgpt.com`, `[a-z]hatgpt.com`, `chatgpt.co[m]`, `api.openai.co[m]`, `[a]pi.openai.com`, `[d]hatgpt.com`, and `chatgpt.co[n]` are parse errors. A `*` inside a label is decided by the upstream glob matcher.
- An IPv6 literal is a parse error, because `:` is outside the alphabet.

### F1-4-P1. Render error

- `yaml::render` returns `Result<String, PolicyError>`, and so does the public `PolicyDocument::render`. `with_rule` propagates the error. Nothing swallows a serialization error any more.
- Probe for a trigger: `openshell-policy` rejects every key it does not know, so only a free-form key can carry deep nesting into a parsed document. The probe used `network_middlewares.<m>.config`, which is free-form. The parse stops at nesting depth 61, below the upstream `ParseLimits.max_depth` of 64, and `noyalib::from_str` refuses deeper input with `recursion depth limit exceeded`. Every depth that parsed went through `with_rule` without a serializer error. No parsed document reaches the serializer with a depth the serializer refuses. The error path is therefore unreachable through a parsed document, and F1-4 has no test. The earlier probe under an unknown key was refused at parse time before depth mattered, so it did not test the serializer boundary.
- The one caller outside `jackin-openshell` is `crates/jackin-testkit/src/gateway.rs` (the policy hash). It maps the error to `GatewayError::Protocol`.

### F1-10-P1. ClosedPolicy

- `ClosedPolicy` is a tuple struct with a private field. It has one public constructor, `ClosedPolicy::parse(text)`, which parses with `PolicyDocument::parse_authored`, then requires `closure_violations` against the embedded Codex profile (`Agent::Codex.spec().openshell.chatgpt_profile`) with an empty `generated` list to be empty. It derives only `Clone` and `Debug`.
- The error is `ClosedPolicyError`: `Policy(PolicyError)` for a parse failure (a reserved `_provider_` key, port 0, an unparsable host) and `Violations(Vec<ClosureViolation>)` for an overlap.
- `SandboxCreate::new` takes a `ClosedPolicy`. `SandboxCreate::policy()` still returns `&PolicyDocument`. A raw `PolicyDocument` cannot reach `CreateSandbox`. The contract suite builds its sandbox through `ClosedPolicy::parse` as well.
- The type-level guarantee has no runtime test and no compile-fail test (nextest does not run doctests). The reviewer checks it by search: one public constructor, a private field, no `Default`, no `FromStr`, no `From<PolicyDocument>`.

### F1-11-P1. Name grammar

- `GatewayName` and `BuilderName` use the edge rule `Edge::First`: the first character must be ASCII alphanumeric. A leading `-`, `.` and `_` are rejected. The explicit rows are `-x`, `--help`, `-` and `.`.
- `.` matters beyond option injection. `Path::join("gateways").join(".")` folds to `gateways/mtls`, a different directory from the gateway's own.
- The last character is not restricted, so `a-` and `a.` stay valid.

### F1-12-P1. The noyalib feature

- `noyalib` supports `lossless-u64` as its own feature. `compat-serde-yaml` is `["lossless-u64"]`. The root manifest now names `lossless-u64`. `port_problem` relies on it for the `as_u64` branch.
- `compat-serde-yaml` is still enabled in the resolved graph, by `openshell-policy-schema` (upstream), so the feature set of the build does not change. `Cargo.lock` changes by one line: `openshell-core` in the dependency list of `jackin-openshell`.
- `cargo tree --offline -e features -i noyalib`, before: `jackin-openshell` is listed under `noyalib feature "compat-serde-yaml"`. After: `jackin-openshell` is listed under `noyalib feature "lossless-u64"` and `noyalib feature "std"`; `compat-serde-yaml` stays with the upstream crates only.
- No test: no behavior changes.

## Gateway discovery (F1-6)

### F1-6-P1. Variants and rules

- `resolve_gateway` accepts only an entry whose endpoint starts with the literal `https://` and whose auth is exactly `mtls`. The check runs before any certificate file is read. The SDK compares the scheme the same way (`starts_with`). Upper-case `HTTPS://` is refused.
- Two new `DiscoveryError` variants: `InsecureEndpoint { name, scheme: Option<String> }` and `UnsupportedAuth { name, auth }`. `scheme` is the text before `://` when it is at most 16 characters of letters, digits, `+`, `-` and `.`, and `None` otherwise (including an endpoint with no `://`). The error never carries the endpoint, so user information in an endpoint cannot reach a log.
- The scheme check comes first. An entry with both faults reports the scheme.
- `registered_gateways` is unchanged. It lists every entry, so `jackin doctor` can still show a non-mTLS entry.

## Contract suite and exec cap (F1-2, F1-5, F1-8)

### F1-2-P1. Ready failure

- `Ctx::ready` deletes the created sandbox on any `Err` from `wait_for` (an unavailable gateway, or the poll limit), ignores a failure of that delete, and returns the wait error. The delete sits on the shared `Err` return, so the poll-limit path and the error path use the same code. No separate test for the poll limit (60 s).

### F1-5-P1. Shell exit bound

- The event loop of `shell_exit_code` runs under `PROGRAM_TIMEOUT` (60 s). When it expires, the check fails with exactly `the shell never ended`, which the test pins, and `cleanup` still runs. The check does not add a field to `ContractFixture`.

### F1-8-P1. Exec output cap

- The cap is 16 MiB (`16 * 1024 * 1024`) inclusive, per stream, inside `exec_collect`. It covers `run_program`, `read_files`, `write_file` and `remote_digest`. A chunk that would take a stream above the cap ends the call with `GatewayError::OutputTooLarge { op, stream, limit }` and drops the response stream, so the supervisor sees the call cancelled and kills the process. A truncated output is never returned.
- `OutputTooLarge` carries `jackin_core::OutputStream` and is not retryable. It is a `GatewayError` variant of this crate. The runner-side `ProcessError` variants belong to batch F3 (F1-8-P2).

## Upload and download (F1-7, F1-8 sink, F1-9)

### F1-7-P1. Sanitizer rules

The sanitizer reads the host `.git/config` through `git config --no-includes -f <config> --list -z`, run by the `ProcessRunner` with `Program::Git`, `GIT_` removed from the environment, `GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=/dev/null`, null stdin and a 30 s timeout. There is no hand parser of the git config format: the only parsing is the NUL and newline framing of the `-z` listing. All names follow git's own parsing, because the listing comes from git:

- Section and variable names are lower-case in the listing, so `[CREDENTIAL]` and `[Credential]` are `credential`. A header with a key on the same line, an old dotted header such as `[credential.example]`, a continuation line of a value, and a quoted value with `#` or `;` reach the filter already decoded.
- An entry is dropped when its section is exactly `credential`, `include` or `includeif`; when its section is `http` and its variable is `extraheader` (this covers `http.extraheader` and `http.<url>.extraheader`); or when its subsection holds user information in a `scheme://` URL (the key itself would carry the secret, and a key cannot be rewritten without changing what it matches). `[credentials]` and `[includes]` stay.
- In the value of every other entry, the user information of the authority of each `scheme://` URL is removed, up to the last `@` of the authority, so a password that contains `@` leaves no tail. The authority ends at `/`, `?`, `#`, whitespace or a quote. Text after the authority, such as `path@v1`, stays. An scp-style `user@host:path` has no `://` and stays.
- An input that git cannot parse (for example `[remote "o]`, exit 128) is an error and yields no text. A listing that is not UTF-8, not NUL-terminated, or has a value with a carriage return is an error as well.
- The output is a fresh config: every value is written in double quotes with `\\`, `\"`, `\n`, `\t` and `\b` escaped, so git reads back the same value.

### F1-7-P2. Where the sanitizer lives

- `GitConfigEntry` and the pure filter `sanitize_entries(Vec<GitConfigEntry>) -> Vec<GitConfigEntry>` are public in `git_config.rs`. `sanitized_git_config(runner, config, cancel)` is the public git-backed reader and renderer in `transfer.rs`. The testkit fake can call both later (F2).

### F1-7-P3. Config path and environment

- The sandbox file is `<upload.dest>/.git/config`, written with the gateway file write at mode 0644 after both CLI uploads. The sanitized text is built before the first CLI call, so an unparsable config stops the upload before anything leaves the host.
- Both upload CLI calls now remove `GIT_` as well as `OPENSHELL_` (`upload_env_prefixes`). The download and the gateway list keep `OPENSHELL_` only.

### F1-7-P4. A `.git` that is not a directory

- `upload_project` returns `GatewayError::UnsupportedGitDirectory { path }` before any CLI call, git call or write when `<source>/.git` is not a real directory (a file as in a linked worktree or a submodule, a symbolic link, or missing). The export through the git common directory stays the open P5 trail item (F1-15).

### F1-7-P5. Failure after the `.git` upload

- A failure after the `.git` upload leaves the raw host config in the sandbox. The caller (P5 launch) must treat any `Err` from `upload_project` as a failed launch, must not start Codex, and writes the upload marker only after `Ok`.

### F1-8 sink half. Transfer CLI calls use the tail buffer

- Each transfer CLI call passes a line sink to `ProcessRunner::run`, drained by a concurrent receiver, so the runner keeps only the bounded 64 KiB tail of each stream (runner contract U7-B5(c)). The tests assert `has_sink` on the recorded request. The memory bound itself belongs to the runner tests of batch F3.

### F1-9-P1. Download

- The remote size comes from `/usr/bin/stat -c %s -- <path>` as a direct program (not through a shell). The backend keys on `command[0]`. The limit is 4 GiB inclusive. A larger size is `GatewayError::DownloadTooLarge { size, limit }` before the CLI call. A failing `stat` or a non-numeric size is `Transfer { op: Size, .. }`.
- The `stat` path is `sandbox_layout::STAT` in `jackin-core` (`/usr/bin/stat`), listed in `sandbox_layout::fixed()`, so the image check of batch F3 covers it. `transfer.rs` reads the constant from `sandbox_layout`.
- On any failure after the call starts (size, transfer, digest read, digest mismatch) `download_file` removes the local file, including a file that existed before the call. The digest is a corruption check only: it shows that the bytes arrived as the sandbox holds them, and it does not authenticate the sandbox.

## SSH main-process path (F1-3)

### F1-3-P1. The model and its limits

- `tests/gateway/ssh_server.rs` is a model of the sandbox supervisor, not interop proof. It is an in-process `russh` server (`russh::server::run_stream`) behind the in-process tonic `ForwardTcp`, which bridges the frames to a `tokio::io::duplex`. The tests call `GrpcGateway::open_terminal` end to end. No production item became more visible and no `cfg(test)` hook exists.
- Keys: two Ed25519 host keys from the fixed seeds `[0x2a; 32]` and `[0x55; 32]` (`Ed25519Keypair::from_seed`), so no RNG and no key file. The expected fingerprints are literals computed with `ssh-keygen -l -f` on the OpenSSH public key of each seed:
  - `[0x2a; 32]`: `SHA256:1Lt0B6m58n9i5yp9hABit7qhCJ/7aR7kRxt8QRziO8k`
  - `[0x55; 32]`: `SHA256:G0rarR9Wy+cqC2YbqeAoaVmUU9hm1OPuwLi2DueJAYU`
- The server is a dumb scripted actor. It accepts `auth_none` and the session channel, answers the `openshell-main` subsystem with success, and records the user, each `env` and `pty` request in order, each window change with its `Instant`, every data chunk, and EOF and close. It does not read detach keys. A test declares what the server sends after the subsystem (`after_subsystem`), what it sends once it has seen given bytes (`reactions`), and what it pushes later (`push`). One option stalls the subsystem reply.
- The in-process `CreateSshSession` returns the scripted fingerprint (right, wrong or empty) and a unique token, and records it. `RevokeSshSession` records the token. Both are recorded in the model.
- Fidelity sources for what the model sends: the supervisor `ssh.rs` of OpenShell v0.1.6 (`openshell-main` handling at lines 748 to 850: success first, then the notice, then exit status, EOF and close for a refusal; the `OPENSHELL_MAIN_READ_ONLY` env at 915; the input-enabled notice at 987) and the live capture in `evidence/P4.1-adapter-live.md` (c and e). The live Terminal group of the contract suite stays the fidelity oracle.
- Real time: the 250 ms lease window and a 600 ms nudge. No paused clock.
- Not covered: a host certificate. `check_server_key` answers `false` for a certificate. A certificate builder needs a signing key and a CA key, which would add test code beyond the plan's rule (no new dependency); the `Certificate => false` arm stays untested.
- A drop of `open_terminal` during `CreateSshSession` itself is not covered: the token is not known yet.
- The drop guard (`RevokeOnDrop`) revokes exactly once. The explicit revoke on the handshake failure path runs first and then disarms the guard. The guard is disarmed when the `Pump` takes the token.

### F1-3-P2. Viewer input

- Client-side dropping applies to `InputMode::Viewer` only. An Owner-mode attachment under a ReadOnly lease still forwards input, because the lease takeover (API.md "Input lease", contract check `main_read_only_takes_lease_after_owner_leaves`) depends on it.
- For a Viewer, the detach keys (Ctrl-D, Ctrl-P Ctrl-Q) and Ctrl-C under the ReadOnly lease end the session locally: the pump closes the channel and queues the pending end reason (`Detached` or `StreamClosed`). No input reaches the supervisor, so the supervisor does not need to answer a key that never arrived.

## Red/green fix round

Each row below was made red by the named mutant in the worktree and green on the clean tree. The mutant is reverted after each run.

| Item | Test | Mutant | Red line |
|---|---|---|---|
| G8 | `git_config::the_sanitized_config_follows_git_parsing_and_keeps_everything_else`, case "an at sign inside the password" | `authority.rfind('@')` to `authority.find('@')` | `an at sign inside the password: remote.o.url` |
| T5 | `transfer::upload_with_an_unparsable_git_config_fails_before_anything_leaves_the_host` | sanitized config built after the two CLI uploads | `assertion failed: runner.calls().is_empty()` |
| T6 symlink | `transfer::upload_fails_when_the_sanitized_write_fails_and_when_git_is_not_a_directory`, row "a symbolic link" | `symlink_metadata` to `metadata` | `called Result::unwrap_err() on an Ok value: ()` |
| T6 missing | same test, row "a missing entry" | `is_ok_and` to `map_or(true, ..)` | `a missing entry: Transfer { op: GitConfig, status: Exited(128), .. }` where `UnsupportedGitDirectory` was expected |
| M6f | `discovery::resolve_gateway_refuses_a_plaintext_or_non_mtls_entry`, row `admin:hunter2@h://x` | `scheme_of` without the length and charset guard | `admin:hunter2@h://x` left `Some("admin:hunter2@h")` right `None` |
| C4 | `attach_ssh::the_detach_keys_end_the_session_without_an_exit_status`, row "ctrl-p then x" | any byte after Ctrl-P detaches | left `[Attached, Lease(Owner), Ended(Detached)]` right `[Attached, Lease(Owner), Exited(Some(0)), Ended(StreamClosed)]` |
| C6 | same test, row "ctrl-c under an owner lease" | Ctrl-C closes under any lease | left `[Attached, Lease(Owner), Ended(StreamClosed)]` right has `Exited(Some(0))` before `Ended(StreamClosed)` |
| C9 | same test, row "ctrl-p x ctrl-q" | the prefix flag is never cleared by another byte | left `[Attached, Lease(Owner), Ended(Detached)]` right has `Exited(Some(0))` and `Ended(StreamClosed)` |
| E3 | `attach_ssh::the_host_key_pin_accepts_the_advertised_key_and_refuses_every_other` | handshake failure path does not disarm the guard | left `["token-id-2", "token-id-2"]` right `["token-id-2"]`; the test without the 150 ms wait passed under the same mutant |
| 3e | `attach_ssh::the_session_token_is_revoked_once_on_every_way_a_session_ends` | no revoke at the end of the pump | `detach: the session token was never revoked` |
| M11b | `ids::other_ids_reject_one_input_of_each_violated_class`, names `a-` and `a.` | `GATEWAY_LIKE` edge `First` to `Both` | `Error: BadEdge { kind: "gateway name" }` |
| F1-9 layout | `paths::sandbox_layout_matches_the_documented_literals` | `STAT` replaced by a second `SHA256SUM` in `fixed()` | `assertion left == right failed: left: 10 right: 11` |
| F1-9 layout | `transfer::a_failed_download_leaves_no_local_file_and_the_size_limit_is_inclusive` | `STAT` set to `/bin/stat` | `called Result::unwrap() on an Err value: Protocol { message: "the exec stream ended without an exit event" }` |
| F1-5 | `contract::shell_exit_code_cannot_hang_the_run` | the timeout text changed to `no end` | left `Err("no end")` right `Err("the shell never ended")` |

### U9-F2 (P4.4 fix batch F2), from rewrite/reconciliation/U9-F2.md

This file records the decisions and the preconditions for batch F2 (`jackin-testkit`, the four shared contract checks of F1-16, and the `FakeEngine` call sites in `jackin-image` tests). Each precondition is written before the test that uses it. Sources: the P4.4 fix decisions (F2-1 to F2-13 and F1-16), the approved F2 test plan with the orchestrator decisions at its end, the F1 test plan rows P44-F1-16a to 16d, and the live evidence `P4.1-adapter-live.md`, `P4.4-live.md` and `P0.6-panes.md`. `API.md` is not edited here. API.md section 7 (U6) and `U8.md` state the rules this file supersedes.

## Fake gateway rules

### F2-P1. Base and numbering

- The base is `3f17ce17`, the P5.1 branch rebased onto `feature/openshell-native` with the F1 and F3 fixes. P5.1 already edited `gateway.rs`, `state.rs`, `exec.rs`, `files.rs`, `engine.rs` and `tests/gateway/main.rs`.
- `U8.md` is P5.1's numbering. This file is `U9-F2.md` and its identifiers are `F2-P*`.
- TK-2 (`StaleSupervisor` counts observations) and TK-3 (`FailAt` and `CancelAt` fire at call entry) interact with F2-7. The strike still happens at call entry, before the lookup and before any observation. Consequence for TK-2: every call that reads the phase is one observation, so a test that counted observations must count its own reads. One existing test changes for this reason (see "Changed expectations").

### F2-P2. Oracle per behavior

| Behavior | Oracle | Status |
|---|---|---|
| `open_terminal(MainProcess)` on `Completed` or `Error{Some(_)}` is `Ok`, events `[Attached, Ended(Refused(..))]` | `P4.1-adapter-live.md` e.3: "open_terminal(main) on it -> Ok; events=[Attached, Ended(Refused("openshell: cannot connect to the canonical main process: canonical main process already finished"))]" | live for `Error{7}` |
| The same for `Completed` | e.3 is captured on `Error{7}` only. `Completed` has the same supervisor path (U6, the `MainProcessCompleted` condition) | source-derived |
| `open_terminal(MainProcess)` on `Stopped` is `NotReady{Stopped}` | e.5: "run_program and open_terminal(main) on Stopped -> NotReady { phase: Stopped }" | live |
| `open_terminal(MainProcess)` on `Starting`, `Provisioning`, `Error{None}` is `NotReady` | `Starting` and `Provisioning` have no main session to refuse from (e.5 for `Stopped`). `Error{None}` has no capture | unverified, stays `NotReady` |
| `run_program` and shell terminal on an ended sandbox are `NotReady{phase}` | e.3 | live |
| `delete_provider` while a sandbox lists the provider is `FailedPrecondition` naming the provider and the sandbox | b.10: "delete_provider while attached -> Err(FailedPrecondition { message: "provider 'jackin-p41a-codex' is attached to sandbox(es): jackin-p41a-ib8t" })" | live for a Ready sandbox |
| Any sandbox that lists the provider counts, in any phase | the fake rule | unverified (b.10 had a Ready sandbox) |
| `delete_provider` after the sandbox is gone is `Ok`, and `Ok` again; state `Absent` | b.12 | live |
| `set_power(Stop)` on `Stopped` is `Ok(Stopped)` | e.5: "Stop again -> Ok(Stopped)" | live |
| `set_power(Stop)` on `Completed` or `Error{Some}` is `MainProcessFailed`; `Start` is `FailedPrecondition` and leaves the phase `Starting` | e.4 | live, unchanged |
| `Stop` on `Error{None}` with `Provisioning::Failed` fails with the status class `Internal`, which the adapter maps to `Protocol`; the phase is unchanged | `P4.4-live.md`, "Stop, twice: Internal error, phase unchanged": `code: 'Internal error', message: "stop sandbox failed: persist stop marker failed: No such file or directory (os error 2)"`; `grpc/status.rs` maps every code outside its list to `Protocol{message}` | live |
| `Start` on `Error{None}` with `Provisioning::Failed` is `FailedPrecondition`; the phase stays `Error` | `P4.4-live.md`, "Start, twice: FailedPrecondition, phase unchanged (not Starting)" | live |
| `delete_sandbox` on a provisioning-failed sandbox succeeds | `P4.4-live.md`, "Delete: accepted" | live |
| `upload_project` on an absent sandbox is `Transfer{op: UploadFiltered}` | `P4.4-live.md`, "Upload to an absent sandbox": both CLI calls exit 1 with "sandbox not found" at once; `transfer.rs` maps the first non-zero exit to `Transfer{UploadFiltered}` | live |
| `upload_project` on a sandbox that is not Ready is `Transfer{op: UploadFiltered}` | the same code path (`run_cli` maps any non-zero exit). No capture of the CLI on a stopped sandbox | source-derived |
| A lone Ctrl-Q (no Ctrl-P before it) is forwarded | the fake rule; P0.6 has no capture of it | source-derived |
| Ctrl-P Ctrl-Q detaches, a lone Ctrl-P is held, Ctrl-P then another byte forwards both | `P0.6-panes.md`: "`0x10` is held until the next byte. `0x10 0x11` detaches. Any other byte releases the held `0x10` first." and "CHECK held Ctrl-P is released with the next byte: PASS" | live |
| A read-only attachment and Ctrl-P Ctrl-Q | the adapter's `Watch::sent` ends any attachment on the pair; the supervisor side for a read-only attachment is not captured | unverified, not asserted |

### F2-P3. Download (amendment A1)

- F1-9 makes the real `download_file` read the remote size through `exec` first. On an absent sandbox that is `NotFound{Sandbox}` and on a stopped one `NotReady` (n.1 and e.3). The fake's download therefore keeps `NotFound` and `NotReady`. F2-5 covers upload only.
- F2-13: on any download failure, including a digest mismatch, the fake removes the local file. This mirrors `transfer.rs::download_file`, which removes the local file when `fetch` returns any `Err`. Before this change the fake left the corrupt file, because it wrote the file before it compared digests.

### F2-P4. Observation order and handle accessors

- `delete_provider` strikes first as well: an injected `Provider` fault fails the call even while a sandbox lists the provider, and a fault that has fired is gone, so the next call gets the attached-check refusal.
- Every gateway call that reads the phase runs in this order: strike (fault and trigger), lookup (`NotFound`), observe, phase read. The calls are `run_program`, `read_files`, `write_file`, `upload_project`, `download_file`, `open_terminal` and `set_power`. `find_sandboxes` and `sandbox_detail` already observed.
- A failed call (an injected error, `NotFound`) does not observe, so it does not settle a `ProvisionFor` window.
- `SandboxHandle` accessors (`phase`, `file`, `files`, `attachments`, `main_input`, `runs`, `supervisor`) never observe.
- This supersedes API.md section 7 (U6) G4 ("observations (find and detail)"): an observation is any call listed above plus `find_sandboxes` and `sandbox_detail`.

### F2-P5. Main attach generation (F2-4)

- The main process keeps a generation number. A restart (`Stop` then `Start`) creates a new `Main` with the next generation, and attachment ids restart at 1. A terminal pump holds a `Seat` (generation and id). `forget` and `command` ignore a seat of another generation. Before this change a pump that ended late released the attachment that a new attach had received under the same id.
- The main pump's `select!` is `biased` and polls the command channel first. A pump that was blocked on a full event channel therefore delivers every command it still holds before it reads the queued `Ended`, so a late command of the old generation reaches `Main::command` every time, and the test for it does not depend on the order a random `select!` picks. The test helper `flood` yields between inputs, so the pump forwards one queued event per input and the event channel and then the command channel fill; without the yield, the command-first pump drains the commands as fast as the test sends them and the channels never fill.

### F2-P6. Process, git and workdir (F2-9)

- One builder makes every host command of the fake: `HostProgram::command()` in `crates/jackin-testkit/src/exec.rs`. It starts `/bin/sh`, `/usr/bin/git` or `/usr/bin/printenv` with a cleared environment and `PATH=/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin`. The gateway program runs, the shell terminal, the fake upload's `git ls-files` and `TestRunner`'s git use it. `spec.env` or `run.env` is applied after the clear.
- Deviation from the real runner: `TokioProcessRunner` inherits the environment and removes the named prefixes; the fake starts from nothing. `env_remove_prefixes` of a git spec has no effect in `TestRunner`, because there is nothing inherited to remove.
- The `/sandbox` mapping in `run_program` arguments is textual and applies to every argument, not only to a whole argument. An occurrence of `/sandbox` maps to the sandbox root when the byte before it is absent or is not an ASCII letter, digit, `_`, `-`, `.` or `/`, and the byte after it is absent or is not an ASCII letter, digit, `_`, `-` or `.`. So `>/sandbox/o.txt`, `"/sandbox/q 1.txt"`, `'/sandbox/q2.txt'`, `V=/sandbox/v` and `cd /sandbox;` map; `/sandbox2/x`, `/opt/sandbox/y`, `/sandboxed` and `/sandbox.bak` do not. The replacement is the host root path without quoting, which is safe for the temporary directory names the fake makes.
- A `workdir` outside `/sandbox` (a program run or a shell terminal) maps to `<sandbox temporary directory>/<path>` and is created on demand. Evidence: `P4.1-adapter-live.md` p.8 "env/workdir -> "a'b c:/tmp:ubuntu" exit=0" and p.9 "env P41A_SHELL=yes and workdir /tmp reached the shell". The real adapter accepts the workdir; the fake used to reject it with `InvalidArgument`.
- Recorded fake limits: `pwd` in a script shows host paths; the shell is `/bin/sh`, not bash; a path outside `/sandbox` other than a workdir is not mapped in arguments.
- `install_file` (P5.1 TK-4) is a separate map and does not change.

### F2-P7. Upload (F2-5)

- The fake `upload_project` refuses an absent sandbox and a sandbox that is not Ready with `Transfer{op: UploadFiltered, status: Exited(1), tail: "sandbox not found"}`. The fault strike runs first, so an injected error still wins. No sandbox appears.
- The fake builds the sanitized git config the way the adapter does: it calls the public `sanitized_git_config` with a `TestRunner`, which runs the real `git config --no-includes -f <config> --list -z` with a cleared environment. It does this before it copies anything, and it writes the result to `<dest>/.git/config` with mode 0644 after it copies the `.git` tree. A `.git` that is not a directory (a linked worktree or a submodule) gives `UnsupportedGitDirectory`, as in the adapter. This keeps the fake short and means a test sees the same config text that a real sandbox would hold.
- The copy of one file goes through a temporary name and a rename. With a plain remove and copy, two uploads of one project into one sandbox could fail with a permission error on a read-only git object that the other upload had just created.
- The test for this runs 8 uploads of one project (a git history with 41 files, so many read-only objects) to one sandbox at the same time, once. No interleaving of the two file steps can be forced from outside without a hook in the copy, and a hook is not added to the fake for one test. The count is the justification for the single run instead of a loop: under remove-then-copy it failed in 10 of 10 runs with `Permission denied`, and with the committed code it passed in 20 of 20 runs.

### F2-P9. Ctrl-P hold rule (F2-12)

- The fake keeps one flag per attachment: a Ctrl-P (`0x10`) is held until the next byte. The held byte is released in front of the next byte, unless the next byte is Ctrl-Q (`0x11`), which detaches the attachment. Ctrl-D keeps precedence over the whole chunk, as before. Bytes in front of the pair in the same chunk are forwarded by the lease rules, then the attachment detaches.
- A lone Ctrl-Q is forwarded unchanged. Source-derived: `P0.6-panes.md` states only that any other byte releases the held `0x10`; it has no capture of a Ctrl-Q that follows no Ctrl-P.
- The held byte is not echoed and not counted in `main_input` until it is released.
- This supersedes API.md section 7 (U6) G6 ("Ctrl-P Ctrl-Q not modelled").
- Recorded fake limits, with no change: `Notice` events are not produced by the fake main; the main PTY echo exists only for the `exec cat` main; the phases `Stopping`, `Deleting`, `Suspended` and `Unknown` are P5.1 TK rows.
- Recorded fake limits (P4.2 confirmation, b94d8dbc): `write_file` outside `/sandbox` gives `PermissionDenied` where live p.7 gives `Protocol("writing ... failed")`, and `read_files` outside `/sandbox` gives `None` unless `install_file` was used; supervisor transitions happen inside `write_file` where the real `jackin-start` polls every 0.5 s, so a caller must poll, never read once after writing `run/start`; the fake writes no `jackin-start: codex exited with code N` line into the main PTY. Scheduled for P5.2 (export consumes them): `download_file` honours its cancel token, and a missing remote file gives the adapter's `Transfer { op: Size }`, each pinned by a new shared contract check with a live run.

## Clock and process fakes

### F2-P8. Clock wake rule (F2-8)

- `FakeClock::sleep(d)` registers the deadline `now + d` at its first poll. Every poll yields to the scheduler once. A sleeper returns, adds its duration to `slept()` and moves `now` to its deadline when one of two things holds: another sleeper already moved `now` past its deadline, or it has yielded at least 64 times (`WAKE_YIELDS`) and its deadline is the earliest of the live sleepers. A sleeper with a later deadline keeps yielding.
- The rule is deterministic and uses no wall time. A lone sleeper adds exactly its duration, so sequential behavior and the `poll_until` sums are unchanged. Two concurrent 5 s sleepers move `now` by 5 s and `slept()` lists both. A 3 s and a 5 s sleeper wake in deadline order whatever the poll order: the 3 s sleeper sees `start + 3 s` and the total is `start + 5 s`.
- A sleeper that is dropped before it wakes is removed from the live set and leaves no trace in `now()` or `slept()`. `poll_until` drops its `sleep(remaining)` when the probe wins the race, so a probe that yields up to 63 times and then returns `Ready` finishes with `now()` unchanged and an empty `slept()`.
- The bound of 64 yields is a count of scheduler turns, not a measure of work. A probe that waits on something that does not yield (the blocking pool behind `tokio::fs`, a real process) can lose the race against a virtual timeout. Tests that race such work against a fake sleep must not rely on the work finishing first. `jackin-session`'s executor races `poll_until` against fake gateway calls, which yield; the `jackin-session` suite is part of the gate on this merge.
- This supersedes API.md section 7 (U6) G8 ("yields once", "There are no pending sleepers").

### F2-P10. Scripted runner (F2-10)

- A token cancelled before the call gives `Cancelled{program}` before any expectation is chosen, in `run` and in `run_secret`, and in `TestRunner` for git. The real runner does the same before it spawns (`jackin-session/src/host.rs`, `execute`). The effect of the reply does not run, the reply is not consumed, and `seen()` does not list the call: `seen()` lists calls that reached the script.
- When a line sink is present, `stdout` and `stderr` of a scripted reply are cut to their last 65,536 bytes (a byte cut, not a line cut), as the live runner does with its tail buffer. The sink still gets every line. Without a sink the whole text is returned. The constant is a copy of `TAIL_LIMIT` in `jackin-session/src/host.rs`, which is private; it is duplicated until a later change exports it. The parity test runs the live runner and the fake on the same lines, so a drift fails there.

## Fake engine

### F2-P11. Engine script and cancel (F2-10, F2-11)

- `FakeEngine::at` takes one `EngineMoment`, and the moment decides which effects it accepts. `AfterFind{tag, retag}` retags once after a `find` of that tag. `FindFails(tag)` makes one `find` of that tag fail with `DigestUnreadable`, the class the buildx engine returns when the image id cannot be read. `CheckBuilderFails` makes one `check_builder` fail with `BuilderUnreadable`. `BuildFinished{stage, effect}` takes a `BuildEffect`: `Retag(Retag)` or `Cancel`. `Cancel` after a find has no way to be written, so it cannot silently do nothing. This replaces the pair `at(EngineMoment, EngineEffect)`. `EngineEffect` is deleted, and the `ensure.rs` call sites of `jackin-image` use the new form (F2-11, proof by compile probe, not by a test).
- A scripted failure fires once and then clears. A failure of one tag does not touch another tag.
- `build` returns `EngineError::Cancelled` for a token that is cancelled before the call. The check runs before the queued `fail_next` is consumed, before any `Line` event and before the image is stored, as `buildx.rs` does before it starts a process. The `BuildFinished` effect `Cancel` keeps its meaning: the build is done, the image is stored, `build` returns `Ok(id)` and the token is cancelled afterwards (U7-C6).
- `FakeEngine` `Line` events stay with P5.1 TK-9.
- The F2-11 probe: `engine.at(EngineMoment::AfterFind { tag, retag: BuildEffect::Cancel })` fails `cargo check` with `expected Retag, found BuildEffect`. At `3f17ce17`, `engine.at(EngineMoment::AfterFind(tag), EngineEffect::Cancel)` compiles and does nothing.

## Shared contract checks (F1-16)

### F2-P12. The four checks and the in-process server

- Sequencing: F1-16 lands with the fake fixes, so the testkit contract test is never red between batches. The live evidence for the upload check is `P4.4-live.md`, "Upload to an absent sandbox": both CLI calls exit 1 at once with "sandbox not found" and do not hang. The check does not need a live run to be written; a live run of the four checks is still owed (see "Not verified").
- Counts: `CORE_CHECKS` 13 to 15, `TRANSFER_CHECKS` 2 to 3, `TERMINAL_CHECKS` 5 to 6. The new names sit where the group runs them: `delete_provider_while_attached_is_refused` after `provider_put_and_delete`, `stop_on_stopped_is_ok` after `power_transitions`, `upload_absent_sandbox_is_a_transfer_error` last in Transfer, `main_ended_is_refused` last in Terminal. The first check of each group is unchanged, so the fault tests that hit "the first call" still hit the same check.
- `stop_on_stopped_is_ok` (P4.1 e.5): a Ready sandbox, Stop, wait for `Stopped`, Stop again must be `Ok` with phase `Stopped`. The sandbox is deleted on every path.
- `delete_provider_while_attached_is_refused` (P4.1 b.10, b.12): clear the contract profile, sync it, put the contract provider, create a sandbox that lists it (not waited to Ready), then `delete_provider` must be `FailedPrecondition` and the provider must still be `Present`. Cleanup order, which matters because the next run starts with `clear_contract_profile`: delete the sandbox, poll until `find_sandboxes(Name)` is empty, then delete the provider and the profile. The check returns the cleanup error when the body passed. The Codex profile of b.10 is not used; the contract profile is, so a live run is owed.
- `main_ended_is_refused` (P4.1 e.3): create the `exit 7` sandbox, wait for the ended phase, open the main terminal as Owner. The events must be exactly `[Attached, Ended(Refused(_))]` and the session must close. The read is bounded by `PROGRAM_TIMEOUT`. A failing open propagates its text, so the existing forward fault shows "connection limit" in this check too.
- `upload_absent_sandbox_is_a_transfer_error`: no sandbox is created. A fresh name, `upload_project` must be `Err(Transfer{op: UploadFiltered, ..})`. The op is the live one (the first CLI call fails).
- In-process server (`crates/jackin-openshell/tests/gateway`): `delete_provider` refuses with `FailedPrecondition "provider '<n>' is attached to sandbox(es): <names>"` when a sandbox of the workspace lists it; `create_ssh_session` and `forward_tcp` accept `Error` and `Completed` sandboxes (e.3) and keep refusing `Stopped` (e.5); Stop on `Stopped` was already `Ok`. Two switches build the red variants: `refuse_stop_on_stopped` and `delete_attached_providers`. A script without the refusal notice builds the third red variant (the adapter grants the lease); a runner that exits 0 builds the fourth.
- Limit: on the in-process server only the Core group passes in full, as before. For the Terminal and Transfer groups the tests assert the outcome of the new check, because the other checks need a PTY model and an upload target that the in-process server does not have. The testkit contract test runs all three groups in full on `FakeGateway`.

### Superseded statements

- API.md section 7 (U6) G5: `Stop` on `Stopped` is `FailedPrecondition`. Now `Ok(Stopped)`.
- The notes line "upload/download NotFound/NotReady". Now upload is `Transfer{UploadFiltered}` on an absent or not Ready sandbox; download keeps `NotFound` and `NotReady`.
- API.md section 7 (U6) G8 (sleep) is superseded in the clock section below.

### Changed expectations in existing tests

- `crates/jackin-testkit/tests/gateway/ended.rs`: `assert_not_ready` listed `open_terminal(main)` among the `NotReady` calls. Main now attaches and is refused on `Completed` and `Error{Some}`, and stays `NotReady` on `Starting`. The test name and the G6 note change with it.
- `crates/jackin-testkit/tests/gateway/lifecycle.rs`, `providers_profiles_and_detail_follow_the_gateway_contract`: it deleted the provider while the sandbox existed. It now expects `FailedPrecondition` first, then deletes the sandbox, then the provider (b.10, b.12).
- `crates/jackin-testkit/tests/gateway/faults.rs`, `provision_for_keeps_provisioning_for_exactly_n_observations`: the `run_program` call after `create` is now observation 1.
- `crates/jackin-testkit/tests/gateway/triggers.rs`, `a_stale_supervisor_window_keeps_the_old_state_for_n_observations_after_power_start`: `settled_state` observed twice (a find, then `read_files`). It reads the state file through the handle now, so each call is one observation.

## Other crates

- `crates/jackin-image/tests/image_io/ensure.rs`: the `FakeEngine::at` call sites use `EngineMoment::AfterFind { tag, retag }` and `EngineMoment::BuildFinished { stage, effect }`; the `retag` helper returns a `Retag`. No expectation changed and no assertion was touched.
- `crates/jackin-session/tests`: no expectation changed. The P5.1 session suite passes on the merged clock, observation and upload changes.

## Red and green record

Each row was proven red by reverting only the named source hunk with the tests kept (the mutated source is restored after each run). The line is the disagreement the failing run printed.

| Row | Test (file under `crates/jackin-testkit/tests` unless noted) | Mutation | Red line |
|---|---|---|---|
| P44-F2-01 | `gateway/ended.rs` `an_ended_main_process_refuses_the_main_attach_and_blocks_every_other_call_for_error_and_completed`, `a_main_attach_is_not_ready_on_a_stopped_sandbox` | main through `exec_cell`; `StreamClosed` for `Refused`; attach in every phase but `Provisioning` | `Error: NotReady { phase: Error { exit_code: Some(7) } }`; `Error: "Owner: [Attached, Ended(StreamClosed)]"`; `Owner: None` |
| P44-F2-02 | `gateway/lifecycle.rs` `delete_provider_is_refused_while_a_sandbox_lists_it_and_ok_after`, `providers_profiles_and_detail_follow_the_gateway_contract` | unconditional delete; always refuse | `Error: "Ok(())"`; `FailedPrecondition { message: "provider 'jackin-other-codex' is attached to sandbox(es): " }` |
| P44-F2-03 | `gateway/power.rs` `stop_on_a_stopped_sandbox_is_ok_keeps_the_files_and_ended_sandboxes_still_refuse_it` | the arm gives `FailedPrecondition`; the arm moves to `Starting` | `Error: FailedPrecondition { message: "x" }`; `left: Starting`, `right: Stopped` |
| P44-F2-04 | `gateway/terminal.rs` `a_late_ending_attachment_of_the_old_generation_does_not_release_or_feed_the_new_attachment`, `a_session_dropped_while_its_channels_are_full_leaves_no_attachment` | unscoped `forget`; unscoped `command` (red in 10 of 10 runs with the `biased` pump; 65 of 70 before); never `forget` | `left: 0`, `right: 1` (attachments); `timed out waiting for the new attachment input to be [120]; saw [97, 97, 97, ..., 120]` (the held inputs of the old attachment, then the new one); `timed out waiting for the attachment to be released to be 0; saw 1` |
| P44-F2-05a | `gateway/transfer.rs` `upload_copies_the_files_and_a_sanitized_git_dir_and_refuses_an_absent_or_stopped_sandbox_as_a_transfer_error` | through `exec_cell`; `UploadGit`; raw git config | `Err(NotFound { what: Sandbox(..) })`; `Err(Transfer { op: UploadGit, .. })`; the raw `[core]` config in the failed `!config.contains(SECRET)` |
| P44-F2-05c | `gateway/transfer.rs` `concurrent_uploads_of_one_project_to_one_sandbox_all_succeed_and_leave_the_project` | `replace_file` as remove then copy (no temporary name, no rename) | `Error: Protocol { message: "Permission denied (os error 13)" }` in 10 of 10 runs; green in 20 of 20 runs with the committed code |
| P44-F2-05b, F2-13 | `gateway/transfer.rs` `a_failed_download_leaves_no_local_file_and_a_good_one_writes_it` | no cleanup of the local file | `assertion failed: !tokio::fs::try_exists(local.as_path()).await?` |
| P44-F2-06 | `gateway/power.rs` `stop_and_start_on_a_provisioning_failed_sandbox_follow_the_p4_4_capture` | `Stop` through the `Error{..}` arm; `Start` through the `Error{..}` arm | `Err(MainProcessFailed { sandbox: .. })`; `left: Starting`, `right: Error { exit_code: None }` |
| P44-F2-02b | `gateway/lifecycle.rs` `a_provider_fault_is_struck_before_the_attached_check_and_clears` | `strike(Provider)` moved after the attached check in `delete_provider` | `Err(FailedPrecondition { message: "provider 'jackin-work-codex' is attached to sandbox(es): jackin-work-cc05" })` where the injected `Protocol` error was expected |
| P44-F2-07 | `gateway/power.rs` `every_phase_reading_call_is_one_observation_that_settles_a_provisioning_window`, `a_start_settles_on_the_first_call_that_reads_the_phase_and_the_files_are_kept`, `a_failed_call_does_not_settle_and_handles_and_absent_sandboxes_never_observe` | settle without observe; no observe in `set_power`; no settle in upload; handle observes; observe before strike | `Program call 3: Err(NotReady { phase: Provisioning })`; `Stop call 3: Err(FailedPrecondition ..)`; `Upload call 3: Err(Transfer { op: UploadFiltered, .. })`; `left: Ready`, `right: Provisioning`; `Ok(ProgramOutcome { exit_code: 0, .. })` |
| P44-F2-08 | `host.rs` `concurrent_sleepers_wake_at_their_own_deadlines_and_do_not_add_their_durations`, `a_dropped_sleeper_leaves_no_trace_and_a_lone_race_still_moves_time` | `+=`; wake after one yield; a dropped sleeper advances time; wake in poll order | `left: Timestamp(1767225610)`, `right: Timestamp(1767225605)`; `Err(Timeout { after: 30s })`; `left: Timestamp(1767225605)`, `right: Timestamp(1767225603)` |
| P44-F2-09a | `host.rs` `test_runner_starts_git_with_a_cleared_environment_plus_the_spec_env` | inherited environment; `spec.env` dropped | `inherited: ["CARGO_MANIFEST_DIR=..", "HOME=.."]`; `the spec env is missing` |
| P44-F2-09b | `gateway/exec.rs` `shell_scripts_map_every_sandbox_path_at_a_path_boundary_and_leave_look_alikes_alone`, `a_program_workdir_outside_sandbox_is_accepted_and_stays_inside_the_sandbox` | whole-argument mapping only; no boundary; reject the workdir; host `/tmp` | `printf a >/sandbox/o.txt: ProgramOutcome { exit_code: 1 .. }`, `left: 1`, `right: 0`; the look-alike echo prints host paths (`left: "/var/folders/.../jackin-fake-sandbox-dMmLJw/sandbox2/x /opt/sandbox/y /var/folders/.../jackin-fake-sandbox-dMmLJw/sandboxed\n"`, `right: "/sandbox2/x /opt/sandbox/y /sandboxed\n"`, as text, not bytes); `Protocol { message: "the fake maps only /sandbox" }`; `assertion failed: !tokio::fs::try_exists(..)` |
| P44-F2-09c | `gateway/terminal.rs` `a_shell_workdir_outside_sandbox_is_a_per_sandbox_directory_and_never_the_host` | host `/tmp` | `left: Some(Some(0))`, `right: Some(Some(9))` |
| P44-F2-10a | `host.rs` `a_pre_cancelled_call_is_cancelled_before_any_reply_is_chosen_or_any_effect_runs` | ignore the token; check after the reply is chosen | `Ok(ProcessOutput { status: Exited(0), stdout_len: 3, .. })`; `assertion failed: runner.seen().is_empty()` |
| P44-F2-10d | `host.rs` `a_pre_cancelled_git_call_is_cancelled_before_git_is_started` | the pre-cancel check removed from `TestRunner::run_git` | `Err(Spawn { program: Git, message: "No such file or directory (os error 2)" })` where `Cancelled { program: Git }` was expected (the cwd does not exist, so only a started git can fail this way) |
| P44-F2-10b | `host_runner.rs` `the_scripted_runner_returns_the_same_tail_as_the_live_runner_for_a_long_output` | no tail; tail by whole lines | `left: 505000`, `right: 65536`; `left: 65448`, `right: 65536` |
| P44-F2-10c | `engine.rs` `a_pre_cancelled_build_is_cancelled_before_it_fails_prints_or_stores_anything`, `a_scripted_find_failure_hits_one_call_of_one_tag_and_then_clears`, `a_scripted_builder_check_failure_hits_one_call_and_then_clears`, `a_cancel_at_the_end_of_a_build_still_returns_the_image_it_stored` | ignore the token; check after the failure is consumed; failure persists; failure hits any tag; cancel drops the image | `Err(BuildFailed { stage: Construct, .. })` for the `Cancelled` assertion; `assertion failed: matches!(engine.find(&hit).await, Ok(None))`; `assertion failed: matches!(engine.find(&other).await, Ok(None))`; `Error: BuilderUnreadable { .. }`; `left: None`, `right: Some(ImageId(..))` |
| P44-F2-11 | none (compile probe) | `engine.at(EngineMoment::AfterFind { tag, retag: BuildEffect::Cancel })` | `error[E0308]: mismatched types ... expected Retag, found BuildEffect`; at `3f17ce17` the old form `at(AfterFind(tag), Cancel)` compiles |
| P44-F2-12 | `gateway/terminal.rs` `ctrl_p_ctrl_q_detaches_in_one_chunk_or_two_and_the_process_survives`, `a_lone_ctrl_p_is_held_and_released_in_front_of_the_next_byte_and_a_lone_ctrl_q_is_forwarded` | no detach; forward Ctrl-P at once; never release the held byte; detach on Ctrl-Q alone | `timed out waiting for one chunk; saw []`; `assertion failed: handle.main_input().is_empty()` and `saw [Output([16, 17])]`; `timed out waiting for the released bytes to be [16, 120]; saw [120]`; `timed out waiting for a lone Ctrl-Q to be [17]; saw []` |
| P44-F1-16a | `gateway/contract.rs` `core_group_passes_on_the_fake_with_every_published_check_ok`; `crates/jackin-openshell/tests/gateway/contract.rs` `a_server_that_refuses_stop_on_a_stopped_sandbox_fails_only_that_check` | fake: Stop on `Stopped` refused; check without the second Stop | `left: [("stop_on_stopped_is_ok", "failed precondition: x")]`; the red-variant test got `Ok(())` |
| P44-F1-16b | `gateway/contract.rs` `core_group_passes_on_the_fake_with_every_published_check_ok`; openshell `a_server_that_deletes_an_attached_provider_fails_only_that_check_and_leaves_nothing`, `core_group_passes_against_grpc_gateway_on_the_in_process_server` | fake: unconditional delete; server without the attached check; check without the assertion | `delete_provider while a sandbox lists it is not FailedPrecondition: Ok(())`; `Err("the provider is absent after a refused delete")` |
| P44-F1-16c | `gateway/contract.rs` `terminal_group_passes_on_the_fake_including_the_lease_checks`; openshell `an_ended_main_process_attach_that_is_refused_passes_the_check_and_one_that_grants_the_lease_fails_it`, `a_main_attach_on_an_ended_sandbox_is_refused_and_on_a_stopped_one_not_ready`, `terminal_group_reports_exactly_the_published_checks_and_a_refused_attach_fails_the_attach_checks` | fake: main through `exec_cell`; server lookup Ready only; check accepts any events | `("main_ended_is_refused", "the sandbox is not ready: Error { exit_code: Some(7) }")`; `called Result::unwrap() on an Err value: NotReady { phase: Error { exit_code: Some(7) } }`; the red-variant test got `Ok(())` |
| P44-F1-16d | `gateway/contract.rs` `transfer_group_passes_on_the_fake`; openshell `an_upload_to_an_absent_sandbox_passes_the_check_when_the_cli_fails_and_fails_it_when_the_cli_succeeds` | fake: upload through `exec_cell`; check accepts any error | `("upload_absent_sandbox_is_a_transfer_error", "an upload to an absent sandbox is not a Transfer error: Err(NotFound { what: Sandbox(..) })")`; the red-variant test got `Ok(())` |
| P44-F2-13 | `gateway/contract.rs` `a_server_that_deletes_a_provider_while_refusing_the_delete_fails_only_the_presence_assertion` | check assertion `provider_state_is(.., Present).await? \|\| true`; server variant `Broken::DeleteAfterRefusal` (removes the provider, still returns `failed_precondition`) | `Ok(())` where `Err("the provider is absent after a refused delete")` was expected; restored: pass |

## Not verified

- No live gateway run. The four new checks have not run against a real gateway. The first live run must confirm: Stop on `Stopped` (e.5 is a Ready-to-Stopped capture followed by a second Stop), the contract profile with an attached provider (b.10 used the Codex profile), the ended-main refusal text, and the upload error class.
- `Completed` main attach (source-derived), attach to a sandbox that is not Ready for upload (source-derived), `Error{None}` main attach (stays `NotReady`), a read-only attachment and Ctrl-P Ctrl-Q.

### U10b (P5.1b) and U10 (P5.2), from rewrite/reconciliation/U10.md

This file records where task P5.1b differs from `rewrite/API.md` and `rewrite/DESIGN.md` D08, the preconditions B1 to B14 of the approved test plan (`P5.1b-test-plan.md`, with the orchestrator decisions O1 to O8), and the red/green proof of each new row. The orchestrator folds it into API.md at merge. Each precondition is written before the commit that carries the test which uses it.

## Preconditions

- B1. Supersession. The D08 text (74676a92), the decisions row of 2026-11-04T01:30, PLAN P5.1b, U9-F1 F1-7-P1 to P5 with its G8/T5/T6 red rows, U9-F2 F2-P2 upload rows, and the API.md lines for `Gateway::upload_project`, `ProjectUpload`, `TransferOp::UploadGit`, the fake `upload_project` description and the `Transfer` contract group are replaced by this task. API.md is not edited here. `upload_project`, `ProjectUpload`, `sanitized_git_config`, `GatewayError::UnsupportedGitDirectory`, `TransferOp::UploadGit`, `TransferOp::GitConfig` and `GatewayOp::Upload` are deleted in the contract-and-deletion commit.
- B2. Gateway surface. `Gateway::upload_tree(&self, sandbox: &SandboxName, source: &ProjectRoot, dest: &SandboxPath, cancel: &CancellationToken) -> Result<(), GatewayError>` runs `openshell sandbox upload <sandbox> . <dest>` with the project root as `cwd`. `Gateway::upload_file(&self, sandbox: &SandboxName, source: &HostPath, dest: &SandboxPath, cancel: &CancellationToken) -> Result<(), GatewayError>` runs `openshell sandbox upload <sandbox> <file> <dest> --no-git-ignore` with no `cwd`, then compares the host `sha256` of the file with `sha256sum <dest>` in the sandbox. Both calls remove `OPENSHELL_` and `GIT_` from the environment, take a null stdin and the transfer timeout. `TransferOp` is `UploadFiltered` (tree call), `UploadFile` (file call), `Download`, `Digest`, `Size`. A mismatch is `Transfer { op: Digest, status: Exited(0), tail }`, and the tail holds both digests. `GatewayOp::UploadTree` and `GatewayOp::UploadFile` replace `GatewayOp::Upload`, so a `Trigger::Op` tells the two calls apart. `ProjectUpload` is deleted; the two calls take their arguments directly.
- B3. Fake fidelity. `FakeGateway::upload_tree` replicates the CLI source at `ac7f615` (`run.rs:5235`, `ssh.rs:1083-1120`) until L2, L7 and L8 are captured: it copies the set that `git ls-files -z -co --exclude-standard` lists, archives a listed directory in full (a nested repository keeps its `.git`), skips a listed path that is missing, never deletes, and does not list the `.git` entry of the project. It keeps a file symlink, a directory symlink and a dangling symlink as symlinks (the CLI does too: live L11 shows `link.txt` arriving as a symlink, and the tree gate passed); the fake used to follow them and so raised a `ProjectMismatch` that the live run never does. Only the file symlink is checked live (the `upload_tree` contract check); the directory and dangling cases rest on the fake alone and are an item for the next live run. It runs `ls-files` with `HOME` set to the shared temp home of the `World` (`FakeGateway::use_host_home`), so the user git configuration is the one the project module sees (`P51b-GATE-GLOBAL`). `upload_file` copies the file to the exact destination (parents created, an existing file replaced), then compares the SHA-256 of the source with that of the landed file. New faults: `Fault::CorruptUpload` flips one byte of the next landed file, so the digest check fails with `Transfer { op: Digest }`; `Fault::EditHostAfter { upload: UploadKind, edit: HostEdit }` edits the host after the next tree or file upload (`HostEdit::Write` or `HostEdit::Chmod`). The fake sandbox runs `/usr/bin/find`, `/usr/bin/mkdir` and `/usr/bin/rm` as the host `find`, `mkdir` and `rm`, and expands `/sandbox` inside environment values as it already does inside arguments (the gate sets `GIT_INDEX_FILE` and `GIT_OBJECT_DIRECTORY` under `/sandbox/.jackin`). `SandboxHandle::root()` returns the host directory that stands for `/sandbox`, so a test can run real git in the sandbox repository.
- B4. Host environment. The real runner applies `ProcessSpec.env` after `env_remove_prefixes` (`tests/process_runner/env.rs`). Every host git call of the project module sets `env_remove_prefixes` to `GIT_` and then sets `GIT_OPTIONAL_LOCKS=0`; the gate sets `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY` and `GIT_ALTERNATE_OBJECT_DIRECTORIES` explicitly; the bundle call sets `GIT_NO_LAZY_FETCH=1`. `TestRunner` gains a temp `HOME` (`TestRunner::with_home`, owned by `World::home()`), sets `GIT_CONFIG_NOSYSTEM=1` for every host git it starts, applies a list of ambient variables (`TestRunner::set_ambient`) before the `env_remove_prefixes` of the spec, so a test can put `GIT_DIR` into the runner as a process would, and runs a scripted reply for `git` when an expectation was added with `ScriptedProcessRunner::expect_git` (`git --version` for `P51b-GIT-VERSION`); every other git call runs the real `/usr/bin/git`. Tests never call `set_var`. `/etc/gitconfig` and the Apple system configuration are machine state; the runner and the fake set `GIT_CONFIG_NOSYSTEM=1`, and no assertion depends on them.

- B5. Launch errors and where the checks run. `LaunchError` gains `Process(ProcessError)`, `Path(PathError)`, `HostGit(HostGitError)` (`TooOld { found, required }` and `Unusable { detail }`; the host floor is the per-line table `GIT_FLOORS` (2.45.4, 2.46.4, 2.47.3, 2.48.2, 2.49.1, 2.50.1, any 2.51 or later)), `Unsupported(UnsupportedProject)` (`SparseCheckout`, `PopulatedSubmodules(Vec<RepoPath>)`, `NestedRepositories(Vec<RepoPath>)`, `PartialClone { output: TerminalText }`), `ProjectMismatch(Box<TreeMismatch>)` and the two step errors `ProjectFailed { step: ProjectStep, exit_code, output }` and `ProjectOutput { step, message }`. `TreeMismatch` carries `host` and `sandbox` as `TreeId` (40 or 64 lowercase hex), `host_only`, `sandbox_only` and `differing` path lists (computed from `git ls-tree -r -z` of both trees, only for the error), and `autocrlf: Option<String>` (O8; the message names it and suggests `.gitattributes` `text eol=...`). The checks and refusals run in `Run::execute`, before the first `find_sandboxes` call and so before the image, the provider and the create (O5). A failed or refused preflight returns `LaunchFailure { step: LaunchStep::Upload, sandbox: None }` and emits no step event; `LaunchStep` is unchanged, so the nine-step event contract of P5.1 holds. A cancel before or during the preflight ends as `Cancelled`. `check_git(runner, cancel) -> Result<GitVersion, HostGitError>` is public for `doctor()` (P5.2); the launch runs it first.
- B6. Fixtures. `RepoFixture` gains `new_sha256`, `git(&[..])` (real git in the root, global and system configuration disabled), `read`, `remove`, `chmod`, `linked_worktree`, `clone_of` (a `file://` clone with options: `--depth`, `--filter`), `superproject` (an outer repository holding the source as a submodule) and `submodule_checkout` (the populated submodule as the root). Submodule fixtures add with `-c protocol.file.allow=always`. The fixture keeps its own `GIT_CONFIG_GLOBAL=/dev/null` and `GIT_CONFIG_NOSYSTEM=1`; the identity tests set `user.name` with `git config`, which the hard-coded `-c user.name` of the fixture does not affect. A new test target `crates/jackin-session/tests/project` holds the rows; its `Rig` builds a role without hooks and a config whose workspace is the fixture root.
- B7. Platform. The fake runs the macOS `/usr/bin/git` as the sandbox git. No assertion reads `core.ignorecase`, `core.precomposeunicode`, `safe.directory` or the owner of the sandbox user; those are live items L3 and L4. The oracle commits in the sandbox with `-c user.name -c user.email` so the probe does not depend on an auto-detected identity, and the identity rows commit with `user.useConfigOnly=true` so that only the repository configuration counts. The fake expands `/sandbox` inside program arguments, so the hostile identity name uses `touch pwned` (a relative path) instead of `touch /sandbox/pwned`.
- B8. Identity is the effective configuration in the project root (`git config -z --get user.name` and `user.email`, global and `includeIf` included), passed to the sandbox as `git config --local -- <key> <value>` arguments of the exec call, never through a shell (O6). A key that is unset is not written. A value that is not UTF-8 is decoded lossily. Git itself trims leading and trailing crud characters (`'`, `"`, `.`, `,`, `;`, `<`, `>`, `\`) from the author name at commit time, so the hostile test name ends in a letter.
- B9. The host staging directory `cache_dir/upload-<launch>-<pid>-<n>` holds the bundle (`upload.bundle`), the temporary index (`gate-index`) and the temporary object directory (`gate-objects`). `prepare` creates it after the refusals and removes it when the bundle step fails; `Run::execute` removes it after the loop on every path (success, failure, cancel). A name that includes the process and a counter keeps two concurrent runs of one launch ID apart (the P5.1 test `two_concurrent_launches_with_one_id_end_with_one_sandbox_and_one_start` runs two in one process). `cache_dir/upload-<launch>.lock` is a file lock that serializes the upload act of one launch ID; it stays on disk like the image locks of F3-P9. The act reads the upload marker after it has the lock, never before: a launch that waited for the lock must see the marker of the peer that held it, and must neither empty the project directory nor upload again while the peer's launch goes on with its hooks and start. `P51b-MARKER-WAIT` pins this order; the two-concurrent-launches test does not (it stays green when the read is removed, because the peers finish before the second one looks). A skip is `SkipReason::AlreadyUploaded`. Without the lock two `git init` runs in one directory fail with `File exists`.
- B10. A host HEAD on `refs/heads/jackin/*` arrives as a detached HEAD at its commit (O1). The bundle command names `HEAD`, so the commit is in the bundle pack; the sandbox fetch does not need `HEAD` as a source, because `git fetch` from a bundle installs the whole pack whatever refs it maps (the fetch source was removed: 32 of 32 project tests stayed green without it, while dropping `HEAD` from the bundle command turns `P51b-JACKIN-HEAD` red with `trying to write ref 'HEAD' with nonexistent object`).
- B11. Host git floor: a patched release per the P5.3 per-line table (2.45.4, 2.46.4, 2.47.3, 2.48.2, 2.49.1, 2.50.1 on their lines, any 2.51 or later) (`--no-lazy-fetch` needs 2.45, the July 2025 security fixes need the patch level). The sandbox never runs `--no-lazy-fetch`; its steps are `init`, `fetch`, `symbolic-ref`, `update-ref`, `reset`, `config`, `add`, `write-tree` and `ls-tree`. L4 reruns when the construct image git changes. The bundle command is `git --no-lazy-fetch bundle create --quiet <file> [HEAD] --exclude=jackin/* --branches --tags --remotes` with `GIT_NO_LAZY_FETCH=1`. Deviation from the D08 text: `--exclude` must come before `--branches` and take the pattern without the `refs/heads/` prefix; checked with git 2.54 on a repository that holds `refs/heads/jackin/old/main` at the same commit as `main`: the D08 order (`--branches --tags --remotes --exclude='refs/heads/jackin/*'`) and the prefixed pattern both list the jackin ref in the bundle, `--exclude=jackin/* --branches` does not. The sandbox fetch still carries `^refs/heads/jackin/*` as defence in depth. Because a bundle fetch installs the whole pack, the two layers are not equal: the bundle `--exclude` keeps the objects of a commit that only a jackin branch reaches out of the sandbox (`P51b-JACKIN-OBJECTS`); the refspec cannot be red on its own, because the bundle lists no jackin ref. The `has_refs` check for an unborn HEAD uses `for-each-ref --exclude=refs/heads/jackin/**`: there `*` does not cross `/` (wildmatch with pathname rules), so `refs/heads/jackin/*` left `jackin/<box>/<branch>` listed and an unborn HEAD with only jackin branches was refused as `PartialClone` (`P51b-JACKIN-UNBORN`). `rev-list`-style `--exclude=jackin/*` in the bundle command is a different matcher and does cross `/`.

## Deleted code and tests

- `crates/jackin-openshell/src/git_config.rs` and its re-export, `sanitized_git_config`, `Gateway::upload_project`, `ProjectUpload`, `GatewayError::UnsupportedGitDirectory`, `TransferOp::UploadGit`, `TransferOp::GitConfig` and `GatewayOp::Upload`; the fake `upload_project`.
- `crates/jackin-openshell/tests/gateway/git_config.rs`; in `tests/gateway/transfer.rs`: `upload_runs_two_openshell_calls_naming_gateway_workspace_and_sandbox` and `upload_failures_name_the_failed_step_and_stop` (replaced by `upload_tree_and_upload_file_run_one_scoped_call_each_and_compare_the_file_digest` and `failed_tree_and_file_uploads_name_their_own_step_and_never_ask_for_a_digest`), `upload_replaces_the_sandbox_git_config_with_a_sanitized_copy_at_the_destination`, `upload_fails_when_the_sanitized_write_fails_and_when_git_is_not_a_directory`, `upload_with_an_unparsable_git_config_fails_before_anything_leaves_the_host`, and the helpers `copy_git_into_sandbox`, `tree`, `ready_sandbox`; in `tests/gateway/fakes.rs`: the `git_calls` record and `run_real_git` (the scripted runner serves only `openshell` now); the `UnsupportedGitDirectory` arms of `status.rs` and `support.rs` (the error count in `every_grpc_code_maps_to_its_gateway_error` is 21).
- `crates/jackin-testkit/tests/gateway/transfer.rs`: the first test (sanitized git directory) is replaced by `an_upload_to_a_stopped_sandbox_is_a_transfer_error_for_both_calls_and_changes_nothing`; its absent-sandbox half is the contract check `upload_absent_sandbox_is_a_transfer_error`. The F2 follow-up test `concurrent_uploads_of_one_project_to_one_sandbox_all_succeed_and_leave_the_project` (U9-F2 row P44-F2-05c) is ported to `upload_tree` and keeps its behaviour under test: the fake copies each file through a temporary name and a rename, so concurrent uploads of one project to one sandbox do not fail on a read-only file that a peer has just landed. The old fixture got its read-only files from the `.git` object store, which P5.1b no longer copies (B3: the fake never lists `.git`). The project now holds 40 untracked files with mode 0444 (`git ls-files -co` lists them), the test runs 8 `upload_tree` calls at once and asserts the last file, `README.md` and no failure. The `.git/HEAD` assertion is dropped because no `.git` entry reaches the sandbox. The `git add` and `git commit` steps are dropped because untracked files are listed anyway. Red row: `P51b-FAKE-RENAME`.
- No `Cargo.toml` dependency was used only by the sanitizer; `cargo shear` passes unchanged.

## Contract

`TRANSFER_CHECKS` is `upload_tree`, `upload_file_with_digest`, `download_with_digest`, `upload_absent_sandbox_is_a_transfer_error`. `upload_tree` builds its own scratch git project (tracked, untracked, ignored files and an untracked symlink `link.txt` to `tracked.txt`; host `git init` and `git add` through `spawn_blocking`) and checks that the tracked and untracked files arrive, the symlink arrives as a symlink with the same target (`test -L` and `readlink`), the ignored file and every `.git` entry do not, a repeat upload gives the same set, and a stale file written into the sandbox survives. `upload_file_with_digest` uploads a small file into an absent parent, a 3 MiB binary into an absent dot-directory, a file into an existing parent, and a replacement over an existing name, and checks the exact file name with `test -f` and the replaced bytes with `read_files`; the digest comparison is inside `upload_file`. The absent-sandbox check covers both calls with their own `TransferOp`. The fixture field `ContractFixture.project` stays and serves the absent-sandbox tree call.

## Remaining preconditions

- B12. The evidence driver is `crates/jackin-session/examples/upload_project.rs`. It prints only synthetic tokens (`jackin-p51b-driver-<round>`). It needs `Session::upload_project(&self, project: &ProjectRoot, sandbox: &SandboxName, launch: &LaunchId, cancel: &CancellationToken) -> Result<(), LaunchError>`, a new public method that runs the preflight, the per-launch lock, the upload steps with the tree gate, and the cleanup, without the marker; the launch keeps its own order (the preflight before any gateway call) and does not call it. A test covers it (`the_public_upload_entry_rebuilds_the_repository_in_an_existing_sandbox`). The driver reaches the contract module, so `jackin-session` has `jackin-openshell` with the `contract` feature as a dev-dependency (the feature of an existing workspace dependency, as in `jackin-testkit`). The driver creates its own sandbox from an image that the gateway already holds and deletes it unless `--keep` is given.
- B13. The tests and code were committed (216ca8a1, 1a4aea54, 53069088) before the red/green proof above.
- B14. P5.2 carries over the export-nesting test (three export rounds give a flat, constant ref set) and `work_state` on a fresh sandbox. The driver shows the nesting check live (`--export --rounds 3`).

## Tests changed in the P5.1 suite

Only the trigger changes. `GatewayOp::Upload` is gone, so `Trigger::Op(GatewayOp::Upload)` in `tests/launch/cancel.rs` (`Point::Upload`), `tests/launch/crash.rs` (the `(Trigger, LaunchStep::Upload)` row) and `tests/launch/events.rs` (the cancel-at-upload row) is `Trigger::Op(GatewayOp::UploadTree)`: the first upload call of the launch, at entry, as before. `crates/jackin-testkit/tests/gateway/triggers.rs` and `power.rs` call `upload_tree` instead of `upload_project`. No assertion of the P5.1 suite changed. The suite passes with the fake's upload step now running real git programs.

## Launch test guard under load (P5.1b-FLAKE)

In one full `xtask ci --fast` run, `cancel::a_cancel_at_each_point_leaves_a_restore_candidate_and_a_retry_completes` and `phases::a_settling_sandbox_is_waited_for_on_the_clock_until_ready_or_timeout` hit the 10 s guard of `tests/launch/support.rs`; four later runs passed. Cause: load, not a hang. The work of each test is bounded, but the guard covered all of it, and P5.1b made every launch run real git.

- Work per test, counted with a temporary counter in `HostProgram::command` and the fixture `git` helper (removed): the cancel test built 8 rigs and ran 16 launches, 330 child processes (216 `git` in the project module and the fake sandbox, 64 fixture `git`, 17 `mkdir`, 24 `rm`, 9 `find`); the settling test built 3 rigs and ran 4 launches, 118 child processes (76 `git`, 24 fixture `git`, 6 `mkdir`, 9 `rm`, 3 `find`). One launch costs about 13 `git` runs. An idle run takes 3.5 s and 2.2 s.
- The settling test also waited 990 fake-clock polls in its patient launch (`ProvisionFor { polls: 1000 }`, ready limit 600 s). Each poll is 64 task yields. That part took 1.2 s of the 2.2 s and is CPU work, not process work.
- Nothing can hang. The tests sleep only on the fake clock, whose `sleep` wakes after 64 yields; the lock poll of `hold` uses the same clock; each child process has a 2 min or 30 min timeout in the project module. With the guard set to 300 s, all 46 launch tests completed in all 6 runs of three concurrent full-workspace `nextest` runs; the slowest were 26.1 s (settling) and 18.2 s (cancel).
- Stress, three concurrent `nextest run --workspace` per round, three rounds, 9 runs, before the change (guard 10 s): the cancel test and the settling test timed out in 9 of 9 runs; `an_ended_suspended_or_deleting_sandbox_stops_the_launch_and_stays` in 9 of 9; the three `crash` tests, `a_silent_or_invalid_supervisor_times_out_before_any_hook_or_start` and one `adr008` test in 1 to 6 of 9.
- Fix, structure: a loop over cases under one guard multiplies the work the guard must cover. The cancel test is now eight tests, one per cancel point (`cancel_at(point)` holds the unchanged body), and the settling test is three tests, one per scenario. No assertion changed. The patient scenario waits `polls: 40` (4 times the 10 polls the 5 s limit spends) instead of 1000; the behaviour under test (a sandbox that stays provisioning past the short limit and is joined by a launch with a long limit) is the same. The guard is 60 s: above the slowest legitimate launch test measured under three concurrent suites (26.1 s before the split, 11.2 s after it for the tests that still run several launches) and below the 180 s at which nextest terminates a test (`slow-timeout` 60 s, `terminate-after` 3), so a real hang still ends as a named failure.
- With the split alone (guard still 10 s), 9 runs: the two tests passed in 9 of 9, but `an_ended_suspended_or_deleting...` (6 of 9), `build_lines_reach_the_feed...` (6 of 9), the `supervisor` and `crash` tests still timed out, which is why the guard changed too.
- Result with the split and the 60 s guard, same stress: 21 runs (9 and 12), no launch test failed. The two split families peak at 4.2 s per test (was 18.2 s and 26.1 s for the whole test).
- Not changed: the other tests of the suite that loop over rigs (`phases::an_ended_suspended_or_deleting_sandbox_stops_the_launch_and_stays`, the three `crash` tests) peak at 9 to 11 s under this stress. They stay within the new guard; splitting them is the same change.
- Out of this change and still red under the same stress, both unrelated to P5.1b: `process_runner::streaming::a_128_mib_line_does_not_grow_process_memory` and `streamed_output_is_lossless_ordered_per_stream_lossy_decoded_and_tailed_to_64_kib` end with `Timeout { program: Git, after: 10s }` (they take 5.8 s to 7.8 s on a quiet machine), and `process_runner::streaming::unstreamed_output_beyond_16_mib_per_stream_fails_with_output_too_large_and_stops_the_child` in a few runs; `plan::accounts::the_plan_selects_the_account_by_the_d26_order_and_names_the_missing_binding` hit its own 10 s guard in 1 of the 9 runs before and in 2 of the 9 runs of the split-only batch, and in none of the 21 later runs. Fixed in the next section.

## Process runner and plan tests under load (P5.1b-LOAD)

Stress: `nextest run --workspace --all-features --locked --no-fail-fast`, three copies at once per round, 18 logical CPUs, debug build. Scripts and logs are in the scratch directory `p51b-load`.

- Before (HEAD 5b881e8c, 18 runs): `process_runner streaming::a_128_mib_line_does_not_grow_process_memory` and `streaming::streamed_output_is_lossless_ordered_per_stream_lossy_decoded_and_tailed_to_64_kib` ended with `Timeout { program: Git, after: 10s }` in 18 of 18 runs. `unstreamed_output_beyond_16_mib_per_stream_...` passed but took 10.5 s on average (max 13.4 s) with only a 10 s limit per step and a 30 s guard. `xtask ci --fast` under two background suites failed twice in two (debugger runs `gs1`, `gs2`, the same two tests).
- Cause: load, not a hang, and not a runner defect. On a quiet machine the lossless test takes 0.9 to 1.2 s with 0.15 s of CPU, the 128 MiB test 3.9 to 4.1 s (4.5 s of CPU in a debug build), the 16 MiB test 1.6 s. With 54 busy-loop processes on the 18 CPUs (the same oversubscription as three suites) the lossless test takes 7.4 to 8.2 s with 0.27 s of CPU, and the 128 MiB test 12.6 s; with 108 loops 16.7 s and 25.0 s; with 216 loops 18.4 s and 51.7 s. A `sample` of the lossless test under load shows 2504 of 2603 samples in `kevent` called from tokio's `park_yield` (inferred: the test sends 30,000 lines through a channel of capacity 1, the task budget forces about one yield per 128 operations, and each yield is a syscall at which the process waits for a CPU slice). The time is scheduler latency times the number of yields, so it grows with load while the work stays small. The 10 s `ProcessSpec.timeout` was a hang bound set from quiet-machine speed.
- Fix 1, bounds sized to the work. `tests/process_runner/support.rs` now has two sizes. `Scratch::new()` and `guarded` keep 10 s and 30 s for the light tests (slowest light test under stress: 7.0 s, `a_timeout_kills_...`, which asserts exact 3 s and 2 s to 6 s values and is unchanged). `Scratch::heavy()` and `guarded_heavy` give the run limit 60 s and the guard 120 s. The three heavy tests use them: the lossless test, the 128 MiB test, and the 16 MiB test (all its runs, including the `yes` run whose explicit 10 s limit was dropped). The 120 s guard stays below the 180 s at which nextest terminates a test (`slow-timeout` 60 s, `terminate-after` 3), so a hang still ends as a named failure. `tests/plan/support.rs`: the guard is 60 s (was 10 s) and its message prints the value; the slowest plan test (`accounts::the_plan_selects_the_account_by_the_d26_order_and_names_the_missing_binding`) took 7.2 s in one batch of 15 runs and 13.6 s in the other (quiet 1.2 s).
- Fix 2, nextest test group `heavy-output` (`max-threads = 1`, `.config/nextest.toml`), assigned by a `[[profile.default.overrides]]` filter to the three heavy tests; the `ci` profile inherits it (checked with `nextest show-config test-groups --profile ci`). Without the group the three tests start together with each other and with the rest of the suite. With it they run one after the other, and the lossless and 16 MiB tests start when the 128 MiB test has finished.
- Not changed: `a_slow_sink_...` (15 s limit; 6.1 to 6.4 s, 3 s stall plus 2 s drain, sleep-bound), the timeout, secret and cancel tests (exact limits, 5.1 s and 6.9 s), and the 10 s `next_line` waits. Every assertion is as before.

Measured under three concurrent suites, 15 runs per column (seconds per test):

| Test | quiet | before (10 s limit) | new bounds, no group | new bounds, with group |
|---|---|---|---|---|
| lossless | 1.2 | timeout in 18 of 18 | 13.8 to 24.2 (avg 17.0) | 1.3 to 1.5 (avg 1.4) |
| 128 MiB | 3.9 | timeout in 18 of 18 | 16.9 to 36.5 (avg 22.1) | 16.1 to 19.5 (avg 18.1) |
| 16 MiB | 1.6 | 10.5 avg, 13.4 max | 12.2 to 17.1 (avg 13.8) | 1.7 to 1.8 (avg 1.8) |

Headroom of the final setting: lossless 60 s / 1.5 s, 16 MiB 60 s / 1.8 s; 128 MiB 60 s / 19.5 s (3.1 times; the run limit is 1.6 times the worst value without the group, which is why the group matters for this test) and 120 s guard (6.2 times). Whole suite wall time, min / median / max of 15 runs: 38.1 / 40.2 / 54.4 s without the group, 37.6 / 42.9 / 45.6 s with it.

Result with the final tree (15 runs, label `group1`): 462 of 462 tests passed in 15 of 15 runs. Without the group (15 runs, `nogroup`): 462 of 462 in 15 of 15. `xtask ci --fast` with one background `nextest` loop, three times: exit 0 three times (10 background suite runs, all 462 passed; the suite inside the gate took 31.1, 32.0 and 27.1 s).

Red proof, mutation applied to `crates/jackin-session/src/host.rs` in the working tree, test run with `nextest run -p jackin-session -E 'test(...)'`, file restored with `git checkout`:

| Test (row) | Mutation | Result |
|---|---|---|
| `a_128_mib_line_does_not_grow_process_memory` (F3-04) | accumulate the whole line and cut at delivery | RED 3 of 3: `peak 141376 KiB`, `141488 KiB`, `141728 KiB exceeds baseline 7632 KiB plus 48 MiB` (4.6 to 4.8 s) |
| `streamed_output_is_lossless_...` | `try_send` and drop a line when the sink is full | RED 2 of 2: `stdout line count` left 2, right 30002 |
| `streamed_output_is_lossless_...` | tail limit 131072 | RED 2 of 2: `stdout tail length 131072` |
| `unstreamed_output_beyond_16_mib_...` (F3-05) | check the 16 MiB bound at EOF only | RED 3 of 3: `expected OutputTooLarge, got Timeout { program: Git, after: 60s }` after 61.3 to 61.5 s (was 10 s; the red run is slower and its `yes` step buffers up to the timeout) |

The plan guard is a hang bound with no mutation row.

## Decisions the brief did not fix

- A failed preflight is `LaunchFailure { step: Upload, sandbox: None }` with no step event (B5).
- The host lock of B9 is required by the P5.1 concurrency test (a per-call lock path turns it red with `ProjectFailed Init ... cannot copy`). The marker read after the lock is required by the launch that waited for a peer (B9), and is pinned by `P51b-MARKER-WAIT`, not by that test.
- The preflight builds the bundle even when the marker exists (a resumed launch). A bundle is the only way to detect a partial clone, as D08 says. The cost is one `bundle create`.
- The bundle flags differ from the D08 text (B11). `HEAD` is a bundle argument for a born HEAD and is not a fetch source (B10).
- The refs oracle skips symbolic remote refs (`refs/remotes/origin/HEAD`): a bundle does not carry them (checked: the sandbox has `refs/remotes/origin/main` and no `origin/HEAD`).
- The gate uses `GIT_ALTERNATE_OBJECT_DIRECTORIES` as D08 and the review say, so `git add -A` freshens (touches the mtime of) loose objects that already exist in the host repository; file bytes and `.git/index` stay untouched, and `P51b-GATE-UNTOUCHED` compares bytes and the mtime of every `index` file.
- `ls-files -v -s -o --exclude-standard -z` and `ls-tree` outputs are bounded by the 16 MiB limit of the runner; a larger project fails with `Process(OutputTooLarge)`. Not verified on the 880 MiB project.

## Live evidence items and the driver

`cargo` runs through `mise exec -- mbx +1.99.1 run -p jackin-session --example upload_project --`. The image must exist in the gateway (`--image`). Flags: `--gateway` (default `jackin-local`), `--workspace` (default `default`), `--project`, `--openshell-home`, `--work-dir`, `--stale`, `--rounds N`, `--export`, `--keep`, `--cleanup-refs`.

| Item | Command |
|---|---|
| L1 | `upload_project contract --image <tag> --project <scratch git repo> --workspace jackin-test` (prints `CHECK <group> <name> Ok` and `GROUP <group> total= failed=`; the Transfer group must read `total=4 failed=0`) |
| L2 | raw CLI: `openshell sandbox upload <sandbox> <file> /sandbox/.jackin/bundle/upload-x.bundle --no-git-ignore`, with and without the parent directory, an overwrite, `sha256sum` in the sandbox against `shasum -a 256` on the host |
| L3 | `upload_project upload --image <tag> --project <repo>`; the section "ownership and modes (L3)" prints `id`, owners and modes, `safe.directory` and `git status` as the exec user |
| L4 | the same run with a sha256 repository and a shallow clone as `--project`; it prints the sandbox `git --version` and runs init, fetch, symbolic-ref or update-ref, reset, status, and the gate's add and write-tree on the construct image; `--export` adds `bundle create` |
| L5 | `upload_project upload --image <tag> --project <worktree root>`, then `<submodule checkout root>`, then `<normal repo>`; it prints host and sandbox HEAD, tree id of the sandbox files, and the modes; `Ok (the tree gate passed)` is the gate on a Linux sandbox |
| L6 | raw CLI upload of a listed untracked nested repository with a synthetic token, then `upload_project upload --project <that repo>`, which prints the refusal before any upload |
| L7 | raw CLI against a global `core.excludesFile`; compare with `git ls-files -co --exclude-standard`, also from a linked worktree |
| L8 | raw CLI second upload (stale file stays); `upload_project upload --stale ...` seeds a nested file, a dotfile and a leftover `.git/HEAD` and prints that none survives |
| L9 | `upload_project upload --export --rounds 3 --cleanup-refs --image <tag> --project <worktree root>`: a fresh sandbox each round, the sandbox refs (none under `refs/heads/jackin/`), a commit, `git bundle create`, `download_file` with digest, a fetch into `refs/heads/jackin/<sandbox>/*`, the host refs after each round (flat), and whether the host worktree and HEAD are unchanged |
| L10 | `upload_project upload --image <tag> --project <jackin repo>`; it prints the upload time and `df` before and after |
| L11 | `upload_project upload ...` on a project with symlinks, NFD names and case-colliding names; it prints the symlinks and the non-ASCII names found in the sandbox |

## Red and green proof

Every mutation was applied to the committed tree (HEAD 138affc4 plus the oracle fix below), the named test was run with `mise exec -- mbx +1.99.1 nextest run`, and the tree was restored. The script is `redgreen.py` in the scratch directory; each red line below is the first assertion or error of the run. The green run of every row is the final gate.

A first pass found that the `refs` oracle in `tests/project/oracle.rs` compared two empty sets (a filter dropped every line). It was fixed (`%(symref)` lines are skipped, others compared as `refname objectid`) and the whole table re-run; the table is the second run.

| Row | Test (file) | Mutation | Red line |
|---|---|---|---|
| P51b-ARRIVE-NORMAL | `a_normal_repository_arrives_with_its_files_refs_and_unstaged_changes` (project/arrive.rs) | drop `git reset -q` | RED: crates/jackin-session/tests/project/oracle.rs:142:5: assertion `left == right` failed: status left: {"?? .gitignore", "?? a.txt", "?? b.txt", "?? run.sh", "?? u.txt", "D  .gitignore", "D  a.txt", "D  b.txt", "D  run.sh"} |
| P51b-ARRIVE-NORMAL | `a_normal_repository_arrives_with_its_files_refs_and_unstaged_changes` (project/arrive.rs) | run `git add -A` instead of `reset` | RED: crates/jackin-session/tests/project/oracle.rs:142:5: assertion `left == right` failed: status left: {"A  u.txt", "M  a.txt"} |
| P51b-ARRIVE-WORKTREE | `a_linked_worktree_root_arrives_as_a_normal_repository` (project/arrive.rs) | refuse a `.git` file | RED: Error: "expected a ready launch, saw Err(LaunchFailure { launch_id: LaunchId(..), step: Upload, sandbox: None, error: Unsupported(NestedRepositories([])) })"  ──────────── |
| P51b-ARRIVE-SUBMODULE-ROOT | `a_submodule_checkout_root_arrives_with_its_detached_head` (project/arrive.rs) | refuse a `.git` file | RED: Error: "expected a ready launch, saw Err(LaunchFailure { launch_id: LaunchId(..), step: Upload, sandbox: None, error: Unsupported(NestedRepositories([])) })"  ──────────── |
| P51b-ARRIVE-SHA256 | `a_sha256_repository_arrives_as_sha256` (project/arrive.rs) | always `init --object-format=sha1` | RED: Error: "expected a ready launch, saw Err(LaunchFailure { launch_id: LaunchId(..), step: Upload, sandbox: Some(SandboxName(\"jackin-platfor-ij4f\")), error: ProjectFailed { step: Fetch, exit_code: 1, output: TerminalText(\"fatal: pack is cor |
| P51b-ARRIVE-SHALLOW | `a_shallow_clone_arrives_with_the_same_shallow_file` (project/arrive.rs) | skip writing `.git/shallow` | RED: Error: "expected a ready launch, saw Err(LaunchFailure { launch_id: LaunchId(..), step: Upload, sandbox: Some(SandboxName(\"jackin-platfor-ij4f\")), error: ProjectFailed { step: Fetch, exit_code: 1, output: TerminalText(\"error: Could not r |
| P51b-ARRIVE-UNBORN | `an_unborn_branch_arrives_unborn_with_every_other_branch` (project/arrive.rs) | pass `HEAD` to `bundle create` when unborn | RED: Error: "expected a ready launch, saw Err(LaunchFailure { launch_id: LaunchId(..), step: Upload, sandbox: None, error: Unsupported(PartialClone { output: TerminalText(\"fatal: ambiguous argument 'HEAD': unknown revision or path not in the wo |
| P51b-ARRIVE-UNBORN | `an_unborn_branch_arrives_unborn_with_every_other_branch` (project/arrive.rs) | skip the bundle when HEAD is unborn although branches exist | RED: crates/jackin-session/tests/project/oracle.rs:134:5: assertion `left == right` failed: refs left: {} |
| P51b-ARRIVE-EMPTY | `a_repository_with_files_and_no_refs_launches_with_an_unborn_head` (project/arrive.rs) | always create a bundle | RED: Error: "expected a ready launch, saw Err(LaunchFailure { launch_id: LaunchId(..), step: Upload, sandbox: None, error: Unsupported(PartialClone { output: TerminalText(\"fatal: Refusing to create empty bundle.\\n\") }) })"  ──────────── |
| P51b-ARRIVE-DETACHED | `a_detached_head_arrives_detached_at_the_same_commit_with_the_branches` (project/arrive.rs) | `update-ref` without `--no-deref` | RED: crates/jackin-session/tests/project/oracle.rs:128:5: assertion `left == right` failed: HEAD left: (Some("refs/heads/master"), Some("2fff62ed3651c1dbc5e3ff23e8ebeb40f8544253")) |
| P51b-REFUSE-SPARSE | `a_sparse_checkout_or_any_skip_worktree_entry_is_refused_and_assume_unchanged_is_not` (project/refuse.rs) | match only the tag `S` | RED: Error: "expected a failed launch, saw Ok(Ready(AttachTarget { sandbox: SandboxName(\"jackin-platfor-ij4f\"), state: Running { attempt: AttemptId(1) } }))"  ──────────── |
| P51b-REFUSE-SUBMODULE | `a_populated_submodule_is_refused_by_name_and_an_empty_one_arrives` (project/refuse.rs) | refuse any gitlink | RED: Error: "expected a ready launch, saw Err(LaunchFailure { launch_id: LaunchId(..), step: Upload, sandbox: None, error: Unsupported(PopulatedSubmodules([RepoPath(\"lib\")])) })"  ──────────── |
| P51b-REFUSE-NESTED | `an_untracked_nested_repository_at_any_depth_is_refused_unless_it_is_ignored` (project/refuse.rs) | skip the nested repository check | RED: Error: "expected a failed launch, saw Ok(Ready(AttachTarget { sandbox: SandboxName(\"jackin-platfor-ij4f\"), state: Running { attempt: AttemptId(1) } }))"  ──────────── |
| P51b-REFUSE-PARTIAL | `a_partial_clone_is_refused_with_the_git_output_and_a_promisor_config_alone_is_not` (project/refuse.rs) | drop `--no-lazy-fetch` and `GIT_NO_LAZY_FETCH` | RED: Error: "expected a failed launch, saw Ok(Ready(AttachTarget { sandbox: SandboxName(\"jackin-platfor-ij4f\"), state: Running { attempt: AttemptId(1) } }))"  ──────────── |
| P51b-GATE-RACE | `a_host_edit_after_the_tree_upload_fails_with_a_mismatch_for_content_and_mode` (project/gate.rs) | compare path sets only | RED: Error: "expected a failed launch, saw Ok(Ready(AttachTarget { sandbox: SandboxName(\"jackin-platfor-ij4f\"), state: Running { attempt: AttemptId(1) } }))"  ──────────── |
| P51b-GATE-GLOBAL | `a_filter_in_the_global_settings_is_seen_and_a_global_exclude_is_honoured` (project/gate.rs) | host git with `GIT_CONFIG_GLOBAL=/dev/null` | RED: Error: "expected a failed launch, saw Ok(Ready(AttachTarget { sandbox: SandboxName(\"jackin-platfor-ij4f\"), state: Running { attempt: AttemptId(1) } }))"  ──────────── |
| P51b-GATE-EOL | `global_autocrlf_with_crlf_files_mismatches_and_repo_attributes_travel_with_the_tree` (project/gate.rs) | host git with `GIT_CONFIG_GLOBAL=/dev/null` | RED: Error: "expected a failed launch, saw Ok(Ready(AttachTarget { sandbox: SandboxName(\"jackin-platfor-ij4f\"), state: Running { attempt: AttemptId(1) } }))"  ──────────── |
| P51b-GATE-UNTOUCHED | `neither_a_success_nor_a_mismatch_changes_a_byte_of_the_host_repository` (project/gate.rs) | do not set `GIT_INDEX_FILE` for the host gate | RED: crates/jackin-session/tests/project/gate.rs:206:5: the host changed: ["/private/var/folders/cc/fzt86g2963s5865x4zxczm8h0000gn/T/jackin-project-0vnFWQ/.git/worktrees/tree/index", "mtime /private/var/folders/cc/fzt86g2963s5865x4zxczm8h0000gn/ |
| P51b-RETRY-DELETE | `a_retry_after_a_host_deletion_ends_ready_without_the_deleted_file` (project/retry.rs) | remove the empty step | RED: Error: "expected a ready launch, saw Err(LaunchFailure { launch_id: LaunchId(..), step: Upload, sandbox: Some(SandboxName(\"jackin-platfor-ij4f\")), error: ProjectMismatch(TreeMismatch { host: TreeId(\"4559c21880f99928a519db40deacef0cc4da34 |
| P51b-EMPTY-DIR | `the_project_directory_is_emptied_but_nothing_outside_it` (project/retry.rs) | empty `/sandbox` instead of `/sandbox/project` | RED: crates/jackin-session/tests/project/retry.rs:107:9: [SandboxPath("/sandbox/.codex/note"), SandboxPath("/sandbox/.jackin/extra"), SandboxPath("/sandbox/outside.txt")] note: run with `RUST_BACKTRACE=1` environment variable to display a backtr |
| P51b-EMPTY-DIR | `the_project_directory_is_emptied_but_nothing_outside_it` (project/retry.rs) | remove the empty step | RED: Error: "expected a ready launch, saw Err(LaunchFailure { launch_id: LaunchId(..), step: Upload, sandbox: Some(SandboxName(\"jackin-platfor-ij4f\")), error: ProjectMismatch(TreeMismatch { host: TreeId(\"f72d363127e2e72c24d7b1a0568fccc7b7bc0c |
| P51b-RETRY-CUTS | `a_failure_at_every_sandbox_program_run_retries_to_the_state_of_a_clean_run` (project/retry.rs) | drop `--update-head-ok` | RED: Error: "expected a ready launch, saw Err(LaunchFailure { launch_id: LaunchId(..), step: Upload, sandbox: Some(SandboxName(\"jackin-platfor-ij4f\")), error: ProjectFailed { step: Fetch, exit_code: 128, output: TerminalText(\"fatal: refusing  |
| P51b-MARKER | `no_marker_is_written_at_a_corrupt_upload_a_failing_step_or_a_mismatch` (project/retry.rs) | write the marker before the upload and gate | RED: crates/jackin-session/tests/project/retry.rs:174:9: assertion failed: handle.file(&marker()).is_none() note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace |
| P51b-JACKIN-REFS | `no_jackin_ref_reaches_the_sandbox_even_after_an_export_between_two_launches` (project/secrets.rs) | drop `--exclude=jackin/*` and the `^refs/heads/jackin/*` refspec | RED: crates/jackin-session/tests/project/secrets.rs:45:9: assertion `left == right` failed left: {"refs/heads/feat", "refs/heads/jackin/old/feat", "refs/heads/jackin/old/main", "refs/heads/main"} |
| P51b-JACKIN-OBJECTS | `no_jackin_ref_reaches_the_sandbox_even_after_an_export_between_two_launches` (project/secrets.rs) | drop `--exclude=jackin/*` from the bundle command only | RED: crates/jackin-session/tests/project/secrets.rs:47:9: the commit of a jackin branch is in the sandbox object store |
| P51b-JACKIN-REFS | `no_jackin_ref_reaches_the_sandbox_even_after_an_export_between_two_launches` (project/secrets.rs) | drop the refspec only (stays green) | GREEN:  |
| P51b-JACKIN-HEAD | `a_host_head_on_a_jackin_branch_arrives_detached_at_its_commit` (project/secrets.rs) | treat a jackin/ HEAD as a branch | RED: crates/jackin-session/tests/project/secrets.rs:79:9: assertion failed: !run(&project, &["symbolic-ref", "-q", "HEAD"])?.status.success() note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace |
| P51b-JACKIN-HEAD | `a_host_head_on_a_jackin_branch_arrives_detached_at_its_commit` (project/secrets.rs) | drop the `HEAD` argument of the bundle command | RED: `ProjectFailed { step: SetHead, exit_code: 128, ... trying to write ref 'HEAD' with nonexistent object b21a890d... }` |
| P51b-JACKIN-UNBORN | `an_unborn_head_with_only_jackin_branches_launches_unborn_without_a_bundle` (project/secrets.rs) | `has_refs` with `--exclude=refs/heads/jackin/*` (the first version) and with no exclude | RED both: `Unsupported(PartialClone { output: "fatal: Refusing to create empty bundle." })`; GREEN with `refs/heads/jackin/**` |
| P51b-MARKER-WAIT | `a_launch_that_waits_for_the_upload_lock_skips_the_upload_when_a_peer_marked_it_meanwhile` (project/retry.rs) | read the marker before taking the lock | RED: retry.rs:268:9 `left: Some(Done)`, `right: Some(Skipped(AlreadyUploaded))` |
| P51b-MARKER-WAIT | same | never skip on the marker | RED: same line, same `left`/`right` |
| P51b-FAKE-SYMLINK | `symlinks_arrive_as_symlinks_for_a_file_a_directory_and_a_dangling_target` (project/arrive.rs) | the fake `upload_tree` follows symlinks (the code before this change) | RED: `ProjectMismatch(TreeMismatch { host_only: ["dangling", "dirlink"], sandbox_only: ["dirlink/inner.txt"], differing: ["link.txt"] })` |
| P51b-FAKE-SYMLINK | `transfer_group_passes_on_the_fake`, `a_faulted_fake_fails_exactly_the_matching_check_and_still_runs_the_rest` (testkit tests/gateway/contract.rs; the check also runs live) | same | RED: contract.rs:58:9 `upload_tree`: `the uploaded files are ["./.gitignore", "./link.txt", "./sub/untracked.txt", "./tracked.txt"], not [...]` (the follow copy lands `link.txt` as a regular file) |
| P51b-NO-CREDENTIAL | `no_host_credential_reaches_any_sandbox_file_or_the_sandbox_repository_settings` (project/secrets.rs) | copy the host `.git/config` | RED: crates/jackin-session/tests/project/secrets.rs:112:17: SECRETTOKEN-remote in /var/folders/cc/fzt86g2963s5865x4zxczm8h0000gn/T/jackin-fake-sandbox-vLR5mn/sandbox/project/.git/config note: run with `RUST_BACKTRACE=1` environment variable to d |
| P51b-IDENTITY | `the_effective_host_identity_reaches_the_sandbox_repository and without_an_identity_no_user_key_is_set_and_a_hostile_name_is_never_interpreted` (project/identity.rs) | read `--local` only | RED: Error: "git [\"-c\", \"user.useConfigOnly=true\", \"commit\", \"-q\", \"--allow-empty\", \"-m\", \"probe\"] in /var/folders/cc/fzt86g2963s5865x4zxczm8h0000gn/T/jackin-fake-sandbox-coMmkk/sandbox/project failed: Author identity unknown\n\n** |
| P51b-EXCLUDE | `the_host_exclude_file_travels_into_the_sandbox_repository_byte_for_byte` (project/secrets.rs) | skip the carry | RED: crates/jackin-session/tests/project/secrets.rs:193:17: assertion `left == right` failed left: "?? local-only.log\n" |
| P51b-EXCLUDE | `the_host_exclude_file_travels_into_the_sandbox_repository_byte_for_byte` (project/secrets.rs) | read `<root>/.git/info/exclude` | RED: Error: "expected a ready launch, saw Err(LaunchFailure { launch_id: LaunchId(..), step: Upload, sandbox: None, error: Fs(Io { path: HostPath(\"/private/var/folders/cc/fzt86g2963s5865x4zxczm8h0000gn/T/jackin-worktree-haEqcq/tree/.git/info/ex |
| P51b-DIGEST-LAUNCH | `a_bundle_that_lands_corrupt_fails_the_launch_before_any_repository_exists` (project/digest.rs) | ignore the `upload_file` error | RED: crates/jackin-session/tests/project/digest.rs:21:9: assertion failed: matches!(*failed.error, LaunchError::Gateway(GatewayError::Transfer |
| P51b-HOST-ENV | `ambient_git_variables_of_the_host_process_never_reach_the_project_git_calls` (project/host_env.rs) | stop stripping `GIT_` | RED: Error: "expected a ready launch, saw Err(LaunchFailure { launch_id: LaunchId(..), step: Upload, sandbox: Some(SandboxName(\"jackin-platfor-ij4f\")), error: ProjectMismatch(TreeMismatch { host: TreeId(\"30f89f972dfe0776a2778026c7035b1a764f7c |
| P51b-GIT-VERSION | `the_version_floor_is_numeric_and_a_bad_git_fails_typed_before_any_gateway_state` (project/git_version.rs) | compare versions as strings | RED: crates/jackin-session/tests/project/git_version.rs:43:18: called `Result::unwrap_err()` on an `Ok` value: GitVersion { major: 2, minor: 9, patch: 0 } note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace |
| P51b-CACHE-CLEAN | `the_host_bundle_and_temporary_git_state_are_deleted_on_every_path` (project/cache.rs) | never release the staging directory | RED: crates/jackin-session/tests/project/cache.rs:32:9: success: upload-bbbbbbbbbbbbbbbbbbbbbbbbbb-59605-0 is left in ["upload-bbbbbbbbbbbbbbbbbbbbbbbbbb-59605-0", "upload-bbbbbbbbbbbbbbbbbbbbbbbbbb-59605-0/gate-index", "upload-bbbbbbbbbbbbbbbbb |
| P51b-CONTRACT | `transfer_group_passes_on_the_fake and a_faulted_fake_fails_exactly_the_matching_check_and_still_runs_the_rest` (testkit tests/gateway/contract.rs) | the fake deletes on upload | RED: crates/jackin-testkit/tests/gateway/contract.rs:58:9: assertion `left == right` failed left: [("upload_tree", "a repeated upload changed the stale file set: [\"./.gitignore\", \"./sub/untracked.txt\", \"./tracked.txt\"]")] |
| P51b-CONTRACT | `transfer_group_passes_on_the_fake and a_faulted_fake_fails_exactly_the_matching_check_and_still_runs_the_rest` (testkit tests/gateway/contract.rs) | the fake names the file as a directory | RED: crates/jackin-testkit/tests/gateway/contract.rs:58:9: assertion `left == right` failed left: [("upload_file_with_digest", "protocol error: Is a directory (os error 21)")] |
| P51b-CONTRACT | `transfer_group_passes_on_the_fake and a_faulted_fake_fails_exactly_the_matching_check_and_still_runs_the_rest` (testkit tests/gateway/contract.rs) | the fake skips the digest | RED: crates/jackin-testkit/tests/gateway/contract.rs:106:9: assertion `left == right` failed: [] left: 0 |
| P51b-DIGEST-ADAPTER | `upload_tree_and_upload_file_run_one_scoped_call_each_and_compare_the_file_digest`, `failed_tree_and_file_uploads_name_their_own_step_and_never_ask_for_a_digest` (jackin-openshell tests/gateway/transfer.rs) | skip the digest compare | RED: transfer.rs:200 `called Result::unwrap_err() on an Ok value: ()` |
| P51b-DIGEST-ADAPTER | same | compare against `/sandbox/project` instead of the destination | RED: `left: [["/usr/bin/sha256sum", "/sandbox/project"]]`, `right: [["/usr/bin/sha256sum", "/sandbox/.jackin/bundle/upload-ab12.bundle"]]` |
| P51b-NO-DEAD | (reviewer command) | add `pub(crate) fn unused_probe() {}` to `crates/jackin-openshell/src/transfer.rs` | RED: clippy `error: function unused_probe is never used` |
| P51b-PUBLIC-UPLOAD | `the_public_upload_entry_rebuilds_the_repository_in_an_existing_sandbox` (project/arrive.rs) | `Session::upload_project` skips the upload | RED: `a.txt: content` left `alpha\n` bytes, right the edited bytes |
| P51b-FAKE-RENAME | `concurrent_uploads_of_one_project_to_one_sandbox_all_succeed_and_leave_the_project` (testkit tests/gateway/transfer.rs) | `replace_file` in `crates/jackin-testkit/src/gateway/files.rs` as remove then copy (no temporary name, no rename) | RED in 10 of 10 runs: `Error: Protocol { message: "Permission denied (os error 13)" }`; green in 20 of 20 runs with the committed code |

Notes on the table. `P51b-JACKIN-REFS`: with the bundle flag in the working order (`--exclude=jackin/*` before `--branches`), dropping only the fetch refspec stays green, because the bundle lists no jackin ref; the refspec is kept as defence in depth. Dropping only the bundle `--exclude` leaves the refs out but brings in the objects of a jackin-only commit, which is `P51b-JACKIN-OBJECTS`. The rows added by the fix round (`P51b-FAKE-SYMLINK`, `P51b-JACKIN-OBJECTS`, `P51b-JACKIN-HEAD` second row, `P51b-JACKIN-UNBORN`, `P51b-MARKER-WAIT`) were each run red on the tree of the fix round and green with the final code. `P51b-GATE-RACE`: the path-set comparison is the plan's "status paths" mutation and passes all three edits wrongly. `P51b-REFUSE-PARTIAL`: the `--no-lazy-fetch` flag and `GIT_NO_LAZY_FETCH` are dropped together, because the variable alone has the same effect. `P51b-ARRIVE-WORKTREE`: the plan's second mutation (upload the `.git` file) is a CLI behaviour; the fake never lists `.git`, so only the refusal mutation was run. `P51b-RETRY-CUTS`: with the empty step every retry starts in an empty directory, so `+` forcing and an idempotent `git init` are not reachable; the mutation that proves the sweep is `--update-head-ok`, which fails on the first fetch because the host repository has a `master` branch. `P51b-GIT-VERSION`: the exit-129 mutation needs a host git older than 2.45, which the machine does not have; the string comparison turns the `2.9.0` case red.

## P5.2 (attach, shell, restart, list, inspect, stop, start, export, work state, purge, doctor)

This section records where task P5.2 differs from `rewrite/API.md` section 4 and DESIGN.md D07 to D09 and ADR-008 and ADR-010, the preconditions U11-1 to U11-18 of the approved test plan (`P5.2-test-plan.md`, with the orchestrator decisions O-A to O-M, which override the plan where they differ), and the red/green proof of each new row. The orchestrator folds it into API.md at merge; the plan calls the numbers U11 and they are kept here.

### Preconditions U11-1 to U11-18

- U11-1. Supersession. The plan, B14 of P5.1b and the P4.2 schedule of D1 (download cancel) and D2 (missing-file class) are replaced by this task. Both are closed by the two contract checks and the fake change of U11-14 and U11-16.
- U11-2. Records go in this file, not in API.md (the orchestrator appends them at merge).
- U11-3. This task is U10 (P5.2) in the sequencing table; P5.1b is U10b.
- U11-4. PLAN P5.2 scope now includes doctor, restart and inspect, and no host runner. Not edited here.
- U11-5. Every host git call of export and `work_state` uses `project::host_git(deps, root, cancel)`, the one construction site of `HostGit` (`project.rs`, found with `grep -rn "HostGit {"`). `HostGit::capture` now returns `ProcessError`; `require` and the launch callers are unchanged. `doctor` calls `check_git` of P5.1b for the git check. Removal of `GIT_*` is proven by the runner rows of P5.1b only.
- U11-6. Export writes refs only under `refs/heads/jackin/<sandbox>/`, in one `git fetch --atomic --no-tags --no-prune --no-write-fetch-head --no-recurse-submodules <bundle> +<src>:<dst>...` with `gc.auto=0` and `maintenance.auto=false`. There is no `--update-head-ok`, no host `status` that writes, no `--refresh`, no `merge`, `checkout` or `reset`. `FETCH_HEAD` is not written. Objects and reflogs may change.
- U11-7. Each export draws one unique token (`LaunchId::generate()`, 26 characters from the OS entropy source; `ExportError::Entropy` if it is unavailable). The host bundle is `cache_dir/export-<sandbox>-<token>/export.bundle` and the directory is removed on success, error and cancel. The sandbox bundle is `/sandbox/.jackin/bundle/export-<token>.bundle`, created after `mkdir -p /sandbox/.jackin/bundle`, and removed with `rm -f` after the download on success, error and cancel (the removal runs on a fresh cancellation token and a failed removal is ignored; the path is unique, so a leftover is only disk). Concurrent exports share nothing in the sandbox; there is no export lock and no lock file (the first version serialized them with `cache_dir/export-<sandbox>.lock`; the red/green review replaced that, see P52-EXP-CONCURRENT). `sandbox_layout::bundle_path` is deleted.
- U11-8. A sandbox branch that `BranchName` rejects fails the export with `ExportError::UnusableBranch { branch: String }` before any sandbox or host write, and `work_state` lists it in `Unexported.unusable`, so it is never `Exported`. Branch lists are sorted ascending (`BTreeMap`).
- U11-9. `GatewayError::Cancelled` and `ProcessError::Cancelled` map to `ExportError::Cancelled` through the two `From` impls (the `#[from]` on `Gateway` is gone). A token that is already cancelled gives `Cancelled` before any call.
- U11-10. `work_state` classification: not `Ready` gives `NotRunning(phase)`; a labels error, an absent or unmanaged sandbox, a host project that cannot be opened, a host git failure and every other sandbox git or gateway failure give `InspectionFailed`; a sandbox with no repository (`git rev-parse --git-dir` exits 128 with "not a git repository", or prints a path other than `.git`) gives `NoRepository`; a workspace missing from the config gives `WorkspaceUnknown`. It never reads `RUN_STATE`.
- U11-11. `purge` finds the sandbox once, computes the state from that view immediately before the delete, and takes `AlreadyGone` from the find (absent, unmanaged or `Deleting`). It never touches providers.
- U11-12. `power` refuses `Completed` and every `Error` (including `None`) with `MainProcessEnded` after the find and before `set_power`. The wait polls every 500 ms (`service::POLL`, now shared with the launch) until `limits.ready` and reports `PowerTimeout { after }` where `after` is the time waited. A phase that ends while waiting is `MainProcessEnded`.
- U11-13. `list` uses `find_sandboxes(Labels(managed()))` and nothing else (O-F: `SandboxListing { sandboxes, unreadable }`).
- U11-14. Fake `download_file`: the fault strike comes first, then the cancel check (`GatewayError::Cancelled { op: Download }`), then the sandbox lookup and phase check, then the file. A missing file under `/sandbox` gives `Transfer { op: Size, status: Exited(1) }`; a path outside `/sandbox` stays `Transfer { op: Download }`. The local file is removed on any error.
- U11-15. Testkit additions: `FakeGateway::insert_unmanaged(name, Labels, Phase)` and `FakeGateway::add_live_rule` (a rule in the live policy that `sandbox_detail` reports); `SandboxHandle::git` and `commit_in_project` (with `-c user.name`, `-c user.email`); `HostSnapshot::take` (index bytes first, then refs split into `exported` and `rest`, HEAD, `--no-optional-locks status --porcelain=v2`, the git config bytes, working-tree bytes and modes); the shared `Rig` (`jackin_testkit::Rig`, a testkit module instead of a copy of `tests/launch/support.rs`), `TestResult` and `guard`. The fix round moved the launch tests onto the same Rig and deleted the copy (see P52-FIX-LAUNCH-RIG): the builder gained `role` and `login`, the Rig gained `gateway`, `run`, `run_on`, `launch_observed`, `session_with`, `session_over`, `sandbox`, `stages` and `final_image`, `loaded` and `home` are public, `Outcome { result, events }` and `guard_within(limit, work)` are public, and `ACCESS`, `REFRESH`, `ACCOUNT_ID` and `PLACEHOLDER` are public constants. The launch tests keep their 60 s guard through `guard_within`. `FakeGateway::set_serving(bool)` makes `health()` report `serving: false`.
- U11-16. `TRANSFER_CHECKS` is `upload_tree`, `upload_file_with_digest`, `download_with_digest`, `download_missing_remote_is_a_size_error`, `download_pre_cancelled_is_cancelled`, `upload_absent_sandbox_is_a_transfer_error` (6). The new checks come after `download_with_digest`, so the one-shot `CorruptDownload` test still fails exactly one check. Each check has a positive control. The in-process server of `jackin-openshell` gained `host_tools` (answers `/usr/bin/stat -c %s -- <path>` and `/usr/bin/sha256sum <path>` against the host root), so the adapter's `stat` precheck and digest run there. The live output goes into `evidence/P5.2-export-live.md` (orchestrator); if live differs, the check and the fake follow live.
- U11-17. Doctor status matrix: `OpenshellBinary`, `SshBinary` and `GitBinary` are `Fail` when the call fails or (git) the version is not a patched release per the P5.3 per-line table (2.45.4, 2.46.4, 2.47.3, 2.48.2, 2.49.1, 2.50.1 on their lines, any 2.51 or later); `GatewayRegistered` is `Fail` when the gateway is not in `openshell gateway list` and `Skip` for `NotChosen`; `GatewayHealth` is `Fail` for a `Failed` access, a health error or `serving: false`, and `Skip` for `NotChosen`; `PinnedVersion` is `Fail` on any difference from the pin (exact comparison after one leading `v`), and `Skip` when `NotChosen`, `Failed` or the health call failed; `EngineSocket` is `Warn` when the socket is missing or cannot be read (the engine is optional, D24). `DoctorReport::is_healthy` is true when no check is `Fail`. All seven checks always run.
- U11-18. Test rules as U8-A9: no comments, tests under 150 lines, `TestResult`, a 10 s guard, no tokio paused time, `>=` on `FakeClock::slept` sums, no assertion on argv, order, `Display` or `Debug` text. Recorded fake limits: a download of a path outside `/sandbox` is `Transfer { op: Download }` live (P0.2-gateway.md:552) and the fake reproduces only that class; the fake produces no `Notice` event.

### API changes and decisions the plan did not fix

- `ExportReport { sandbox, bundle: Option<Sha256>, branches, detached: Option<ExportedDetached> }` (O-J). With nothing to export, no bundle is made and `bundle` is `None`. `ExportedDetached { tip, host_ref }`.
- `Unexported { uncommitted, branches, detached: Option<CommitId>, unusable: Vec<String> }`. A stash and an untracked file count as `uncommitted`; ignored files do not.
- Detached HEAD (O-A, O-K): the sandbox HEAD commit is unchanged when it equals the host HEAD commit or `refs/heads/jackin/<sb>/HEAD`. D08 says "the host branch of the same name", and a detached HEAD has no name, so the host HEAD commit is the reading that keeps "a fresh sandbox is exported". A changed detached HEAD is exported as `refs/heads/jackin/<sb>/HEAD`.
- A branch is bundled when it differs from the host branch of the same name (including one that is already exported), and is safe when it equals the host branch or its exported ref.
- New `ExportError` variants: `Process`, `Fs`, `Labels { sandbox, source }`, `HostOutput`, `RefCheckedOut { host_ref, worktree }`, `UnusableBranch { branch }`; `TipMismatch` holds `host_ref: String` instead of a `BranchName` (a detached tip has no branch name). `RefConflict` and `RefCheckedOut` are decided before any sandbox bundle exists, from the host ref list and `git worktree list --porcelain -z`.
- `Session::list` returns `SandboxListing`; `Session::inspect` returns `InspectView` (`schema: 1`); `restart_agent` maps an absent or unmanaged sandbox to `RestartError::Gateway(NotFound)` and a sandbox that is not `Ready` to `Gateway(NotReady)`.
- `attach` and `open_shell` check the phase themselves (ended gives `MainProcessEnded`, other not `Ready` gives `Gateway(NotReady)`), so the result does not depend on the gateway's own refusal.
- O-E: a sandbox without `jackin.dev/managed=true` is not found for attach, shell, restart, stop, start, inspect, export and purge (`AlreadyGone`), and `work_state` gives `Unknown(InspectionFailed)`.
- O-L: a `Deleting` sandbox is `AlreadyGone` for purge.
- O-D waiver: the expected tips come from `git bundle list-heads`; `TipMismatch` stays as a guard without a test, as the plan accepts.
- `HookFailure` replaces `LaunchError` as the error of `launch::hooks::run`, with `From` into `LaunchError` and `RestartError`; `launch::hooks` and `launch::probe` are `pub(crate)`.
- `Gateway::download_file` returns the `Sha256` of the downloaded file (it was `()`). The adapter returns the digest it already compares with the sandbox's `sha256sum`, so a returned digest is verified against the sandbox. The fake returns the digest it compares. The Transfer check `download_with_digest` asserts it equals the digest of the downloaded bytes. Export uses it for `ExportReport::bundle` and no longer reads the bundle into memory (a bundle can be up to the 4 GiB download limit). The alternative, a streaming digest on `FileSystem`, would have added a method to every `FileSystem` implementation. `rewrite/API.md` still shows `Result<(), GatewayError>` and is not edited here.
- `Rig` writes its auth JSON by hand (no `serde_json` in the testkit), from the constants that the launch tests compare against.
- New `ExportError::Entropy(EntropyError)`. `sandbox_layout::bundle_path` is deleted.

### Deviations from the plan

- P52-LIST-LABELS: "one managed sandbox with a name that does not start with `jackin-`" cannot exist. `SandboxName` rejects such a name, and the adapter fails a listing that returns one (`view_of`). The managed sandbox with other labels uses `jackin-custom-9999`; the unmanaged `jackin-ghost-0000` covers the name-prefix mutation from the other side.
- P52-EXP-WORKTREE: the plan's mutation (`--git-dir=<root>/.git` on the `.git` file) stays green. Git accepts a gitfile as `--git-dir` and still writes shared refs to the common directory, so the test pins the positive and negative outcome only.
- P52-EXP-GIT: the plan and the first version held that the all-or-nothing fetch (`--atomic`) is not observable once the conflict pre-check exists. The red/green review showed that it is: a stale host lock file `refs/heads/jackin/<sb>/b.lock` fails the update of one ref only, and plain `git fetch` still creates `a`. `a_stale_lock_on_one_target_ref_...` (P52-FIX-ATOMIC) pins it. The ref-collision test is unchanged.
- P52-EXP-CONCURRENT is kept. It found a real defect: the sandbox bundle path was fixed (`bundle_path`), so two exports of one sandbox collided on the git lock of the sandbox file (`SandboxGit { 128 }`). The first fix serialized the exports with a per-sandbox host lock. The review pointed at the cause, shared state, so each export now has its own sandbox bundle path (U11-7) and the lock and its lock file are gone. Two exports of one sandbox still update the same host refs; git's own ref lock covers that and the test is green 30 of 30.
- P52-ATT-OPEN: with the Ready check removed, the fake's refused attach opens and the following state read fails, so the red line is `Gateway(NotReady { phase: Completed })` instead of the plan's `Ok`; the test still fails on the wrong error.
- P52-POWER-ENDED: "the pre-check runs after the call" is the same observable as "no pre-check" (the call errors first), so one mutation covers both.
- The three mutations that the first record called not runnable (`AlreadyGone` taken from the delete result, the provider deleted with the sandbox, the supervisor file read for work state) are observable and red; they are the three last rows of the fix-round table. The first record was wrong: it reasoned about the code instead of running the mutation.
- `tests/project` is not changed. The launch tests changed only in how they build the Rig; every launch assertion is unchanged (P52-FIX-LAUNCH-RIG).

### Live evidence items and the driver

`cargo` runs through `mise exec -- mbx +1.99.1 run -p jackin-session --example export_live --`. The image must exist in the gateway (`--image`). The driver prints only synthetic tokens (`jackin-p52-driver-*`). Flags: `--gateway` (default `jackin-local`), `--workspace` (default `default`), `--project`, `--openshell-home`, `--work-dir`, `--cleanup-refs`.

| Item | Command |
|---|---|
| Export from a normal repository, a refused guarded purge, a forced purge | `export_live scenario --image <tag> --project <scratch git repo> --cleanup-refs` |
| Export from a linked worktree | `export_live scenario --image <tag> --project <linked worktree root> --cleanup-refs`; the printed `refs/heads/jackin/*` list comes from the shared repository |
| The two new contract checks live | `export_live contract --image <tag> --project <scratch git repo>`; the Transfer group must read `total=6 failed=0` with `CHECK Transfer download_missing_remote_is_a_size_error Ok` and `CHECK Transfer download_pre_cancelled_is_cancelled Ok` |

### Red and green proof (P5.2)

Every mutation was applied to the committed tree, the named test was run with `mise exec -- mbx +1.99.1 nextest run`, and the tree was restored. The scripts are `mut.py` and `export_muts.py` in the scratch directory (`/private/tmp/claude-501/p52-impl`). A mutation that failed to compile was rewritten until it compiled; only runtime reds are listed. The green run of every row is the final gate. Test names are in `crates/jackin-session/tests/<target>/<file>.rs` unless a crate is named.

| Row | Test | Mutation | Red line |
|---|---|---|---|
| P52-CON-MISSING | `transfer_group_passes_on_the_fake` (jackin-testkit gateway/contract.rs) | the fake keeps `Transfer { op: Download }` | `left: [("download_missing_remote_is_a_size_error", "a download of a missing file is not a Size error: Err(Transfer { op: Download, status: Exited(1), ...` |
| P52-CON-MISSING | `the_download_checks_pass_on_the_adapter_when_the_sandbox_answers_like_the_real_tools` (jackin-openshell gateway/contract.rs) | the adapter ignores a failed `stat` (`remote_size(..).await.unwrap_or(0)`) | `contract.rs:487 left: Err("a download of a missing file is not a Size error: Err(Transfer { op: Download, status: Exited(1), tail: ... })") right: Ok(())` |
| P52-CON-MISSING | `a_gateway_that_finds_a_missing_file_and_fails_later_fails_the_size_check` | (negative case of the check itself) a server whose `stat` always succeeds | passes: the check message names `Digest` |
| P52-CON-CANCEL | `transfer_group_passes_on_the_fake` and `a_cancel_armed_at_the_download_ends_the_call_as_cancelled_and_leaves_no_file` (jackin-testkit) | the fake ignores the cancel | `left: [("download_pre_cancelled_is_cancelled", "a download with a cancelled token is not Cancelled: Ok(())")]`; `transfer.rs:125` panicked |
| P52-LIST-LABELS | `list::list_returns_exactly_the_managed_sandboxes_with_their_labels_and_phases` | list runs `git` in each sandbox | `list.rs:105 left: [(SandboxName("jackin-custom-9999"), {}, 1) ...] right: [(..., {}, 0) ...]` |
| P52-LIST-LABELS | same | list reads `RUN_STATE` in each sandbox | `Error: Gateway(NotReady { phase: Error { exit_code: Some(7) } })` |
| P52-LIST-LABELS (O-F) | `list::a_managed_sandbox_with_unreadable_labels_is_reported_apart_and_the_rest_is_listed` | one bad label fails the whole list | `Error: Labels { sandbox: SandboxName("jackin-broken-0002"), source: Missing { key: "jackin.dev/agent" } }` |
| P52-POWER-CYCLE | `power::a_sandbox_whose_main_process_ended_refuses_stop_and_start_and_stays_as_it_was` (its Ready control) | no wait for the phase (`while false`) | `power.rs:110 left: Starting right: Ready` |
| P52-POWER-CYCLE | `power::stop_and_start_keep_the_files_and_reset_the_supervisor_and_a_stuck_start_times_out` | no timeout | `Error: "the test hung for 10 s"` |
| P52-POWER-CYCLE | same | a second stop on a stopped sandbox refused | `Error: NotFound(SandboxName("jackin-platfor-ij4f"))` |
| P52-POWER-ENDED | `power::a_sandbox_whose_main_process_ended_refuses_stop_and_start_and_stays_as_it_was` | no pre-check (also the pre-check after the call) | `Error: "Completed { exit_code: 0 } Stop: saw Err(Gateway(MainProcessFailed {...}))"` |
| P52-POWER-ENDED | same | pre-check only `Completed` | `Error: "Error { exit_code: Some(3) } Stop: saw Err(Gateway(MainProcessFailed {...}))"` |
| P52-POWER-ENDED | same | pre-check only `Error { Some }` and `Completed` | `Error: "Error { exit_code: None } Stop: saw Err(Gateway(Protocol { message: \"stop sandbox failed: persist stop marker failed...` |
| P52-ATT-OPEN | `open::attach_refuses_an_absent_a_foreign_a_stopped_and_an_ended_sandbox_without_a_terminal` | the ended check removed | `Error: "Completed { exit_code: 0 }: saw Err(Gateway(NotReady { phase: Completed { exit_code: 0 } }))"` |
| P52-ATT-OPEN | same | `NotFound` mapped to `Gateway` | `open.rs:87 assertion failed` (the `NotFound` match) |
| P52-ATT-OPEN | `open::attach_opens_the_main_terminal_reads_the_state_at_attach_and_detach_leaves_the_sandbox` | the reading fixed instead of read at attach | `FAIL` at the `exited 1 3` reading |
| P52-ATT-LEASE | `lease::a_second_owner_is_read_only_until_the_first_leaves_and_a_viewer_never_writes` | `viewer` maps to `Owner` | `lease.rs:49 assertion failed: dropped(&handle, &mut watcher, b"v").await?` |
| P52-ATT-LEASE | `lease::*`, `open::*`, `replies::*` | `AttachedTerminal::input` drops everything | `Error: "the condition never held"` |
| P52-ATT-REPLIES | `replies::terminal_replies_reach_the_agent_only_while_it_runs_and_typed_keys_always_pass` | filter always on | `replies.rs:37 left: [104, 105, 121, 111] right: [104, 105, 27, 91, 49, 50, 59, 52, 48, 82, 121, 111]` |
| P52-ATT-REPLIES | same | filter always off | `replies.rs:46 left: [104, 105, 27, 91, 49, 50, 59, 52, 48, 82, 121, 111] right: [104, 105, 121, 111]` |
| P52-ATT-REPLIES | same | state read once at attach | `replies.rs:46` same as always off |
| P52-SHELL-ENV | `shell::a_shell_sees_the_role_env_untouched_and_starts_in_the_project_without_touching_codex` | workdir not the project | `shell.rs:47 left: None` (no `greeting.txt` in `/sandbox/project`) |
| P52-SHELL-ENV | same | env not passed | `shell.rs:47 left: Some([]) right: Some([97, 39, 98, 32, 34, 99, 34, 32, 36, 120])` |
| P52-SHELL-ENV | `shell::a_shell_is_refused_without_a_usable_env_file_a_launch_label_or_a_ready_sandbox` | a missing env file treated as empty | `shell.rs:67` panicked (expected `EnvFile { Missing }`) |
| P52-RESTART | `restart::restart_refuses_a_running_agent_and_otherwise_runs_preflight_and_requests_the_next_attempt` | `running` not refused | `restart.rs:31 Err(AttemptsExhausted { sandbox: SandboxName("jackin-platfor-ij4f") })` |
| P52-RESTART | same | preflight skipped | `restart.rs:53 left: 1` (preflight run count not raised) |
| P52-RESTART | `restart::a_failing_preflight_stops_the_restart_before_any_request_is_written` | the request written after a failed preflight | `restart.rs:100` panicked |
| P52-EXP-REFS | `refs::export_writes_only_the_changed_branches_under_the_sandbox_prefix_and_never_the_host_state` | refspec without the `jackin/<sb>/` prefix | `HostGit { status: Exited(128), output: "fatal: refusing to fetch into branch 'refs/heads/main' checked out at '...'" }` |
| P52-EXP-REFS | same | the `+` dropped | `Error: HostGit { status: Exited(1), output: TerminalText("") }` (the amended re-export is refused) |
| P52-EXP-REFS | same | `reset --hard` after the fetch | `refs.rs:41 left: HostState { index: [68, 73, 82, 67, ...` differs |
| P52-EXP-REFS | same | `update-index --refresh` after the fetch | `Error: HostGit { status: Exited(1), output: TerminalText("src/main.rs: needs update\n") }` |
| P52-EXP-REFS | same | `git config` write after the fetch | `refs.rs:41 left: HostState { ... } right: HostState { ... }` (config bytes differ) |
| P52-EXP-WORKTREE | `worktree::export_from_a_linked_worktree_writes_the_shared_repository_and_no_worktree_state` | `--git-dir=<root>/.git` on the `.git` file | GREEN (git accepts a gitfile); see deviations |
| P52-EXP-NEST | `nesting::three_export_rounds_keep_a_flat_ref_set_with_only_changed_branches` | `--tags` added after the refs of the sandbox `git bundle create` | `nesting.rs:41 round 1 left: {"refs/heads/jackin/jackin-platfor-ij4f/refs/tags/tag-1", "refs/heads/jackin/jackin-platfor-ij4f/work-1"} right: {"refs/heads/jackin/jackin-platfor-ij4f/work-1"}` (the first record used a tag refspec glued to the branch refspec, which is an invalid refspec and not the mutation) |
| P52-EXP-EMPTY | `empty::a_repository_with_no_commits_exports_a_sandbox_branch_into_a_missing_bundle_directory` | `mkdir -p` of another directory | `Error: SandboxGit { exit_code: 128, output: "fatal: Unable to create '.../sandbox/.jackin/bundle/export-wk5kcrrepognul76xqrlxhfczr.bundle.lock': No such file or directory" }` |
| P52-EXP-REFUSE | `refuse::export_refuses_before_any_sandbox_or_host_write` | the Ready check removed | `refuse.rs:30 Err(Gateway(NotReady { phase: Stopped }))` |
| P52-EXP-REFUSE | same | a sandbox write before the host resolution | `refuse.rs:48 assertion left == right failed` (file map differs) |
| P52-EXP-ABORT | `abort::an_interrupted_export_leaves_no_ref_and_no_staging_file_and_a_retry_succeeds` | cancel mapped to `Gateway` | `abort.rs:34 Err(Gateway(Cancelled { op: Download }))` |
| P52-EXP-ABORT | same | staging directory left behind | `abort.rs:72 left: ["export-jackin-platfor-ij4f-6uvvgrfpywxx6af44hwuyq23f4", "export-jackin-platfor-ij4f-2otldei2veg5acvs6qveo4cdka", ...] right: []` |
| P52-EXP-ABORT | same | download error ignored | `abort.rs:34 Err(Gateway(NotFound { what: Sandbox(...) }))` |
| P52-EXP-GIT | `git::a_ref_name_collision_is_a_ref_conflict_and_the_fetch_is_all_or_nothing` | conflict pre-check removed | `Error: "expected a conflict, saw Err(HostGit { status: Exited(254), output: \"error: cannot lock ref 'refs/heads/jackin/jackin-platfor-ij4f/z/deep': ... exists; cannot create ...` |
| P52-EXP-GIT | same | pre-check removed and no `--atomic` | `Error: "expected a conflict, saw Err(HostGit { status: Exited(1), output: \"error: some local refs could not be updated; try running\n 'git remote prune ...` |
| P52-EXP-DETACHED | `detached::a_detached_head_commit_is_unexported_until_it_is_exported_as_the_head_ref` | the detached HEAD ignored when choosing refs | `Error: "no detached head exported"` |
| P52-EXP-DETACHED | same | the detached HEAD never unexported | `detached.rs:26 left: Exported right: Unexported(Unexported { ... detached: Some(CommitId(...)) ... })` |
| P52-EXP-UNNAMEABLE | `unusable::a_sandbox_branch_under_jackin_fails_the_export_and_is_never_exported` | the unusable branch skipped | `unusable.rs:24` panicked (no `UnusableBranch`) |
| P52-EXP-UNNAMEABLE | `purge::a_guarded_purge_refuses_every_state_that_is_not_exported_and_changes_nothing` | `work_state` ignores `unusable` | `purge.rs:22 assertion left != right failed left: Exported right: Exported` |
| P52-EXP-HEADCHECKOUT | `unusable::a_target_ref_checked_out_in_any_host_worktree_is_refused_with_no_ref_change` | the worktree check removed | `Error: "expected a refusal, saw Err(HostGit { status: Exited(128), output: \"fatal: refusing to fetch into branch 'refs/heads/jackin/jackin-platfor-ij4f/main' checked out at ...` |
| P52-EXP-CONCURRENT | `concurrent::two_concurrent_exports_of_one_sandbox_both_succeed_and_leave_no_staging_and_no_bundle` | the unique sandbox bundle path made fixed again (`export.bundle`) | red in 10 of 10 runs: `Error: SandboxGit { exit_code: 128, output: "fatal: Unable to create '.../sandbox/.jackin/bundle/export.bundle.lock': File exists.` ; green in 30 of 30 runs with the unique path (12, then 18 more) |
| P52-WS-FRESH | `work_state::an_untracked_file_alone_counts_as_uncommitted` and the purge matrix | `uncommitted` ignores untracked files (`--untracked-files=no`) | `work_state` row `an_untracked_file_alone_counts_as_uncommitted` FAIL; `purge.rs:22 assertion left != right failed left: Exported` |
| P52-WS-FRESH | `empty::a_repository_with_no_commits_exports_...` and `detached::*` | branches compared by name only | `empty.rs:28 left: Exported right: Unexported(Unexported { uncommitted: false, branches: [BranchName("main")], ... })` |
| P52-WS-FRESH | `work_state::a_fresh_launch_is_exported_and_only_real_changes_make_it_unexported` | a stash ignored | `work_state.rs:57 left: Exported right: Unexported(Unexported { uncommitted: true, ... })` |
| P52-WS-MATRIX | `work_state::what_jackin_cannot_inspect_is_unknown_and_a_forged_supervisor_changes_nothing` | an inspection failure maps to `Exported` | `work_state.rs:118 left: None right: Some(InspectionFailed)` |
| P52-WS-MATRIX | `work_state::a_launch_that_failed_before_the_upload_has_no_repository` | `NoRepository` and `InspectionFailed` swapped | `work_state.rs:152 left: Some(InspectionFailed) right: Some(NoRepository)` |
| P52-PURGE-GUARD | `purge::a_guarded_purge_refuses_every_state_...` and `purge::a_launch_without_a_repository_is_refused_too` | the guard checks only `Unexported` (so `Unknown` passes) | `Error: "no repository: expected a refusal, saw Ok(Deleted { ..., was: Unknown(NoRepository) })"` ; `"no workspace: ... Unknown(WorkspaceUnknown(...))"` |
| P52-PURGE-GUARD | same | the guard checks only `uncommitted` | `Error: "branch: expected a refusal, saw Ok(Deleted { ..., was: Unexported(Unexported { uncommitted: false, branches: [BranchName(\"main\")], ...` |
| P52-PURGE-GUARD | same | purge exports first and then deletes | `Error: "untracked: expected a refusal, saw Ok(Deleted { ..., was: Unexported(Unexported { uncommitted: true, ...` |
| P52-PURGE-OUTCOME | `purge::an_exported_sandbox_is_deleted_alone_and_a_forced_purge_reports_what_it_deleted` | a forced purge reports `Exported` | `purge.rs:158 left: Deleted { ..., was: Exported } right: Deleted { ..., was: Unexported(Unexported { ... branches: [BranchName("main")] ...` |
| P52-FOREIGN | `foreign::a_sandbox_without_jackin_labels_is_never_purged_stopped_started_or_exported` | the managed-label filter of `find_managed` removed | `foreign.rs:24 left: Deleted { sandbox: SandboxName("jackin-ghost-0000"), was: Unknown(InspectionFailed) } right: AlreadyGone { ... }` |
| P52-INSPECT | `inspect::inspect_reports_the_sandbox_and_flags_only_a_foreign_rule_on_a_codex_host` | no `generated` exemption | `inspect.rs:42` panicked (the generated `_provider_` rule reported) |
| P52-INSPECT | same | every provider-named rule exempt | `inspect.rs:63 left: [] right: [("_provider_other", "api.openai.com")]` |
| P52-DOC-MATRIX | `matrix::each_problem_flips_only_its_own_check_and_nothing_is_repaired` | checks short-circuit after a failed binary | `left: [(OpenshellBinary, Fail)] right: [(OpenshellBinary, Fail), (SshBinary, Pass), ...]` |
| P52-DOC-MATRIX | same | versions compared by prefix | `assertion left == right failed: prefix of the version` (`Pass` instead of `Fail`) |
| P52-DOC-MATRIX | same | `Failed` access gives `Skip` | `failed access left: [... (GatewayHealth, Skip), (PinnedVersion, Skip) ...] right: [... (GatewayHealth, Fail), (PinnedVersion, Skip) ...]` |
| P52-DOC-MATRIX | same | `is_healthy` ignores `Fail` | `no openshell left: true right: false` |
| P52-DOC-GIT | `git::the_git_check_agrees_with_the_launch_check_for_the_same_version_output` | doctor only checks that `git --version` runs | `git.rs:41 ... [103, 105, 116, 32, 118, ...] (git version 2.44.9) left: Some(Pass) right: Some(Fail)` |

Rows without a table line: P52-EXP-ABORT "a temporary file is left behind" is the staging line above; the P52-EXP-REFUSE "removed workspace directory gives `Project`" is a positive case in the same test.

### Fix round after the independent red/green review (CHANGES on 69c8e104)

The review report is `plans/P5.2-redgreen.md`; its surviving mutants 1 to 8 and its required fixes are binding. Every row below was run red on the final tree with one mutation, then green with the committed code. The scripts are `mut.sh` and `loop.sh` in `/private/tmp/claude-501/p52-fix`. Mutation 9 of the report (the power "ended while waiting" branch, marked low) is not covered: it needs a fake that ends the phase during a start, and it is not in the fix-round brief.

Changes without a test row of their own: the bundle digest comes from `download_file` (API changes above); `attach.rs` has its SPDX header again; the sandbox bundle path is unique per export and removed after the download (U11-7).

| Row | Test | Mutation | Red line |
|---|---|---|---|
| P52-FIX-ATOMIC | `git::a_stale_lock_on_one_target_ref_fails_the_fetch_as_a_host_git_error_and_exports_no_ref` | `--atomic` removed from the host fetch | `git.rs:129` panicked, the message is the exported ref map `{"refs/heads/jackin/jackin-platfor-ij4f/a": "badb9988..."}` (the fetch created `a` and failed on `b`) |
| P52-FIX-CANCEL-PROC | `abort::a_cancel_that_ends_a_host_git_call_is_cancelled_and_leaves_nothing` | the `ProcessError::Cancelled` arm of `From<ProcessError> for ExportError` removed | `abort.rs:96` panicked: `Err(Process(Cancelled { program: Git }))`. The test runs the export on a session whose process runner (`CancelsOn`, a wrapper of the testkit runner) cancels the token when `git bundle verify` starts |
| P52-FIX-WS-IGNORED | `work_state::a_fresh_launch_is_exported_and_only_real_changes_make_it_unexported` | `--ignored` added to the sandbox `git status` | `work_state.rs:39 left: Unexported { uncommitted: true, branches: [], .. } right: Exported`. The ignored file `project/target/x` is written inside the sandbox |
| P52-FIX-POWER-LIMIT | `power::stop_and_start_keep_the_files_and_reset_the_supervisor_and_a_stuck_start_times_out` | the wait uses `limits.hook` instead of `limits.ready` | `power.rs:82` panicked: `600s` is outside `3s..=4s` |
| P52-FIX-SHELL-ENDED | `shell::a_shell_on_a_sandbox_whose_main_process_ended_is_main_process_ended` | `open_shell` refuses a phase other than `Ready` with `NotReady` | `Error: "Completed { exit_code: 0 }: saw Err(Gateway(NotReady { phase: Completed { exit_code: 0 } }))"` |
| P52-FIX-RESTART-ENDED | `restart::a_restart_on_a_sandbox_whose_main_process_ended_is_main_process_ended_and_writes_nothing` | `restart_agent` refuses a phase other than `Ready` with `NotReady` | `Error: "Completed { exit_code: 0 }: saw Err(Gateway(NotReady { phase: Completed { exit_code: 0 } }))"` |
| P52-FIX-RESTART-ENV | `restart::restart_refuses_a_running_agent_and_otherwise_runs_preflight_and_requests_the_next_attempt` | the restart preflight runs with an empty env | `restart.rs:58 left: Some({}) right: Some({EnvName("GREETING"): "from-launch"})` (the env on the hook run record) |
| P52-FIX-DOC-SERVING | `matrix::each_problem_flips_only_its_own_check_and_nothing_is_repaired` (case `not serving`) | `serving: false` treated as serving | `matrix.rs:137 not serving left: [.., (GatewayHealth, Pass), ..] right: [.., (GatewayHealth, Fail), ..]`. The fake gained `FakeGateway::set_serving` |
| P52-FIX-DOC-EXIT | same (cases `openshell exits non-zero` and `ssh exits non-zero`) | a version call that exits non-zero passes (all binaries; then `ssh` only) | `openshell exits non-zero left: [(OpenshellBinary, Pass), ..] right: [(OpenshellBinary, Fail), ..]`; `ssh exits non-zero left: [.., (SshBinary, Pass), ..]` |
| P52-FIX-BUNDLE-PATH | `concurrent::two_concurrent_exports_of_one_sandbox_...` | see P52-EXP-CONCURRENT (fixed path): red 10 of 10 | `export.bundle.lock: File exists` |
| P52-FIX-BUNDLE-DELETE | the same test, `refs::export_writes_only_...`, `abort::an_interrupted_export_...` and `abort::a_cancel_that_ends_a_host_git_call_...` | the sandbox bundle is not removed after the download | `concurrent.rs:32 left: ["/sandbox/.jackin/bundle/export-l2f6vhz42354sor4x7v2txwd56.bundle", ..] right: []`; `refs.rs:65` (one bundle); `abort.rs:73` (three bundles: cancelled, corrupt and failed downloads); `abort.rs:104` (one bundle after a cancel) |
| P52-FIX-DIGEST | `refs::export_writes_only_the_changed_branches_under_the_sandbox_prefix_and_never_the_host_state` | the report digest is `Sha256::of(b"")` | `refs.rs:61 left: Some(Sha256("e3b0c442..855")) right: Some(Sha256("2f40d470..."))`. The oracle is a bundle that the test builds in the sandbox with the same refs after the export (the export deletes its own bundle) |
| P52-FIX-DIGEST | `contract::the_download_checks_pass_on_the_adapter_when_the_sandbox_answers_like_the_real_tools` (jackin-openshell), `contract::transfer_group_passes_on_the_fake` and `transfer::a_failed_download_leaves_no_local_file_and_a_good_one_writes_it` (jackin-testkit) | the adapter, then the fake, returns `Sha256::of(b"")` | `contract.rs:487 left: Err("the returned digest e3b0c442..855 is not the digest of the bytes") right: Ok(())`; `transfer.rs:87 left: Sha256("e3b0c442..855") right: Sha256("eb333942...")`; `left: [("download_with_digest", "the returned digest e3b0c442..855 is not the digest of the bytes")]` |
| P52-FIX-LAUNCH-RIG | the whole `launch` binary (46 tests, 60 s guard each, assertions unchanged) | hook exit code ignored (`if true` for the exit check in `launch::hooks::run`) | red in 3 tests: `hooks::a_failing_hook_stops_the_launch_with_name_code_and_sanitized_output`, `hooks::hooks_run_with_the_env_file_values_and_a_retry_replaces_them`, `events::a_failed_or_cancelled_run_ends_its_events_at_the_failing_step` (`events.rs:140`) |
| P52-FIX-LAUNCH-RIG | same | hooks run with an empty env | `hooks.rs:154 left: {} right: {EnvName("GREETING"): "first"}` |
| P52-FIX-LAUNCH-RIG | same | the ready limit is 100 times `limits.ready` | `phases::a_sandbox_that_settles_too_slowly_times_out_on_the_clock_and_a_patient_launch_joins_it`: `Error: "expected a failed launch, saw Ok(Ready(AttachTarget { .. }))"` |
| P52-PURGE-OUTCOME (was "not run") | `foreign::a_sandbox_without_jackin_labels_is_never_purged_stopped_started_or_exported`, `purge::a_repeated_purge_is_already_gone_a_failed_delete_keeps_the_sandbox_and_a_deleting_one_is_gone` | `AlreadyGone` taken from the delete result (an absent sandbox is deleted and reported `Deleted { was: Unknown(InspectionFailed) }`) | `foreign.rs:24 left: Deleted { sandbox: "jackin-ghost-0000", was: Unknown(InspectionFailed) } right: AlreadyGone { .. }`; `purge.rs:217` |
| P52-PURGE-OUTCOME (was "not run") | `purge::a_repeated_purge_is_...`, `purge::an_exported_sandbox_is_deleted_alone_and_a_forced_purge_reports_what_it_deleted` | the providers of the sandbox are deleted after the sandbox | `Error: Gateway(FailedPrecondition { message: "provider 'jackin-work-codex' is attached to sandbox(es): jackin-platfor-uxe1" })` (the provider is shared; the fake refuses it, as the live contract does) |
| P52-WS-MATRIX (was "not run") | `work_state::what_jackin_cannot_inspect_is_unknown_and_a_forged_supervisor_changes_nothing` | `work_state` reads `RUN_STATE` and turns `exited` into `Unexported` | `work_state.rs:136 "exited 1 0\n" left: Unexported { uncommitted: true, branches: [], .. } right: Unexported { uncommitted: false, branches: [BranchName("main")], .. }` |

The launch suite ran green on the shared Rig before and after these mutations (46 of 46 in the `launch` binary; 231 of 231 in `jackin-session` and `jackin-testkit` after the migration commit).
