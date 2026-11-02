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
| `jackin-xtask` | none; optional (feature `e2e`): core, openshell, `jackin-usage` from U13 | testkit | `portable-pty` and `schemars`, optional (feature `e2e`) |

The gateway crates are `openshell-sdk`, `openshell-policy`, `tonic`, and `russh`; the common set is `serde`, `thiserror`, `tokio`, `tokio-util`, `async-trait`, and `clap` where a crate has a CLI. Rules that follow from the table. `image` and `openshell` never depend on each other. `session` never depends on `jackin-usage`, and no crate outside `crates/usage/` names a `jackin-usage-*` crate other than the `jackin-usage` facade. `docker` is started only from `jackin-image`; the `containers` step enforces it, and the layer table does not. `Secret::expose` is called only from an allowlist of files. `jackin-testkit` depends on `jackin-tui`, and `jackin-tui` dev-depends on `jackin-testkit`: Cargo allows this cycle for dev-dependencies, which is why every test is an integration test (below).

### State owners

| State | Single owner | Others |
|---|---|---|
| `config.toml` | `ConfigStore`, one writer API `edit(Vec<ConfigEdit>)` under a file lock | read a `Config` snapshot |
| Sandbox lifecycle, labels, providers, policy | the gateway | jackin reads through `Gateway` |
| Launch progress | observed sandbox state (labels, phase, marker files) | the executor keeps one per-run `Done` value |
| `/sandbox/.jackin/run/start` | jackin (`Session`) | `jackin-start` reads |
| `/sandbox/.jackin/run/state` | `jackin-start` | jackin reads and treats as advisory (D09) |
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
- We accept that `jackin-session::host` holds the real `ProcessRunner` and `Clock` (PLAN P5.2) and that `jackin-core` holds only `StdFileSystem`, so D14's "no I/O except config files" stays true. `xtask` cannot use `jackin-session`, so it carries a private runner of about 50 lines for the `openshell` calls of its `GrpcGateway` and a plain `tokio::process` call for the `jackin` binary.
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
| `BranchName` | valid per `git check-ref-format`, no leading `jackin/` |
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
    pub fn fixed() -> [&'static SandboxPath; 10];
    pub fn uploaded_marker(launch: &LaunchId) -> SandboxPath;
    pub fn setup_done_marker(launch: &LaunchId) -> SandboxPath;
    pub fn env_file(launch: &LaunchId) -> SandboxPath;
    pub fn hook_path(kind: HookKind) -> SandboxPath;
    pub fn bundle_path(sandbox: &SandboxName) -> SandboxPath;
}```

`SandboxPath` is absolute, normalized, and has no `..`. `from_static` is `const` and does not validate, because `panic` is denied and a `const fn` cannot return an error: a test in `jackin-core` parses every `sandbox_layout::fixed()` entry and every `AgentSpec::paths()` entry and requires the parsed value to equal the constant. `RepoPath` is relative with no `..` and no empty or `.` segment (D11). `HostPath::resolve` joins a relative CLI argument to the working directory and normalizes it without touching the disk. `ProjectRoot::open` checks that the path is an existing directory with a `.git` entry (D08 needs a work tree). `sandbox_layout` is the single owner of every in-sandbox path that jackin and `jackin-start` agree on (D09, D11), including the env file, and of the three absolute program paths that `run_program` uses (`/usr/bin/git`, `/usr/bin/printenv`, `/usr/bin/sha256sum`). `jackin-image` embeds `jackin-start`, and a test there asserts the script text contains each constant, so the script and the Rust cannot drift.

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

`Secret` and `SecretBytes` have a redacting `Debug`, no `Display`, no `Serialize`, and zero their memory on drop. `expose` is the only way out, and the `xtask` layer step lists the files allowed to call it (the OpenShell adapter, the credential resolver, the `auth.json` and Docker client file writers). `TerminalText::sanitize` removes ESC sequences and control bytes except `\n` and `\t`, and replaces invalid UTF-8. Hook output (D11) and build tails pass through it before they enter an error.

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
    async fn list(&self, dir: &HostPath) -> Result<Vec<DirEntry>, FsError>;
    async fn create_dir_all(&self, dir: &HostPath) -> Result<(), FsError>;
    async fn write_new(&self, path: &HostPath, bytes: &[u8], mode: FileMode) -> Result<(), FsError>;
    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), FsError>;
    async fn remove_file(&self, path: &HostPath) -> Result<(), FsError>;
    async fn sync_dir(&self, dir: &HostPath) -> Result<(), FsError>;
    async fn lock(&self, path: &HostPath) -> Result<FileLock, FsError>;
}

pub struct FileMode(pub u32);
pub enum FileKind { File, Dir, Symlink }
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
    pub env: Vec<(String, String)>,
    pub stdin: ProcessStdin,
    pub timeout: Duration,
}
pub enum ProcessStdin { Null, Bytes(SecretBytes) }
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

`ProcessStdin::Bytes` carries a secret, so a secret never sits in `args` (D10). With `lines` set, the runner streams complete lines to the sink and returns a stdout and stderr tail of at most 64 KiB; with `lines` unset it returns all output. Cancellation kills the child and returns `ProcessError::Cancelled`. The runner always applies `spec.timeout` and takes stdin from `/dev/null` for `ProcessStdin::Null` (D03, P0.2 G1). The real `ProcessRunner` and `Clock` live in `jackin-session::host`, and so does `capture_env`, the only reader of the process environment; `StdFileSystem` lives here.

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

These views derive `Serialize`, `Deserialize`, and `JsonSchema`. `xtask e2e` parses `jackin ls --json` and `jackin inspect --json` into them. `Compatibility::pinned` parses `compatibility.lock.toml` embedded with `include_str!` and returns an error value, so the OpenShell pin has one file (D02) and no panic path. The binary calls it once and passes the value to `doctor`. `BuildOutput` is what `jackin build --json` prints, so the final image ID is observable without `docker image inspect` (J03). The usage view belongs to `jackin-usage` (section 5).

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
    Timeout { program: Program, after: Duration },
    Cancelled { program: Program },
}
pub enum PollError<E> { Timeout { after: Duration }, Cancelled, Probe(E) }
```

All of these derive `Debug` and `thiserror::Error`. No variant holds a `Secret`. Errors that carry process or hook text hold `TerminalText` that already passed a `Redactor`. The `Display` of `ConfigError::NoWorkspaceForPath` names `jackin workspace add` (decision 8).

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
- `ImageKey::derive` is the SHA-256 of a length-prefixed concatenation of: role name, `RoleRevision` (commit, plus the content digest when dirty), the role Dockerfile digest, the built construct image ID, the Codex version and the archive checksum of the platform from the `AgentSpec`, the platform, the `DockerLayer`, the digest of the embedded `jackin-start`, the digest of the embedded derived Dockerfile, and the path and digest of each hook file. The function destructures `ImageRequest` exhaustively, so a new request field that does not feed the key does not compile. The construct image ID, not the construct Dockerfile or its base digest, is the input (D05): a rebuilt construct with a new ID changes every final tag.
- The final tag is `ImageTag::for_image(role, key)`, and the role-stage tag is `jackin-role/<role>:<16 hex>`. Both come from the key, so tag equals cache key.
- `final_builds` renders two `PlannedBuild` values: role (`--build-context construct=docker-image://<construct tag>`, context is the role repository) and derived (`--build-context role=docker-image://<role tag>`, context materialized with `jackin-start` and the hook files). Each carries `--builder <name>`, `--platform <platform>`, `--provenance=false`, `--sbom=false`, `--load`, `--progress=plain`, and `--no-cache` when `Rebuild::Yes` (D05). The derived build passes `CODEX_VERSION`, `CODEX_BIN_DIR`, and both archive checksums from the `AgentSpec` row, and the Docker client argument only for `DockerLayer::Client` (D06).
- `ensure_image` runs in this order. Validate the Dockerfile. `find` the construct tag, or build it when absent or when `Rebuild::Yes`, and take its ID. Derive the final tag. `find` the final tag and return `ImageSource::Cache` on a hit unless `Rebuild::Yes`. Otherwise `check_builder` once, build the role stage, build the derived stage, and return `ImageSource::Built` with the ID that the derived build returned. Build lines stream to `progress` as `Line` events. A cancel kills the build and returns `ImageError::Cancelled`.
- `BuildxEngine::check_builder` runs `docker buildx inspect <builder>` and requires the driver `docker` (`EngineError::BuilderDriver`, D05 step 3). `build` writes the materialized context into a fresh per-call directory under `work_dir` through `FileSystem::write_new`, runs `docker buildx build` with a metadata file, reads the image ID, and removes the directory on every exit path. `find` runs `docker image inspect --format {{.Id}}`.
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
    Cancelled,
}
pub enum ImageError { Dockerfile(DockerfileError), MissingInstallTarget { platform: Platform }, Engine(EngineError), Cancelled }
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

    async fn upload_project(&self, sandbox: &SandboxName, upload: &ProjectUpload, cancel: &CancellationToken) -> Result<(), GatewayError>;
    async fn download_file(&self, sandbox: &SandboxName, remote: &SandboxPath, local: &HostPath, cancel: &CancellationToken) -> Result<(), GatewayError>;

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
pub struct ProjectUpload { pub source: ProjectRoot, pub dest: SandboxPath }

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
- `upload_project`: runs the two `openshell sandbox upload` calls of D08 with the project root as the working directory, stdin from `/dev/null`, explicit gateway and workspace, and a hard timeout. It fails with `Transfer` on a non-zero exit. Overwrites; safe to repeat.
- `download_file`: runs `openshell sandbox download`, then compares the SHA-256 of the downloaded bytes with the digest that `sha256sum` reports in the sandbox. A mismatch fails with `Transfer { op: Digest }`.
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
pub struct ClosureViolation { pub rule: RuleKey, pub host: HostPattern, pub provider_host: HostPattern }

impl PolicyDocument {
    pub fn parse(text: &str) -> Result<PolicyDocument, PolicyError>;
    pub fn parse_authored(text: &str) -> Result<PolicyDocument, PolicyError>;
    pub fn network_rules(&self) -> Vec<NetworkRule>;
    pub fn closure_violations(&self, profile: &ProviderProfile, generated: &[RuleKey]) -> Vec<ClosureViolation>;
    pub fn with_rule(&self, rule: NetworkRule) -> Result<PolicyDocument, PolicyError>;
    pub fn render(&self) -> String;
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

`PolicyDocument` types only the `network_policies` view; the rest of the document is the author's. The adapter converts the document to the gateway policy message through `openshell-policy` with no change of its own (D12), so `render` is for display and `jackin inspect`. `parse_authored` rejects a rule key that starts with `_provider_`, which OpenShell reserves for the rule it generates. `parse` accepts those keys, for the live policy that `sandbox_detail` reads back. `closure_violations` implements the D10 provider host closure: any rule whose endpoint host pattern overlaps a profile host, on any port and with any binary, is a violation. `generated` lists the rule keys that are exempt. A caller checking a live policy passes `RuleKey::provider_rule(p)` for each OpenShell provider attached to the sandbox, so the generated `_provider_<name>` rule is not reported, while a foreign rule named `_provider_other` that matches a Codex host still is. A caller checking an authored file passes an empty slice. `overlaps` treats `*.openai.com`, `**`, and exact hosts. `NetworkRule::docker_engine` builds the D24 rule: host `host.openshell.internal`, the engine port, `tcp`, `tls: skip`, binary `/usr/local/bin/docker`.

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
- `Transfer`: upload then download with the digest check. It needs the `openshell` CLI and a real sandbox filesystem.
- `Terminal`: a main-process terminal with detach and re-attach replay, the input lease rules, `Viewer` input ignored, and a shell terminal exit code. It needs the SSH relay.

`jackin-testkit` runs all three groups against `FakeGateway` in nextest (`crates/jackin-testkit/tests/contract.rs`). `crates/jackin-openshell/tests/contract.rs` runs only `Core` against `GrpcGateway` and an in-process tonic server, because that server implements the gRPC subset only. `xtask e2e` runs all three groups against the live gateway before J00 and fails the run on a failed check. Name length is not a suite check: `SandboxName` makes a 20-character name unrepresentable, so `SandboxName::parse` unit tests in `jackin-core` cover the rule, and `crates/jackin-openshell/tests/raw_names.rs` sends a raw 20-character name through the tonic client to the in-process server and requires `GatewayError::InvalidArgument { field: "name" }`. The real v0.1.6 rejection is recorded in the P4.1 evidence.

### Error enums

```rust
pub enum GatewayError {
    Unreachable { endpoint: String, message: String },
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
    Timeout { op: GatewayOp, after: Duration },
    Cancelled { op: GatewayOp },
    Process(ProcessError),
    Transfer { op: TransferOp, status: ProcessStatus, tail: TerminalText },
    HostKeyMismatch,
    TerminalRefused { reason: String },
    Protocol { message: String },
}
pub enum Missing { Sandbox(SandboxName), Provider(ProviderName), Profile(ProfileId), Workspace(OpenShellWorkspace) }
pub enum GatewayOp {
    Health, Profile, Provider, Create, Find, Detail, Power, Delete,
    Program, ReadFiles, WriteFile, Upload, Download, Terminal,
}
pub enum TransferOp { UploadFiltered, UploadGit, Download, Digest }
pub struct SessionClosed;
pub enum DiscoveryError {
    ListFailed { status: ProcessStatus, tail: TerminalText },
    ListUnparsable { message: String },
    UnknownGateway(GatewayName),
    MissingCertificate { path: HostPath },
    Process(ProcessError),
    Fs(FsError),
}
pub enum PolicyError {
    Syntax { message: String },
    NotAMapping { key: String },
    ReservedRuleName(String),
    InvalidHost(String),
    InvalidPort { rule: String, port: i64 },
    DuplicateRule(RuleKey),
    Profile { id: String, message: String },
}

impl GatewayError {
    pub fn is_retryable(&self) -> bool;
}
```

Status mapping (one function and one table in `grpc/sandbox.rs`): `NotFound` to `NotFound`; `AlreadyExists` to `AlreadyExists`; `FailedPrecondition` on program or terminal use of a sandbox that does not accept exec to `NotReady`; `FailedPrecondition` on stop of an `Error` sandbox to `MainProcessFailed`; other `FailedPrecondition` to `FailedPrecondition`; `Unauthenticated`; `PermissionDenied`; `ResourceExhausted` (stream or token limit); `InvalidArgument`; `DeadlineExceeded` to `Timeout`; `Unavailable` and connect errors to `Unreachable`; anything else to `Protocol`. `is_retryable` is true for `Unreachable`, `Timeout`, and `ResourceExhausted`; the session decides whether to retry (bounded, with the clock).

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
| Upload and download argv | the two upload calls run with the project root as `cwd`, name the gateway, workspace, and sandbox, use `--no-git-ignore` for `.git`, have stdin null and a timeout; download compares digests | a non-zero exit gives `Transfer { UploadGit }` with a sanitized tail; a digest mismatch gives `Transfer { Digest }` |
| Terminal main process (J11) | attach delivers replay then live output; input reaches the process for `Owner`; `Resize` reaches the server; detach leaves the process running; re-attach replays; a second attach gets `Lease(ReadOnly)`, and after the owner detaches its next input is forwarded with `Lease(Owner)` | `Viewer` input is not forwarded; a Ctrl-C in a read-only attachment ends only that attachment; a wrong host key fingerprint gives `HostKeyMismatch` and no data; an attach after exit gives `Ended(Refused)` |
| Repaint nudge | with `NudgeAfter(600ms)` the server sees one resize to `rows-1` and one to `rows`, in that order, after the delay in paused time, with the size the pane last set | `Never` sends none; a pane resize before the timer changes what the nudge restores |
| Terminal shell (J12) | the start message carries `workdir` and `env`; the exit code arrives as `Exited` | dropping the session closes the stream and the server observes the process kill |
| Limits | a fourth stream on one token surfaces `ResourceExhausted` | none |
| Discovery (fix 6) | `registered_gateways` parses a `gateway list -o json` sample without any connection; `resolve_gateway` reads three certificate files | unknown name; missing `tls.key`; invalid JSON each give the right `DiscoveryError`; the registry is never written (no `write_new` reaches the fs fake) |
| Policy and closure (D10) | an authored role policy parses and `with_rule(docker_engine(p))` adds exactly one rule and keeps every other node; a live policy that holds `_provider_jackin-work-codex` passes when that key is in `generated` | `parse_authored` rejects `_provider_x`; a rule with host `*.openai.com`, a rule with `api.openai.com` on port 8080, and a rule with host `**` each give a `ClosureViolation`; a live rule named `_provider_other` that matches a Codex host is still reported |
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
  roles.rs          RoleCatalog, LoadedRole, RoleError
  credentials.rs    read_chatgpt_login, ChatgptLogin, CredentialError
  accounts.rs       AccountSync, AccountError
  plan.rs           plan_launch, plan_image, LaunchRequest, Prompting, LaunchPlan, PlannedFiles, SandboxDraft, PlanError, EnvQuestion
  files.rs          pub(crate): render_auth_json, render_codex_config
  launch.rs         LaunchStep, LaunchEvent, LaunchEvents, LineDelivery, LaunchFeed, LaunchOutcome, LaunchFailure, LaunchError
  launch/{decide,probe,hooks}.rs   pub(crate): Observed, Done, Decision, decide; SandboxProbe; run_hook
  attach.rs         AttachRequest, AttachTarget, AttachedTerminal, ShellRequest, errors; pub(crate) split_replies
  export.rs         ExportReport, WorkState, ExportError
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

    pub async fn list(&self) -> Result<Vec<SandboxSummary>, LifecycleError>;
    pub async fn inspect(&self, sandbox: &SandboxName) -> Result<InspectView, LifecycleError>;
    pub async fn power(&self, sandbox: &SandboxName, action: PowerAction) -> Result<SandboxSummary, LifecycleError>;

    pub async fn work_state(&self, config: &Config, sandbox: &SandboxName) -> WorkState;
    pub async fn export(&self, config: &Config, sandbox: &SandboxName, cancel: CancellationToken) -> Result<ExportReport, ExportError>;
    pub async fn purge(&self, config: &Config, sandbox: &SandboxName, mode: PurgeMode) -> Result<PurgeOutcome, PurgeError>;

    pub async fn sync_account(&self, account: &Account) -> Result<AccountSync, AccountError>;
    pub async fn forget_account(&self, account: &Account) -> Result<(), AccountError>;
}

pub struct SystemClock;
pub struct TokioProcessRunner;

impl HostPorts {
    pub fn system(fs: Arc<dyn FileSystem>) -> HostPorts;
}
```

A `Session` is bound to one gateway. The console holds one per configured gateway that connected. The operations take `&Config` only where they need it (the workspace path for export); the session owns no config copy.

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

`RoleCatalog::new` takes the filesystem and the runner only, so `jackin role check <path>` (G-ROLE) builds one and calls `load_path` with no gateway, no config, and no engine. `load` picks the role from the argument, else the workspace `default_role`, else the only configured role, else `RoleError::NoRole`. A `RoleSource::Git` is cloned or fetched into `cache_dir/roles/<name>` through the runner (`Program::Git`). Both do all role-intrinsic validation once: parse the manifest; require that the Dockerfile, policy, and hook paths are regular files inside the repository (no symlink escape); `validate_dockerfile`; `PolicyDocument::parse_authored`; and the D10 provider host closure check against every profile of every `Agent` (`ProviderProfile::parse` of each `AgentSpec::profiles`, empty `generated` slice), reported as `RoleError::ProviderHostClosure`. `RoleRevision` is `Clean { commit }` when `git status --porcelain` is empty, else `Dirty { commit, digest }`, where `digest` hashes the sorted names and contents from `git ls-files -co --exclude-standard` (decision 4).

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

`read_chatgpt_login` parses `<home>/auth.json` once into typed fields; `jackin account add codex --chatgpt-from <home>` calls it to learn the account ID for `CredentialSource::CodexLogin`. The secrets appear only inside `ChatgptLogin` and `ProviderMaterial`. `sync_account` runs on `account add` and on launch when the OpenShell provider is absent on the selected gateway. For a Codex account with a `CodexLogin` source it reads the login, calls `ensure_profile`, and calls `put_provider`. For a Codex account with any other source (credential mode A) it returns `AccountError::Unsupported { account, mode }`, because `profile_for(ApiKey)` is `None` until a profile is proven (P10.1 or later). For an account of any other provider it returns `HostOnly` and touches no gateway (D26). When mode A gets a profile, this file gains the `Env`, `OnePassword`, and `File` readers (D28 1Password flow); the usage crates read their own credentials. `forget_account` deletes the OpenShell provider (`account rm`).

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
pub struct LaunchFailure { pub launch_id: LaunchId, pub step: LaunchStep, pub sandbox: Option<SandboxName>, pub error: LaunchError }
pub enum LaunchError {
    Image(ImageError),
    Gateway(GatewayError),
    Credential(CredentialError),
    Fs(FsError),
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
}
```

`LaunchEvents::emit` never drops a step event. With `LineDelivery::Reliable` it waits for room for `Image(Line)` events too, so the CLI never drops a build line and the build runs at the speed of the renderer. With `BestEffort` it drops `Image(Line)` events when the consumer lags, because the cockpit keeps its own tail. `Cancelled` is an outcome, not an error: it carries the sandbox so the CLI and console report a restore candidate (D22, ADR-008). A failure keeps the sandbox and names it.

#### How the executor works (concurrency and idempotence)

`launch` is one async function with one owner of the run state. It loops: `observe` (one `find_sandboxes` by launch label, plus one `read_files` probe when the sandbox is `Ready`), then the pure `decide(plan, observed, done, limits)`, then one act. Each act is idempotent against the observation that selected it. The loop checks `cancel` before every act, and long acts (`ensure_image`, `run_program`, `upload_project`) receive the token.

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
| provider not done | `Provider` | `provider_state`; if absent, `sync_account` (read login, `ensure_profile`, `put_provider`). A differing profile is updated by `ensure_profile`. Equal material changes nothing | `ProviderPresent` |
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
| `Ready`, `uploaded` marker absent | `Upload` | `upload_project`, then `write_file` of the marker last | `AlreadyUploaded` |
| uploaded, supervisor unreadable, or `Running` while `boot` is `Powered` | `Supervisor` | poll `read_files(RUN_STATE)` until a valid state (`limits.supervisor`). After this run started the sandbox, a `running` reading is the file of the earlier boot, so it is not accepted until `waiting` has been read (`boot = Observed`) | none |
| supervisor `Running` | finish | none | `AlreadyRunning` |
| supervisor `Waiting` or `Exited`, hooks not done | `Hooks` | `run_program(setup, touch_on_success = setup_done marker)` unless `setup_done`; `run_program(preflight)`; a non-zero exit fails with `HookFailed` and the sandbox stays `Ready` | `SetupAlreadyDone` |
| hooks done, no attempt started | `Start` | `write_file` of `StartRequest(state.next_attempt())` for the observed state; `None` fails with `AttemptsExhausted`; poll until the state names that attempt as `Running` or `Exited` (`limits.attempt`) | none |
| attempt started and state names it | finish | none | none |

The delete in the recreate row is safe because this run created the sandbox and never saw it accept exec, so it holds no upload. The upload marker itself cannot be read: an `Error` sandbox refuses exec. A retry run that finds an `Error` sandbox it did not create takes the `MainProcessEnded` row and deletes nothing.

A retry with the same launch ID re-enters the loop at the observed row. A crash between the upload and its marker uploads again (safe overwrite). A retry after `HookFailed` runs `preflight` again, and runs setup again only when its marker is absent. A retry when Codex exited starts a new attempt after `preflight` (explicit request, ADR-008). The supervisor state is read only to choose between these rows. It never decides export or purge (D09).

Fencing keys: the launch ID label (which sandbox is this launch), the sandbox name (at most one creator wins), the marker file names that embed the launch ID (upload and setup scope), and the attempt ID (which start request the supervisor honors, D09).

### Supervisor protocol (D09), jackin side

`/sandbox/.jackin/run/state` is written only by `jackin-start`: absent (not up yet, or clearing old state), `waiting` (ready for an attempt; hooks may run), `running <attempt>` (Codex is a child; attach), `exited <attempt> <code>` (the PTY is alive and shows one line; restart on request). `/sandbox/.jackin/run/start` holds an `AttemptId`, written only by jackin (`write_file`, atomic) and only while the state is `waiting` or `exited`, because a request written while Codex runs takes effect at its exit (P7.4 F5). Jackin picks the ID with `SupervisorState::next_attempt`; `jackin-start` honors an ID only when it is greater than the last one it handled, so a stale or repeated ID does nothing. `jackin-start` also sources `/sandbox/.jackin/env-$JACKIN_LAUNCH_ID` before each attempt (D11; open question 1). `SupervisorState::parse` and `StartRequest::render` in `jackin-core` are the only Rust code that reads or writes these formats; `jackin-start` implements the same grammar in sh (P7.4).

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
    HookFailed { hook: HookKind, exit_code: i32, output: TerminalText },
    AttemptTimeout { sandbox: SandboxName, attempt: AttemptId },
}
```

`attach` finds the sandbox, requires `Ready` (a `Completed` or `Error` sandbox has no live main PTY, so `MainProcessEnded` names the phase and exit code), opens `TerminalKind::MainProcess { input, repaint: NudgeAfter(600 ms) }`, and reads the supervisor state once so the CLI can print "Codex exited with code N; use `jackin attach --restart`" (D09). `open_shell` takes the launch ID from the sandbox labels, reads and parses the env file of that launch, and opens `TerminalKind::Shell { workdir: PROJECT, env }` with its values, so a shell sees the role `[env]` values and the `DOCKER_*` variables (D24). A sandbox whose launch never reached the Credentials step has no env file and gives `AttachError::EnvFile`. The console opens at most one `MainProcess` terminal per sandbox (D28) by holding it in its pane model, not by a lock in the session. `restart_agent` runs the hook rows `Hooks` (preflight only, with the env file) and `Start` with `next_attempt()`, and refuses with `AlreadyRunning` when the state is `running` (no silent reconnect or restart). Ctrl-D and Ctrl-P handling (D09, D28) is a terminal front-end concern in `jackin` and `jackin-tui`.

`AttachedTerminal` wraps the `TerminalSession` and is the only handle that `attach` returns, so the CLI pump and every pane send input through `AttachedTerminal::input`. `input` splits each chunk with the pure function `split_replies` into plain bytes and terminal replies: cursor position reports (`ESC [ <row> ; <col> R`), device attribute replies (`ESC [ ? ... c` and `ESC [ > ... c`), and OSC 10 and 11 color replies (`ESC ] 10 ;` and `ESC ] 11 ;`, ended by BEL or `ESC \`). Plain bytes always go through. Only when the chunk holds a reply does `input` read `RUN_STATE` once through the gateway: the replies go through only for `Reported(Running)`, and they are dropped for `Waiting`, `Exited`, `Unreadable`, and `Invalid`. Reading at the moment of the reply sees every change, including a restart after `Exited`. A reply split across two chunks is not recognized (terminals write a reply in one chunk; inferred). Replayed queries make the terminal answer them again on every attach, and those answers would sit in the PTY queue while `jackin-start` waits (D09, P7.4 F3).

### Export and purge (D08, ADR-010)

```rust
pub struct ExportedBranch { pub branch: BranchName, pub tip: CommitId, pub host_ref: String }
pub struct ExportReport { pub sandbox: SandboxName, pub bundle: Sha256, pub branches: Vec<ExportedBranch> }
pub enum UnknownReason { NotRunning(Phase), WorkspaceUnknown(WorkspaceName), NoRepository, InspectionFailed }
pub struct Unexported { pub uncommitted: bool, pub branches: Vec<BranchName> }
pub enum WorkState { Exported, Unexported(Unexported), Unknown(UnknownReason) }
pub enum PurgeMode { Guarded, Forced }
pub enum PurgeOutcome { Deleted { sandbox: SandboxName, was: WorkState }, AlreadyGone { sandbox: SandboxName } }
pub enum ExportError {
    Gateway(GatewayError),
    NotFound(SandboxName),
    NotRunning { sandbox: SandboxName, phase: Phase },
    WorkspaceUnknown(WorkspaceName),
    Project(PathError),
    SandboxGit { exit_code: i32, output: TerminalText },
    HostGit { status: ProcessStatus, output: TerminalText },
    RefConflict { host_ref: String, output: TerminalText },
    TipMismatch { branch: BranchName, expected: CommitId, found: Option<CommitId> },
    Cancelled,
}
pub enum PurgeError { WorkNotExported { sandbox: SandboxName, state: WorkState }, Gateway(GatewayError) }
pub enum LifecycleError {
    Gateway(GatewayError),
    Labels { sandbox: SandboxName, source: LabelError },
    NotFound(SandboxName),
    PowerTimeout { sandbox: SandboxName, wanted: Phase, after: Duration },
    MainProcessEnded { sandbox: SandboxName, phase: Phase },
}
```

`export` steps, in order, each failing with a typed error and writing no host state before the last two:

1. The sandbox must be `Ready`. Resolve the host repository from the workspace label and `config` (`ProjectRoot::open`).
2. In the sandbox, list branch tips (`run_program(git for-each-ref refs/heads)`) and create one bundle of those refs at `bundle_path(sandbox)` (`run_program(git bundle create)`).
3. `download_file` to a temporary file in `cache_dir` (the gateway compares digests).
4. On the host, verify the bundle, then fetch each branch with a forced refspec into `refs/heads/jackin/<sandbox>/<branch>` only, through `Program::Git`. The working tree and the index are never written. A ref name collision (`jackin/<sandbox>` against `jackin/<sandbox>/x`) is `RefConflict`.
5. Read each host ref back and require it to equal the sandbox tip (`TipMismatch`). Remove the temporary bundle.

`export` is idempotent: repeating it moves the same refs to the same tips.

`work_state` follows D08 exactly. `Exported` when the sandbox is `Ready`, `git status --porcelain` is empty, and every branch tip equals its `refs/heads/jackin/<sandbox>/<branch>` on the host. `Unexported` lists uncommitted work and each branch whose host ref is missing or different. Anything jackin cannot inspect (not `Ready`, git failed, workspace gone) is `Unknown(..)` and counts as not exported. `purge(Guarded)` deletes only for `Exported` and otherwise returns `PurgeError::WorkNotExported { state }` with nothing deleted. `purge(Forced)` deletes after computing the state for the report. A missing sandbox returns `AlreadyGone`, so repeating a purge is safe. The supervisor state file is never read here.

`power` refuses before it calls the gateway when the sandbox phase is `Completed` or `Error`: it returns `LifecycleError::MainProcessEnded { sandbox, phase }`, and `phase` carries the exit code. OpenShell 0.1.6 cannot stop or restart such a sandbox (D09), so the operator exports and purges it. `inspect` builds the D10 closure report from the live policy: for each attached OpenShell provider it passes `RuleKey::provider_rule` in `generated`, so the generated `_provider_<name>` rule is not a violation.

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

`doctor` never fails and never repairs (D13). It runs `openshell --version`, `ssh -V`, and `git --version` through the runner, calls `registered_gateways`, calls `Gateway::health` when the access is `Connected`, compares the versions with `Compatibility`, and `stat`s the engine socket. With `NotChosen` or `Failed`, the gateway checks report `Skip` or `Fail` and the other checks still run. One `CheckId` per D13 item, matched exhaustively by the CLI and the console doctor panel.

### Remaining error enums

```rust
pub enum RoleError {
    NoRole,
    UnknownRole(RoleName),
    Manifest { path: HostPath, source: ManifestError },
    FileMissing { path: RepoPath },
    FileNotRegular { path: RepoPath },
    EscapesRepository { path: RepoPath },
    Dockerfile(DockerfileError),
    Policy(PolicyError),
    ProviderHostClosure(Vec<ClosureViolation>),
    Git { status: ProcessStatus, output: TerminalText },
    Fs(FsError),
    Process(ProcessError),
}
pub enum CredentialError { LoginUnreadable { path: HostPath }, AuthJsonInvalid { key: String }, NotAChatgptLogin, Fs(FsError) }
pub enum AccountError {
    Credential(CredentialError),
    Gateway(GatewayError),
    Unsupported { account: AccountName, mode: CodexAuthMode },
}
```

All error enums derive `Debug` and `thiserror::Error`, and none holds a secret.

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
| Role validation without a gateway (G-ROLE, fix 6) | `RoleCatalog::new(fs, runner, cache_dir).load_path` loads a valid role with no `Session`; a dirty role tree yields `RoleRevision::Dirty` and a changed final tag | an unknown manifest field, a final stage without `FROM construct`, and a role policy rule for a Codex host (decision 14: `parse_authored` and closure) each give the right `RoleError` |
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
- `UsageReport` derives `Serialize`, `Deserialize`, and `JsonSchema`. `jackin usage --json` prints it unchanged. `schema` is `1`. `xtask e2e` parses it for J17.
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
}

pub struct SandboxHandle { }

impl SandboxHandle {
    pub fn name(&self) -> SandboxName;
    pub fn phase(&self) -> Phase;
    pub fn labels(&self) -> Labels;
    pub fn environment(&self) -> BTreeMap<EnvName, String>;
    pub fn file(&self, path: &SandboxPath) -> Option<Vec<u8>>;
    pub fn file_mode(&self, path: &SandboxPath) -> Option<FileMode>;
    pub fn supervisor(&self) -> Option<SupervisorState>;
    pub fn attachments(&self) -> usize;
    pub fn main_input(&self) -> Vec<u8>;
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

`World.usage` and `UsageServer` arrive with U13. `TestRunner` is the `ProcessRunner` of a `World`. It runs `Program::Git` for real and answers `Docker`, `Openshell`, `Ssh`, and `Op` from a `ScriptedProcessRunner`; an unscripted spec is an error that names it. `ScriptedProcessRunner` alone cannot be given a `Git` expectation: `ScriptedProgram` has no such variant, so faking git does not compile. `World` uses `StdFileSystem` over a temporary directory, because upload and export run real `git` on real files; tests that need fault injection or no disk use `MemFs`.

Semantics the fakes must match (each is also a contract-suite case or a journey step).

- `FakeGateway` checks what the types cannot: names are unique, and a create returns before `Ready` (`ProvisionFor` sets how many polls stay `Provisioning`). With `ProvisionFails { creates: n }` the first `n` creates end in `Error` with no exit code and `Provisioning::Failed` (P7.4 F8). Profiles carry a resource version, and `ensure_profile` updates only with the version it read, as the real RPC does. The spec environment of a sandbox is `JACKIN_LAUNCH_ID` only; the role environment is a file like any other.
- Phases (fix 4): `exit_main(0)` sets `Completed { exit_code: 0 }` and `exit_main(n)` sets `Error { exit_code: Some(n) }`. After that, `run_program`, `read_files`, `write_file`, and terminals return `NotReady { phase }`, `set_power(Stop)` fails with `MainProcessFailed`, and `set_power(Start)` returns a `Starting` view that never becomes `Ready`. This is the observed 0.1.6 behavior for `Error` (D09, P7.1 F3); `Completed` uses the same model until P4.1 observes it (open question 2). Stop on a `Ready` sandbox and start afterwards keep files under `/sandbox` and reset the supervisor model to `waiting`.
- `run_program` runs a closed set of absolute programs against the sandbox state: `/usr/bin/git`, `/usr/bin/printenv`, and `/usr/bin/sha256sum`, plus programs registered with `script_program` (the hook paths). Any other program returns exit code 127. Each run is recorded with the environment it received (`SandboxHandle::runs`), which is how a test sees that a hook got the env file values. There is no shell and no argument parsing beyond one rule: an argument that starts with `/sandbox/` maps to the host directory that backs `/sandbox`. When the program exits 0 and `touch_on_success` is set, the fake creates that file in the same step. `git` is the real host `git` in a temporary directory with the working directory mapped, so bundles and refs are real.
- `upload_project` copies the files that `git ls-files -co --exclude-standard` lists, then `.git`, from the host project into the sandbox directory. `download_file` copies the file and compares digests; `CorruptDownload` flips a byte so the digest check fails with `Transfer { Digest }`.
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
  main.rs           fn main: system deps, tokio runtime, std::process::ExitCode
  lib.rs            run, AppEnv, Deps, GatewayConnector
  cli.rs            Cli, Command, argument structs (clap derive, help in attributes)
  app.rs            pub(crate) App: config store, role catalog, host ports, session_for
  commands/{doctor,build,load,attach,shell,ls,inspect,export,power,purge,account,usage,role,workspace,gateway,console}.rs
  term.rs           CLI terminal hand-off: raw mode, resize, pump
  output.rs         human and JSON writers (no print_stdout)
  error.rs          CommandError, ExitClass
```

### Public surface

```rust
pub struct Deps {
    pub host: HostPorts,
    pub engine: Arc<dyn ImageEngine>,
    pub connect: Arc<dyn GatewayConnector>,
    pub usage_endpoints: ProviderEndpoints,
}

#[async_trait]
pub trait GatewayConnector: Send + Sync + Debug {
    async fn connect(&self, name: &GatewayName, entry: &GatewayEntry) -> Result<Arc<dyn Gateway>, GatewayError>;
}

pub struct AppEnv {
    pub args: Vec<OsString>,
    pub stdin_is_tty: bool,
    pub stdout: Box<dyn Write + Send>,
    pub stderr: Box<dyn Write + Send>,
    pub config_path: HostPath,
    pub env: EnvSnapshot,
    pub cancel: CancellationToken,
}

pub async fn run(env: AppEnv, deps: Deps) -> ExitClass;

impl std::fmt::Debug for AppEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result;
}
```

`AppEnv` writes `Debug` by hand because its writers are `dyn Write`; it prints the other fields. `Deps.usage_endpoints` and the `usage` command arrive with U13. `GatewayConnector` is the construction seam of the gateway boundary: production returns `GrpcGateway::connect(resolve_gateway(..))` and tests return `FakeGateway`. The same holds for `Deps.engine` (`BuildxEngine` or `FakeEngine`). Commands that need no gateway (`doctor`, `role`, `workspace`, `gateway`, `account` for non-Codex providers, `usage`, `build`) never call `connect`; `doctor` passes the connection error to `GatewayAccess::Failed`.

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
    Plan(PlanError),
    Role(RoleError),
    Launch(LaunchFailure),
    Attach(AttachError),
    Restart(RestartError),
    Lifecycle(LifecycleError),
    Export(ExportError),
    Purge(PurgeError),
    Account(AccountError),
    Image(ImageError),
    Usage(UsageError),
    Gateway(GatewayError),
    Discovery(DiscoveryError),
    Terminal(std::io::Error),
    Console(ConsoleError),
    NoTerminal,
}

impl CommandError {
    pub fn class(&self) -> ExitClass;
}
```

Every field of the argument structs has `#[arg(help = ...)]` text and no doc comment (D17); the block shows the attributes on `LoadArgs` only. A field of type `bool` is a `--long` flag and `Option` is optional; `name`, `sandbox`, `path`, and `account` are positional, except `--name` of `account add` and `gateway add`'s `name`, which follow the DESIGN table. `BindArgs` has a required, exclusive clap group over `--role` and `--workspace`, and `scope` turns the pair into the enum (`CommandError::Argument` when clap let neither or both through, which the group makes unreachable). `RoleSourceArg` parses an existing path as `Path` and anything else as a Git URL. `CredentialArg` parses `env:NAME`, an `op://` reference, or `file:PATH`.

`class` is an exhaustive match: `Refused` for `PurgeError::WorkNotExported`, `PlanError::EnvNeeded` without a TTY, and `RestartError::AlreadyRunning`; `Usage` for argument errors; `Cancelled` for `LaunchOutcome::Cancelled`; the rest `Failure`. Errors print to stderr with the cause chain, the step name, and the sandbox that remains.

### Output and terminal hand-off

`output.rs` writes through the `AppEnv` writers (`print_stdout` is denied). `ls --json` writes `LsOutput`, `inspect --json` writes `InspectView`, `build --json` writes `BuildOutput`, and `usage --json` writes `UsageReport`. `build` without `--json` prints the final image ID on one line. Human output derives from the same values. `term.rs` implements the CLI hand-off for `attach` and `shell`: enter raw mode, pump stdin bytes to `TerminalSession::input`, host SIGWINCH to `resize`, `Output` bytes to stdout, restore the terminal on every exit path, and print the exit or detach line. It forwards Ctrl-D and Ctrl-P Ctrl-Q unchanged because the supervisor detaches on them (D09), and it sends every input chunk through `AttachedTerminal::input`, so the reply filter applies. It never calls `openshell` (D03).

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
| `role check` (J02, fix 6) | a valid role exits `Ok` with no gateway configured | an unknown manifest field exits `Failure` and stderr names the field; a policy rule for a Codex host is rejected naming the rule |
| `doctor` (J01, fix 6) | with the fake gateway healthy, all checks pass | with the connector failing, exit is `Failure` and the binary checks still print |
| accounts (J17, decision 5) | `account add`, `default`, `bind` produce the stored config; `account add codex --chatgpt-from` creates the OpenShell provider; `account rm` of a bound account refuses | an unknown provider exits `Usage`; a Codex account with `--credential env:X` exits `Failure` with `Unsupported` |
| `usage --json` | stdout parses into `UsageReport` | an unavailable account is present with its reason |
| `attach --restart` | restarts after `exited`; prints the exit code line when not restarting | on `running` exits `Refused` and writes no start request |
| No terminal | `jackin` with no TTY prints help and exits `Ok` | `attach` with no TTY exits `Failure` with `NoTerminal` |

---

## 9. jackin-xtask

Role: repository gates and the live journey runner. Package `jackin-xtask`, alias `cargo xtask`. A binary plus a lib target, with pure check modules, so each gate step is tested on fixture repositories and also run on the repository. With the default features it depends on no product crate, so a compile error in a product crate cannot stop `xtask ci` from building and reporting the other gates. The cargo feature `e2e` adds `jackin-core` (typed views), `jackin-openshell` (a `Gateway` to run the sentinel probe and read files, decision 12), the `jackin-usage` facade (`UsageReport` for J17, from U13), `portable-pty`, and `schemars`, and no other product crate. The `clippy` and `nextest` steps pass `--all-features`, so they still build and test the `e2e` items.

### Module map

```text
crates/xtask/src/
  main.rs         clap entry
  lib.rs          re-exports
  ci.rs           CiArgs, CiProfile, CiStep, run_ci
  repo.rs         Repo (file listing, cargo metadata)
  violation.rs    Violation
  checks/comments.rs     lexer-based comment scan (D17)
  checks/layers.rs       LAYERS table, import rules, Secret::expose allowlist
  checks/containers.rs   forbidden container invocations (D01)
  checks/pins.rs         Rust version pins agree (U1 fix round, P3)
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
pub enum CiStep { Fmt, Clippy, Nextest, Deny, Shear, Comments, Layers, Pins, Containers, Reuse, Docs }

impl CiStep {
    pub const ALL: [CiStep; 11];
    pub fn runs_in(self, profile: CiProfile) -> bool;
}

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
pub enum CiError { ToolFailed { step: CiStep, status: i32 }, Metadata(String), Io(std::io::Error) }
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

- `CiStep::ALL` is the one ordered list of the P2.3 steps. `run_ci` runs the given steps in that order and stops at the first failing step. It runs each tool as a subprocess itself and has no seam to fake, because the tools are the repository's own and not an external boundary. `--fast` and `--no-docs` both give `CiProfile::SkipDocs`: every step but `Docs`. `--only` selects steps by name and conflicts with both flags (U1 fix round, P1). `Full` runs all eleven. The run tests drive the shipped binary with scripted tools, so the order, the profiles, `--only`, the stop at the first failure, and the exit codes (0, 1, and 2 for a usage error) are tested through the real entry point.
- `comments` uses a Rust lexer for `.rs` and line-prefix rules for `.toml`, `.sh`, and `Dockerfile`. It allows SPDX header lines (`SPDX-FileCopyrightText:` or `SPDX-License-Identifier:` with a value), shebang lines, `# syntax=` parser directives, and files whose first line carries the Velnor generated header. A `//` inside a string literal is not a comment.
- `layers` reads `cargo metadata` and checks `LAYERS` against every workspace member. It fails for a member with no row, for a dependency or dev-dependency the row does not list, for a dependency the row lists as `optional` that is not optional in the manifest, for a banned import, and for a `Secret::expose` call outside the allowlist. It does not look for container programs: `containers` does. `LAYERS` is the table in the Shape section; the `crates/usage/` crates are rows with a `jackin-usage-*` pattern, added in U13. Adding a crate without a row fails the gate, so a new crate cannot skip the rule. One table drives the D14 order, the usage rules, and the import rules.
- `containers` is the only step that looks for container programs (D01). The subcommand rule is the `DockerSubcommand` type in `jackin-core`, so the step guards the crate boundary: it flags `Program::Docker`, `DockerSubcommand`, and the aliases, globs, and groups that reach them outside `jackin-image`, `jackin-core`, and `jackin-testkit`; any `Command::new` with a program that is not one string literal outside `crates/xtask/`; and any literal `docker`, `podman`, or `container`. U1 fix round P4 has the exact rules.
- `Journey::start` connects a `GrpcGateway` through the private `e2e/runner.rs` runner (about 50 lines over `tokio::process`: stdin from `/dev/null`, a timeout, kill on cancel), because `xtask` cannot use `jackin-session::host`. It then runs `contract::run` against the live gateway and fails with `E2eError::Contract` on a failed check. The journey runs the `jackin` binary as a subprocess, runs `/usr/local/bin/jackin-sentinel-probe` in the sandbox through `Gateway::run_program`, parses `ls --json`, `inspect --json`, and `usage --json` into the `jackin-core` and `jackin-usage` types, and compares the host observation with the probe report per step.
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

- P1. CLI contract. `xtask ci [--fast|--no-docs] [--only STEP[,STEP...]]`. `--only` is repeatable and comma-delimited. `CiStep` implements clap `ValueEnum` with the names `fmt clippy nextest deny shear comments layers pins containers reuse docs`. `CiStep::ALL` is Fmt, Clippy, Nextest, Deny, Shear, Comments, Layers, Pins, Containers, Reuse, Docs. The selected steps run once each in `CiStep::ALL` order, whatever the order or duplication on the command line. An unknown or empty step name is a usage error, and so is `--only` with `--fast` or `--no-docs` (the intersection could select zero steps). A usage error exits with status 2 before any step runs. A violation or a tool failure exits 1. Success exits 0.
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
- `BuildStage` implements `Display` (`construct`, `role`, `derived`). `EngineError` messages use Display for `BuilderName`, `ImageTag`, `BuildStage`, and `TerminalText::as_str()` (the tail starts on a new line). `ProcessStatus` has no `Display` in `jackin-core` (not in this writer's scope), so `engine.rs` renders it as `exit code N` or `signal N` through a private function.
- T-B6 and T-B7 use `std::env::temp_dir()` plus a `Drop` guard instead of the `tempfile` crate, so no dev-dependency is added. They spawn `Command::new("sh")` with `spawn` plus `wait_with_output` and `LC_ALL=C`.
- T-B3 and T-B4 read the Dockerfiles as text. The behavior itself (hostile role with `sudo`, a setuid binary, a file capability) is a P4.3 or P7.1 live hand-off.

- The image drafts (construct.Dockerfile, derived.Dockerfile, jackin-start, codex-provider.yaml) were deleted from rewrite/drafts in the P3 fix round. The single sources are crates/jackin-image/images/ and crates/jackin-core/src/assets/codex-provider.yaml. Older task scopes in PLAN.md that name rewrite/drafts files are historical.

||||||| parent of 2c265d73 (fix(core)!: close the phase P3 review findings in jackin-core)

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
- U7-B5. Contracts of the runner. (a) A line is the bytes up to `\n`; the `\n` is stripped; `\r` is kept; a trailing partial line at EOF is delivered; invalid UTF-8 is decoded lossily. (b) The runner awaits `send` and never drops a line; a closed sink is not an error, and the runner keeps draining and discards. (c) With a sink, `ProcessOutput.stdout` and `stderr` are each at most 65536 raw bytes, the true suffix of that stream (the runner keeps exactly the last 65536 bytes); without a sink all output is returned, uncapped (orchestrator decision 4). (d) A nonzero exit or a signal is `Ok(status)`. (e) `Null` stdin is `/dev/null`; `Bytes` is written while output is read, then stdin is closed; a broken pipe is ignored. (f) `Timeout { program, after: spec.timeout }`; the clock starts at spawn and covers the whole call. (g) `Cancelled { program }`, including for a token that is already cancelled (the child is not started then). (h) Timeout and cancel kill the process group of the child (`process_group(0)` and `rustix::process::kill_process_group`), then reap the direct child before returning; the runner returns even when a descendant holds the pipes. (i) `run_secret` shares one spawn, timeout, cancel and kill core with `run`, takes no sink, puts stdout only into `SecretBytes` (the buffer grows by allocating the next buffer and zeroizing the old one), and returns stderr as plain bytes. (j) A spawn failure is `Spawn { program, message }` where the message is the OS error only. (k) The runner removes inherited variables that match a non-empty prefix first, then applies `spec.env`; `EnvValue::Secret` is exposed only at the spawn call.
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
