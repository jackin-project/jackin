# Jackin OpenShell rewrite plan

This plan rebuilds jackin from zero around OpenShell, with Codex as the only agent. It changes five repositories on one branch, `feature/openshell-native`. They are the four jackin repositories and `terminal-components-baseline`. Work on jackin and jackin-sentinel comes first, because the sentinel journey proves that jackin works. Work on Agent Smith and The Architect starts after the first proof.

[DESIGN.md](DESIGN.md) is the authority for what to build. This plan tells you how to build it and how to prove each task.

## How to read this

- Each task has an ID, such as `P3.2`. Cite the ID in commits, briefs, evidence files, and trail rows.
- Each task names its repository, its owner subagent, its dependencies, its write scope, its steps, and its proof.
- Mark a task box only when its proof exists. A proof is a gate output, a commit SHA, an evidence file, or a reviewer verdict.
- A task that cannot finish because OpenShell lacks a capability is `BLOCKED`. Write a report for it in [blockers/](blockers/). Do not mark its box.
- Tests alone do not prove a task. A task that changes behavior is proven only when its unit tests pass and its live proof exists.

## Repositories

All five repositories are under `/Users/donbeave/Projects/jackin-project/`. Each one has the branch `feature/openshell-native`. It was created from `origin/main` on 2026-11-02.

| Repository | Path | Base commit |
|---|---|---|
| jackin | `jackin/` | `470fc5d6` |
| jackin-sentinel | `jackin-sentinel/` | `cb2ce2a` |
| jackin-agent-smith | `jackin-agent-smith/` | `f9e06ba` |
| jackin-the-architect | `jackin-the-architect/` | `7b72b38` |
| terminal-components-baseline | `terminal-components-baseline/` | `95bbcbb` |

## Autonomy

The program runs silently from start to end. Never ask a human. Never use `AskUserQuestion`. Never stop to wait for a reply. The operator reads the trail, the evidence, and the pull requests after the program ends.

If you do not know what to do, use this procedure:

1. Start two analysis subagents on different AI models with the same brief. Each subagent finds the facts and proposes a way to continue within DESIGN.md.
2. If they agree, do what they propose. If they disagree, run `pstack:arena` and take the winner.
3. Add a trail row with the decision, the reason, and the evidence. If the decision changes DESIGN.md, edit it in the same commit.
4. Continue.

These items had no obvious owner. Resolve each one with this procedure. Do not ask a question.

- **A1. Codex credential.** Tasks P0.3, P8.2, P8.3, P9.5, P10.1, P11.1, P11.2, and P13.2 need a Codex credential. Find one in this order:
  1. the `OPENAI_API_KEY` environment variable
  2. a 1Password item that holds an OpenAI API key, read with `op item list` and `op read`, only if `op whoami` exits with code 0
  3. the host Codex ChatGPT login in `~/.codex/auth.json`, imported through credential mode B (D10)

  On 2026-11-02 the development Mac has only source 3. Record which source you used. Never record the value. With source 3, the gateway can rotate the refresh token, and the host login can break. Accept this risk and record it in the trail. Run live tasks that share one refresh token one after the other. If no source works, write `blockers/A1.md`. Run no live task. Report the gap at the end. Never report `SKIP` as `PASS`.
- **A2. Image delivery.** Jackin builds images with BuildKit (D05). The gateway driver must see them (D23). Use the first method that works with the P0.2 driver:
  1. `docker-local`. The OpenShell Docker driver reads the OrbStack image store.
  2. `registry`. A local OCI registry runs on OrbStack, and the gateway pulls from it.

  Record the method in the trail. P0.3 proved method 1 with the `vm` driver (D23).
- **A3. Merge.** The program ends with one open pull request for each repository. Do not merge. Do not ask whether to merge.
- **A4. Product change.** If evidence shows that a decision in the product statement of DESIGN.md is wrong, use the procedure above. Then change the decision and continue.

## Orchestrator protocol

You are the orchestrator. You run as Opus 5.5 at effort `high`. You own the plan, the briefs, the integration, and the evidence. You do not write product code.

### Start and restart

1. Read [README.md](README.md), [DESIGN.md](DESIGN.md), this plan, and the last 30 rows of [decisions.tsv](decisions.tsv).
2. In each repository, run `git switch feature/openshell-native` and `git pull --ff-only`.
3. Find the first task whose box is not marked and whose dependencies are satisfied. Start there. A dependency is satisfied in one of two cases. Its box is marked. Or the task is `BLOCKED`, has `blockers/<id>.md`, and has a reviewer verdict that confirms the block.
4. After a context compaction, do steps 1 to 3 again. The marked boxes and the trail are the only durable state.

### Use pstack and the subagent roster

- Work in the `pstack:poteto-mode` style. Name the principle that changed each decision.
- Select subagents by the routing table in `~/.claude/CLAUDE.md`. Each task names its owner subagent.
- Write each brief to the brief contract in `~/.claude/CLAUDE.md`. The contract has six fields, which are Repo, Scope, Task, Gate, Return, and Permissions. Put the task ID and the decision IDs that apply in the brief. Tell the subagent to read DESIGN.md.
- Use these pstack skills at these points:

| Point | Skill |
|---|---|
| Public API of each crate (P2.1) | `pstack:architect` |
| Credential, policy, and image changes (P7.3, P10.2, P13.3) | `pstack:interrogate` |
| Independent live verification (P8.3, P13.2) | `pstack:swarm` |
| Before each commit | `pstack:deslop` |
| Before each review | `pstack:no-comments` |
| Every document | `pstack:technical-writing`, then `pstack:unslop` |
| Trail rows | `pstack:show-me-your-work` |

### Run each code task through this pipeline

1. If the task creates a new public API, `architect` designs it first.
2. `test-reviewer` approves the test plan before anyone writes a test.
3. The implementer writes the code and the tests in its own worktree.
4. `verifier` runs the gate and reports PASS or FAIL.
5. `test-reviewer` proves each new test red, then green, at the committed SHA.
6. At the end of each phase, one `code-reviewer` reviews `<phase start SHA>..HEAD`. Add `security-reviewer` when the phase touches credentials, policy, images, or the gateway connection.
7. A fresh implementer fixes the findings. You verify each fix against its finding. `verifier` runs the gate again.

### Keep one remote branch

- Give each parallel writer its own worktree: `git worktree add ../wt/<repo>-<task> -b wt/<task> feature/openshell-native`.
- Parallel writers never share a file.
- The implementer may commit on its `wt/<task>` branch. It never pushes.
- After the verifier passes, merge the work into `feature/openshell-native` with `git merge --ff-only` or `git cherry-pick`. Then remove the worktree and delete the local `wt/<task>` branch.
- Push only `feature/openshell-native`. Push after each commit.
- Before each commit, verify that the committer is `Alexey Zhokhov <alexey@zhokhov.com>`. Use `git commit -s`. Use Conventional Commits. Put the task ID in the commit body.

### Record the work

- When a task is done, make a follow-up commit in jackin with the subject `docs(rewrite): record <task-id>`. In that commit, mark the task box in this plan and add a row to [decisions.tsv](decisions.tsv). The row names the task, the commit SHA of the work in each repository, the gate result, and the evidence path.
- Put live outputs in `evidence/<task-id>-<slug>.md`. Copy the raw command output. Remove secrets first.
- After each task, paste these lines into the chat: the task ID, the commit SHA, the last line of each gate, and each reviewer verdict. The `/goal` evaluator reads only the chat.

### Handle failure

- When a result fails its gate, start a fresh subagent of the same type with the failure evidence.
- After two failed rounds on one finding, use the next stronger AI model, as `~/.claude/CLAUDE.md` says.
- When OpenShell lacks a capability that a task needs, write `blockers/<task-id>.md`. Give the OpenShell version, the command, the output, the smallest failing case, and the upstream change that the task needs. Continue with independent tasks. Do not add a Docker fallback, a host fallback, or a weaker verification.

## Gates

Run every cargo command from the jackin repository root through mise and mbx.

| Gate | Command | When |
|---|---|---|
| G0 | `mise exec -- mbx +1.99.1 check --workspace --locked` | P2.2 only, before `xtask` exists |
| G-FAST | `mise exec -- mbx +1.99.1 xtask ci --fast` | After each unit, from P2.3 |
| G-FULL | `mise exec -- mbx +1.99.1 xtask ci --no-docs` until P12.1, then `mise exec -- mbx +1.99.1 xtask ci` | At the end of each phase |
| G-E2E | `mise exec -- mbx +1.99.1 xtask e2e --gateway jackin-local --role ../jackin-sentinel --builder orbstack --platform linux/arm64 --engine-port 2376 --engine-certs ~/.local/share/jackin-e2e-engine/client` | From P8, at the end of each later phase |
| G-ROLE | `/Users/donbeave/Projects/jackin-project/jackin/target/debug/jackin role check .` in the role repository, after `mise exec -- mbx +1.99.1 build -p jackin` in jackin | Each role task, from P6.1 |

If D02 changes the Rust version, change `+1.99.1` in this table, in `COMMITS.md`, and in `mise.toml`. Make all three changes in the same commit.

## Phase P0. Prove the platform

The program depends on facts that only a real run can prove. Prove them before you delete code.

### P0.1 Pin the latest stable tools

- **Repo:** jackin. **Owner:** `docs-researcher` finds the versions. `mechanic` at `model: sonnet, effort: high` applies them. **Depends:** none.
- **Scope:** `mise.toml`, `mise.lock`, `rust-toolchain.toml`, `COMMITS.md`, new `compatibility.lock.toml`.
- **Steps:**
  1. Find the latest stable version of Rust, mr-boxington, OpenShell, Codex CLI, cargo-nextest, cargo-deny, and cargo-shear.
  2. Set the pins. Set the mise tool option `mr_boxington = true` on `rust`.
  3. Write `compatibility.lock.toml` with the OpenShell version, tag, commit, and date, and the Codex version.
- **Proof:** `evidence/P0.1-pins.md` contains the output of these commands: `mise exec -- mbx --version`, `mise exec -- rustc --version`, and `openshell --version`.
- [x] P0.1 done — 17975729, evidence/P0.1-pins.md (mbx 1.29.1, rustc 1.99.1, openshell 0.1.6)

### P0.2 Run a local OpenShell gateway

- **Repo:** jackin. **Owner:** `openshell-specialist`. **Depends:** none.
- **Scope:** new `rewrite/evidence/P0.2-gateway.md` and a new draft of a `docs/content/` how-to page in `rewrite/drafts/gateway.md`.
- **Steps:**
  1. Start `openshell-gateway` with the `vm` compute driver. Register it as `jackin-local`.
  2. If the `vm` driver fails, record the error. Then use the Docker driver with `DOCKER_HOST` from OrbStack.
  3. Create a sandbox from the OpenShell default image. Run `id`, `sudo -n true`, and `ls /usr/bin/docker` in it.
  4. Upload a git repository. Verify that `.git` arrives and that `git bundle create` works in the sandbox. Download the bundle. Compare the digests.
  5. Start a long command with a TTY. Detach with Ctrl-P Ctrl-Q. Reconnect and verify the replayed output.
  6. Apply a policy that denies `example.com`. Show that `curl https://example.com` fails.
  7. Create a sandbox whose main process waits for a marker file. Upload a project. Write the marker. Attach. These actions prove the D09 `jackin-start` pattern.
  8. Stop the sandbox and start it again. Show that the uploaded files remain.
  9. Delete the sandboxes.
- **Proof:** `evidence/P0.2-gateway.md` shows the raw commands and outputs. A trail row names the chosen driver.
- [x] P0.2 done — 43bf7590, evidence/P0.2-gateway.md (vm driver, steps 1-9 PASS; D08 .git and D06 user findings feed P0.4)

### P0.3 Run Codex in a sandbox with an OpenShell provider

- **Repo:** jackin. **Owner:** `openshell-specialist`. **Depends:** P0.2, A1.
- **Scope:** `rewrite/evidence/P0.3-codex.md`, `rewrite/drafts/construct.Dockerfile`, `rewrite/drafts/role.Dockerfile`, `rewrite/drafts/derived.Dockerfile`, `rewrite/drafts/codex-provider.yaml`.
- **Steps:**
  1. Write draft construct, role, and derived Dockerfiles (D05, D06). Build them by hand with `docker buildx build`, in the same order that jackin uses. Give the construct image to the role build as the named build context `construct`.
  2. Deliver the final image to the gateway with the A2 method.
  3. Adapt the upstream `providers/codex.yaml` so `binaries` names `/usr/local/bin/codex`. Import it.
  4. Create an OpenShell provider with the A1 credential. For source 3, follow the upstream `examples/codex-app-server` pattern by hand. Put the refresh material into the OpenShell provider. Give the sandbox a placeholder `auth.json`.
  5. Create a sandbox with the image and the OpenShell provider. Run `codex exec "Reply with the word ready"`. Then start interactive Codex with the D04 launch command and verify that the first prompt appears.
  6. Show that `env` in the sandbox has a placeholder and not the key. Show that `curl` from a binary that is not Codex gets no credential.
- **Proof:** `evidence/P0.3-codex.md`. Record the A1 source in the trail.
- [x] P0.3 done — 32b9caa7, evidence/P0.3-codex.md (codex exec answers ready; interactive prompt; placeholder env; A1 source 3; A2 docker-local via vm driver)

### P0.4 Correct the design from the evidence

- **Repo:** jackin. **Owner:** orchestrator. **Depends:** P0.2, P0.3, P0.5, P0.6.
- **Steps:** Compare the P0 evidence with D03, D05, D06, D08, D09, D10, D13, D23, D24, and D28. Change each decision that the evidence refutes. Add one trail row for each change.
- **Proof:** the DESIGN.md diff and the trail rows.
- [x] P0.4 done — DESIGN.md D03 D05 D06 D08 D09 D10 D11 D12 D13 D14 D23 D24 D27 D28, CLI table, Launch sequence corrected; arena between Sonnet and Opus architect analyses with a Sonnet cross-judge; new task P7.4

### P0.5 Prove a Docker engine for sandboxes

- **Repo:** jackin. **Owner:** `openshell-specialist` and `agent-sandbox-engineer`, in parallel, with the same brief. **Depends:** P0.2.
- **Scope:** `rewrite/evidence/P0.5-docker.md`, `rewrite/drafts/docker-*`.
- **Steps:** Try each D24 engine source in order, on the P0.2 driver:
  1. An engine inside the sandbox. Find which OpenShell policy, process settings, and driver let `dockerd` or rootless Docker run as the sandbox user. Run `docker run --rm hello-world` in the sandbox.
  2. A dedicated external engine on OrbStack. Permit only its endpoint in the policy. Run `docker run --rm hello-world` from the sandbox. Find whether OpenShell can keep the engine credentials outside the sandbox.
  3. The host engine, with the same test.
  4. For each source, record whether it works, the exact settings, the isolation that it gives, and the credential path.
- **Proof:** `evidence/P0.5-docker.md` exists, and a trail row names the chosen source. If no source works, write `blockers/P0.5-docker.md` and continue. The capability then stays `BLOCKED` until upstream OpenShell changes.
- [x] P0.5 done — 7d0c04bb, evidence/P0.5-docker.md (source 2 dedicated external engine; source 1 fails on vm driver; credential gap in blockers/P0.5-docker-credentials.md)

### P0.6 Prove a terminal session in a console pane

- **Repo:** jackin. **Owner:** `openshell-specialist`, with `rust-cli-tui-engineer` for the prototype. **Depends:** P0.2.
- **Scope:** `rewrite/evidence/P0.6-panes.md`, a prototype on a local worktree branch that you do not merge.
- **Steps:**
  1. In a Rust prototype, open an `ExecSandboxInteractive` session and render it in a ratatui pane. Send keys and a resize.
  2. Attach to the sandbox main process from the same prototype. Try SSH over `ForwardTcp` to the `openshell-main` subsystem with `russh`. If that fails, try a sandbox-side terminal multiplexer started by `jackin-start`.
  3. Detach the pane and attach it again. Verify the replay.
- **Proof:** `evidence/P0.6-panes.md` names the chosen transport for the Codex pane and for shell panes. If no transport works for the Codex pane, write `blockers/P0.6.md`. The Codex pane then opens with the hand-off to `openshell sandbox connect`.
- [x] P0.6 done — 8e619ca9, evidence/P0.6-panes.md (Codex pane: CreateSshSession + ForwardTcp + russh openshell-main; shell pane: ExecSandboxInteractive; ratatui prototype 47 checks PASS on local branch wt/P0.6)

## Phase P1. Delete the old product

Between P1 and P2.2 the workspace does not build. This break is planned. P2.2 ends it.

### P1.1 Delete the old code and assets

- **Repo:** jackin. **Owner:** `mechanic` at `model: sonnet, effort: high`. **Depends:** P0.4.
- **Steps:**
  1. Move the D25 usage crates to `crates/usage/`. Delete their tests and keep them out of the workspace members until P6.3.
  2. Delete the rest of `crates/`, and delete `native/`, `docker/`, `docker-bake.hcl`, `vendor/`, `fuzz/`, `policy/`, `plans/`, `examples/`, `scripts/`, `release.toml`, `ratchet.toml`, `container-path-allowlist.toml`, `code-health-baseline.toml`, `.alint.yml`, `.codebook.toml`, `.xcode-version`, and `Cargo.lock`.
  3. Delete `docs/openshell-v2/` if it still exists.
  4. Delete `docs/integration/`, the `research/`, `roadmap/`, and `development/` sections under `docs/content/`, and every page about Docker runtime, Apple containers, or agents other than Codex. Keep the ADR pages, because P12.1 marks them superseded. Keep the site code.
  5. Delete the `REUSE.toml` annotations for deleted paths.
  6. Make the root `Cargo.toml` an empty workspace with the workspace lints only.
- **Proof:** `git ls-files crates native docker vendor fuzz | grep -v '^crates/usage/' | wc -l` prints `0`.
- [x] P1.1 done — 2ff4c2c4, gate prints 0; 34 usage crates under crates/usage without tests

### P1.2 Rewrite the glossary

- **Repo:** jackin. **Owner:** `docs-writer`. **Depends:** P1.1.
- **Scope:** `CONTEXT.md`.
- **Steps:** Write the glossary for these terms only: operator, role, workspace, provider, agent, account, default account, binding, usage, gateway, sandbox, OpenShell provider, policy, image, construct, Docker capability, launch, attach, shell, export, purge. Use the DESIGN.md meaning for each term.
- **Proof:** the diff, and proof that no other term from the old glossary remains.
- [x] P1.2 done — d58d631e, CONTEXT.md has the 21 terms only; rumdl passes; old terms Instance, Session, Reconnect, Restore, Durable home, Default-home, First-seed, Role hook removed

## Phase P2. Build the skeleton and the gates

### P2.1 Design the crate APIs

- **Repo:** jackin. **Owner:** orchestrator runs `pstack:architect` with `architect` runners. **Depends:** P1.1.
- **Scope:** new `rewrite/API.md`.
- **Steps:** Write the public types and function signatures of each D14 crate. Include the `Gateway` trait, `AgentSpec`, the config schema, the role manifest schema, `LaunchPlan`, the launch steps, and the error enums. Include the test plan outline for each crate.
- **Proof:** `rewrite/API.md`, reviewed by a second `architect` run on a different AI model.
- [x] P2.1 done — a27464d6, rewrite/API.md (arena: Sonnet and Opus runners, Sonnet cross-judge picked candidate 1; Opus second-model review CHANGES REQUIRED with 4 blocking findings, all fixed in one round and checked by the orchestrator)

### P2.2 Create the workspace and the eight crates

- **Repo:** jackin. **Owner:** `rust-engineer`. **Depends:** P2.1.
- **Scope:** root `Cargo.toml`, `Cargo.lock`, `crates/*` except `crates/usage/`, `.cargo/config.toml`, `deny.toml`, `clippy.toml`, `rustfmt.toml`, `.config/nextest.toml`. The xtask package name is `jackin-xtask`, so the `.cargo/config.toml` alias works.
- **Steps:**
  1. Create the eight crates from D14 with the API.md signatures and no logic.
  2. Keep the old workspace lint list. Add `unsafe_code = "forbid"`.
  3. In `deny.toml`, ban `bollard` and each Docker client crate, and keep the `openssl` ban.
  4. Add `openshell-sdk` as a git dependency at the pinned tag, used only by `jackin-openshell`.
- **Proof:** G0 passes.
- [x] P2.2 done — c8a358c5, G0 `Finished dev profile [unoptimized] target(s)`; clippy -D warnings, fmt, deny, shear clean

### P2.3 Write the repository gates in xtask

- **Repo:** jackin. **Owner:** `rust-engineer`. **Depends:** P2.2.
- **Scope:** `crates/xtask/`.
- **Steps:** `xtask ci` runs these steps in order and stops at the first failure:
  1. `fmt --check`
  2. `clippy --workspace --all-targets --all-features --locked -- -D warnings`
  3. `nextest run --workspace --all-features --locked`
  4. `deny check`
  5. `shear`
  6. the comment step (D17)
  7. the layer step (D14)
  8. the pins step: `Cargo.toml` `rust-version`, `rust-toolchain.toml`, the `mise.toml` `rust` tool, and each `mbx +<version>` in `mise.toml` tasks agree
  9. the container step (D01). It fails if code runs `podman` or `container`, runs `docker` outside `jackin-image`, or runs a `docker` subcommand that D01 does not permit.
  10. `reuse lint`
  11. the docs build. `--fast` and `--no-docs` skip this step. `--only <step>[,<step>]` runs only the named steps in this order.

  `xtask e2e` is a stub that P8.1 fills.
- **Proof:** G-FAST passes. For each of steps 6, 7, and 8, `test-reviewer` records a red run and a green run. Add a forbidden comment, a forbidden dependency, and a forbidden `docker run` call. Verify that each step fails.
- [x] P2.3 done — 8ead8ba7, G-FAST exit 0 (`18 tests run: 18 passed`; reuse compliant); test-reviewer red/green for all 18 tests and live red/green for steps 6, 7, 8

### P2.4 Update the tool config and the generated CI

- **Repo:** jackin. **Owner:** `cicd-architect`. **Depends:** P2.3.
- **Scope:** `mise.toml`, `hk.pkl`, `renovate.json`, `.velnor/config.toml`, `.github/` through `velnor-actions generate` only.
- **Steps:**
  1. Remove the Swift, weaver, boltffi, and xcodegen tools from `mise.toml`.
  2. Make `hk.pkl` run fmt, clippy, gitleaks, rumdl, and the comment step.
  3. Change `.velnor/config.toml` for the eight crates. Run `velnor-actions generate`. Never edit `.github/workflows/` by hand.
  4. If Velnor cannot express a need, write the need in `blockers/P2.4-velnor.md` for the upstream `tailrocks/velnor-new`.
- **Proof:** `hk check --all` passes. `velnor-actions generate` leaves no diff on a second run.
- [x] P2.4 done — 431279fa, `hk check --all` exit 0; `velnor-actions generate` 0.1.71 leaves no diff on a second run; blockers/P2.4-velnor.md (reuse in CI)

## Phase P3. Build jackin-core

### P3.1 Build the ids, the agent table, and redaction

- **Repo:** jackin. **Owner:** `rust-engineer`. **Depends:** P2.3.
- **Scope:** `crates/jackin-core/src/{ids,agent,redact}.rs`.
- **Steps:** Build the branded ids, `Agent::Codex`, the `AgentSpec` row (D04, D06, D10, D12), the `ProcessRunner` trait, and the redaction of secret values. Take the Codex row values from `rewrite/drafts/agentspec-codex.toml` when P7.1 has written it.
- **Proof:** G-FAST passes. Tests show that the code rejects each invalid id with its reason. Tests show that a secret value never appears in a rendered error.
- [x] P3.1 done — fd42bbde, G-FAST exit 0 (`65 tests run: 65 passed`); test-reviewer KEEP (47 tests, 285 mutants) plus a reviewer-proven hardening patch

### P3.2 Build the config schema and the role manifest

- **Repo:** jackin. **Owner:** `rust-engineer`. **Depends:** P3.1.
- **Scope:** `crates/jackin-core/src/{config,manifest}/`.
- **Steps:** Build the D15 config with atomic writes and the D11 manifest parser, including the D24 `docker` settings and the D26 account bindings. Reject each unknown field and name the key.
- **Proof:** G-FAST passes. Tests show a round trip of a valid file. Tests show the rejection of each invalid case. Tests show that a crash during a write leaves the old file intact.
- [x] P3.2 done — eb69d1c7, G-FAST exit 0 (`99 tests run: 99 passed`); test-reviewer KEEP (34 tests, about 85 mutations)

### P3.3 Build the image plan

- **Repo:** jackin. **Owner:** `rust-engineer`. **Depends:** P3.2.
- **Scope:** `crates/jackin-image/src/plan/`.
- **Steps:** Build the pure part of D05 and D06: the `Dockerfile` validation, the derived layer from the Codex `AgentSpec` row, the cache key, and the buildx argument list with an explicit builder and the named context `construct`.
- **Proof:** G-FAST passes. Tests show that the code rejects a final stage without `FROM construct`. Tests show that each cache key input changes the key. Tests show that the derived layer uses the `AgentSpec` binary path.
- [x] P3.3 done — 7f985da7, G-FAST exit 0 (`132 tests run: 132 passed`); test-reviewer KEEP (16 tests + I9 and views)

## Phase P4. Build the OpenShell adapter

### P4.1 Build the Gateway trait and the SDK adapter

- **Repo:** jackin. **Owner:** `rust-engineer`, with `openshell-specialist` as reviewer. **Depends:** P3.1.
- **Scope:** `crates/jackin-openshell/`.
- **Steps:**
  1. Build the `Gateway` trait from API.md.
  2. Build `GrpcGateway` on `openshell-sdk`. Use raw clients for OpenShell providers and policy.
  3. Map each gRPC status to a `GatewayError` variant.
  4. Build the argument builders for `sandbox upload` and `sandbox download`. Each one takes an explicit gateway, runs with stdin from `/dev/null`, and has a timeout (D03).
  5. Build the terminal sessions from `evidence/P0.6-panes.md`: the main-process attach (`CreateSshSession`, `ForwardTcp`, `russh`) and `ExecSandboxInteractive` with and without a PTY. Expose one session handle with the commands `Input`, `Resize`, and `Detach` and the events `Output`, `Exited`, and `Ended`. Do not leak tonic or russh types.
  6. Resolve the gateway endpoint through `openshell gateway list -o json` (D03).
- **Proof:** G-FAST passes. Tests show the status mapping against an in-process tonic server. Tests show that each argument list names the gateway.
- [x] P4.1 done — 804a4147, G-FAST exit 0 (`217 tests run: 217 passed`); test-reviewer KEEP twice; openshell-specialist review CHANGES REQUIRED, fixed, live re-check PASS (Core 13/13, Transfer 2/2, Terminal 5/5), evidence/P4.1-live.md and P4.1-adapter-live.md

### P4.2 Build the fake gateway

- **Repo:** jackin. **Owner:** `rust-engineer`. **Depends:** P4.1.
- **Scope:** `crates/jackin-testkit/`.
- **Steps:** Build an in-process fake that implements `Gateway` with sandboxes, labels, OpenShell providers, and injectable failures.
- **Proof:** G-FAST passes. The `code-reviewer` of phase P4 confirms that the fake obeys the same contract as `GrpcGateway`.
- [x] P4.2 done — 561d0644 and e1f9f07f, fixed in P4.4 batch F2 (deae699e, b94d8dbc); G-FAST exit 0; code-reviewer confirmation CONFIRMED on b94d8dbc: all 11 phase P4 testkit findings closed, the shared contract suite (24 checks) passes on FakeGateway and live on jackin-local; remaining divergences recorded as fake limits in API.md, download cancel and missing-file class scheduled in P5.2

### P4.3 Build the image builder and the delivery

- **Repo:** jackin. **Owner:** `rust-engineer`. **Depends:** P3.3, P0.3, P7.1, P7.4.
- **Scope:** `crates/jackin-image/src/{buildx,context,engine,ensure}.rs`, the derived `Dockerfile`, `crates/jackin-image/images/construct/Dockerfile`, the `FakeEngine` in `crates/jackin-testkit`, and the host process runner `TokioProcessRunner` in `crates/jackin-session/src/host.rs`, moved forward from P5.2 so the live build proof runs through `BuildxEngine`.
- **Steps:**
  1. Run the buildx plan through the `ProcessRunner` trait of `jackin-core`. Stream the build output as typed progress events.
  2. Read the image ID from the `--iidfile` that buildx writes (U7-A1).
  3. Deliver the image with the D23 method of the gateway.
  4. Put the construct `Dockerfile` and `jackin-start` from P7.4 in the crate. Make `jackin-start` read `/sandbox/.jackin/env-<launch-id>` before each attempt (D11).
- **Proof:** G-FAST passes. `evidence/P4.3-build.md` shows a build of the construct image and of a minimal role. It shows a second build that uses the cache. It shows a delivered image that the gateway can start.
- [x] P4.3 done — ba214a71, G-FAST exit 0 (288 tests run: 288 passed), red/green KEEP twice (9cdc4278 and the fix 00f2b804), evidence `evidence/P4.3-build.md` (L1–L9 and the fix-round re-check: jackin.invalid fail-closed, hostile roles fail, contract Core 13 / Transfer 2 / Terminal 5 live)

### P4.4 Review phase P4

- **Owner:** `code-reviewer` and `security-reviewer` on `<P4 start>..HEAD`. **Depends:** P4.2, P4.3.
- **Proof:** both verdicts in the chat, the fix round, and G-FULL.
- [x] P4.4 done — b94d8dbc, G-FULL exit 0 (`xtask ci --no-docs`: Fmt, Clippy, Nextest 428 run: 428 passed, Deny, Shear, Comments, Layers, Pins, Containers, Reuse compliant); verdicts: code review A CHANGES (1 blocking), B CHANGES (1 blocking), C CHANGES (0 blocking), security CHANGES (1 high, 3 medium); fix batches F1 (31d0a2d4), F3 (55be1a3d), F2 (deae699e, b94d8dbc), each red/green KEEP; live contract Core 15, Transfer 3, Terminal 6 (`evidence/P4.4-live.md`)

## Phase P5. Build jackin-session

### P5.1 Build the launch planner and executor

- **Repo:** jackin. **Owner:** `rust-engineer`. **Depends:** P3.2, P4.2, P4.3.
- **Scope:** `crates/jackin-session/src/launch/`.
- **Steps:** Build the pure planner and the idempotent executor from the DESIGN.md launch sequence. Support cancel.
- **Proof:** G-FAST passes. Tests against the fake gateway prove these behaviors:
  - Two runs with one launch ID give one image build, one sandbox, one upload, and one setup run.
  - A failure after create leaves the sandbox, and a retry continues it.
  - A cancel leaves the sandbox as a restore candidate.
  - Jackin never reuses a live sandbox as a new launch (ADR-008).
- [x] P5.1 done — cafccc7c, G-FAST exit 0 (394 tests run: 394 passed), red/green CHANGES (217 mutations, 15 survivors) then KEEP on the fix round and the rebase rows; the PLAN proofs are tests/launch proofs.rs (one launch ID: one build, one sandbox, one upload, one setup run), crash.rs (failure after create keeps the sandbox, a retry continues), cancel.rs (cancel leaves a restore candidate), adr008.rs (no reuse of a live sandbox)

### P5.1b Upload the project as a working tree and a git bundle

- **Repo:** jackin. **Owner:** `rust-engineer`. **Depends:** P5.1, P4.4.
- **Scope:** `crates/jackin-openshell` (replace `upload_project` with a tree upload and a single-file upload; delete the `.git` copy, `git_config.rs`, the sanitizer and `UnsupportedGitDirectory`), a new project module in `crates/jackin-session`, the launch upload step, the testkit fake, `jackin doctor`'s git check.
- **Steps:** Build D08 as written: host checks and refusals, the bundle, the emptied project directory, the two uploads with the digest check, the sandbox repository, the tree-id gate, the marker last. Delete the superseded code in the same change.
- **Proof:** G-FAST passes. Tests with real host git show a normal repository, a linked worktree, a submodule checkout as the root, a SHA-256 repository, a shallow clone, an unborn branch with other branches, and a detached HEAD each arrive with the same HEAD and tree. Tests show each refusal before any gateway call, a host edit during the upload fails with `ProjectMismatch`, a retry after a host deletion succeeds, and no `refs/heads/jackin/*` ref and no host credential reach the sandbox. `evidence/P5.1b-upload.md` shows the live upload from a linked worktree, the bundle digest, and an export back.
- [x] P5.1b done — 073ac4c5, G-FAST exit 0 (465 tests run: 465 passed, 3 of 3 runs under load), red/green CHANGES (5 fixes, one real bug: the jackin ref exclusion did not cross `/`) then KEEP; evidence `evidence/P5.1b-upload.md` (live: normal repo, linked worktree, submodule root, SHA-256, shallow, symlinks and NFD names arrive with the same HEAD and tree on sandbox git 2.43; nested repo refused; retry clears stale files; export from a worktree does not nest; contract Core 15, Transfer 4, Terminal 6)

### P5.2 Build attach, shell, export, list, stop, start, and purge

- **Repo:** jackin. **Owner:** `rust-engineer`. **Depends:** P5.1b.
- **Scope:** `crates/jackin-session/src/{attach,export,lifecycle,doctor}.rs` (the host runner landed in P4.3), with `restart`, `inspect`, and `doctor` (API.md U10), and the fake download fixes scheduled in P4.2.
- **Steps:** Build each operation from DESIGN.md. `purge` refuses unexported work without `--force` (ADR-010).
- **Proof:** G-FAST passes. Tests show that an export writes only `refs/heads/jackin/<sandbox>` and never the working tree. Tests show that purge refuses unexported work. Tests show that list finds sandboxes by label only.
- [x] P5.2 done — 20842a9b, G-FAST exit 0 (514 tests run: 514 passed), red/green CHANGES (8 survivors, `--atomic` shown observable; bundle read into memory) then KEEP; tests: export writes only `refs/heads/jackin/<sandbox>/...` and leaves the working tree, index, HEAD and other refs unchanged (tests/export refs.rs, worktree.rs), purge refuses unexported and unknown work without `--force` (purge.rs), list finds sandboxes by label only (tests/lifecycle list.rs, foreign.rs); evidence `evidence/P5.2-export-live.md` (live export from a normal repo and a linked worktree with the host unchanged, guarded and forced purge, contract Core 15, Transfer 6, Terminal 6); bug found: two exports of one sandbox collided on one sandbox bundle path, fixed with one bundle per export

### P5.3 Review phase P5

- **Owner:** `code-reviewer` and `security-reviewer` on `<P5 start>..HEAD`. **Depends:** P5.2.
- **Proof:** both verdicts in the chat, the fix round, and G-FULL.
- [x] P5.3 done. Proof: code-reviewer A/B/C and security-reviewer verdicts (/private/tmp/claude-501/plans/P5.3-review-A.md, -B.md, -C.md, P5.3-security.md) with the G1 G2 G3 fix round (acead3eb); pstack interrogate (Sonnet, Opus, Haiku; four verified findings V1-V4, V1 a live leak) with the int fix round merged at 9cc24701 and red/green APPROVE after R1-R4; live contract Core 15/15, Transfer 8/8, Terminal 6/6 and export scenario on jackin-local; G-FULL `xtask ci --no-docs` 773 passed, exit 0 on 9cc24701.

### P5.4 Build the Docker capability

- **Repo:** jackin. **Owner:** `agent-sandbox-engineer`. **Depends:** P5.3, P0.5.
- **Scope:** the D24 parts of `crates/jackin-core/`, `crates/jackin-image/`, and `crates/jackin-session/`.
- **Steps:** If P0.5 is `BLOCKED`, build only the settings and a launch error that names the blocker. Otherwise, build D24 with the engine source that P0.5 chose. Resolve the capability from the role and the workspace. Add the Docker layer to the derived image only when the capability is enabled. Add the engine policy rule only when the capability is enabled. Show the D24 warning.
- **Proof:** G-FAST passes. Tests show that the capability is disabled by default. Tests show that the role or the workspace can enable it. Tests show that the cache key changes with it.
- [x] P5.4 done. Proof: merged at 9666419c9; G-FAST 1083 passed; red/green APPROVE (/private/tmp/claude-501/plans/P5.4-redgreen.md) with the four follow-ups proven red; live L1-L4 and L7-L9 against the rootless test engine in evidence/P5.4-docker.md (hello-world from a preflight hook, docker build, run -i, exec, compose; key 0600 in 0700 and absent from output; disabled has no client, file, variable, or rule; StaleSandbox on a flipped capability or engine port); engine binaries forbidden in every image.

### P5.5 Review the Docker capability

- **Owner:** `security-reviewer`, then the orchestrator runs `pstack:interrogate`. **Depends:** P5.4.
- **Proof:** the verdicts in the chat. No open blocking finding.
- [x] P5.5 done. Proof: security-reviewer CHANGES_REQUIRED (/private/tmp/claude-501/plans/P5.5-security.md, F1 high) fixed and red/green APPROVE (816ef7d14); pstack interrogate (Sonnet, Opus, Haiku; seven verified findings V1-V7, /private/tmp/claude-501/plans/P5.5-interrogate.md) fixed on wt/P5.5-int with red/green KEEP and the four review edits proven red, merged at 28164db91; G-FAST 1187 passed; live IP-literal check in evidence/P5.4-docker.md.

## Phase P6. Build the CLI

### P6.1 Build the clap commands

- **Repo:** jackin. **Owner:** `rust-cli-tui-engineer`. **Depends:** P5.2.
- **Scope:** `crates/jackin/`.
- **Steps:** Build each command in the DESIGN.md CLI table. Write help text with clap attributes, not doc comments (D17). `--json` output is stable and has a typed schema.
- **Proof:** G-FAST passes. Tests run the binary against the fake gateway and assert the exit code, the JSON output, and the resulting fake state.
- [x] P6.1 done. Proof: merged at ad60590d; G-FAST `xtask ci --fast` 650 passed, exit 0; red/green by test-reviewer in three rounds (/private/tmp/claude-501/plans/P6.1-redgreen.md, -2.md, -3.md), verdict APPROVE.

### P6.1b Name identifiers plainly in user-facing errors

- **Repo:** jackin. **Owner:** `rust-engineer`. **Depends:** P6.1.
- **Scope:** every `thiserror` message in `jackin-core`, `jackin-image`, `jackin-openshell`, `jackin-session`, and `jackin`.
- **Steps:** Replace each `{x:?}` of an identifier or a path in an error message with its `Display` form, so the user reads `jackin-work-codex`, not `Provider(ProviderName("jackin-work-codex"))`. Add a repo gate that refuses `:?}` in an `#[error(...)]` attribute. Fix the P6.2 defects: a manifest error that repeats its cause (`unknown field x: unknown field x`); `role check` on a missing directory, which must name the directory; `jackin ls` on an empty gateway, which must print that there are no sandboxes.
- **Proof:** G-FAST passes; the new gate goes red on a reintroduced `:?}`.
- [x] P6.1b done. Proof: merged at 427ae44a; G-FAST `xtask ci --fast` 702 passed, exit 0; the `errors` CI step refuses Debug placeholders, source fields, and bare String fields in error messages; red/green by test-reviewer (/private/tmp/claude-501/plans/P6.1b-redgreen.md: about 205 mutations, then R1-R5, the String rule, and 104 plain removals red); E1 and E2 proven red by the implementer as the reviewer specified.

### P6.3 Port the usage crates

- **Repo:** jackin. **Owner:** `rust-engineer`, with `mechanic` for the comment removal. **Depends:** P6.1.
- **Scope:** `crates/usage/`, the workspace members, the `jackin usage` command.
- **Steps:**
  1. Apply D25. Remove each dependency on a deleted crate.
  2. Remove every comment with a script, then run the D17 comment step.
  3. Make each usage provider read the credentials of a jackin account of that provider on the host.
  4. Add the usage crates to the workspace members and to the layer step.
  5. Build `jackin usage` and `jackin usage --json`. Show each limit window that a provider reports, such as the 5-hour and weekly windows, with the percentage used, the remaining amount, and the reset time.
- **Proof:** G-FAST passes with the usage crates in the workspace. Tests verify the usage behavior through public interfaces, with a fake HTTP server for each provider backend that a test covers.
- [x] P6.3 done. Proof: merged at 4cdd4514; G-FAST 1020 passed, exit 0; 13 usage crates (facade, provider-core, 11 providers with Amp unsupported) and `jackin usage [--json]`; red/green by test-reviewer in three rounds (/private/tmp/claude-501/plans/P6.3-redgreen.md); live read-only run with the host Codex login in evidence/P6.3-usage-codex.md (weekly window 1% used, credits, no token refresh).

### P6.2 Run the CLI against the real gateway

- **Owner:** `verifier` at `model: sonnet, effort: high`. **Depends:** P6.1, P0.2.
- **Steps:** Run `jackin doctor`, `jackin ls --json`, and `jackin role check` on a valid and an invalid manifest against `jackin-local`.
- **Proof:** `evidence/P6.2-cli.md`.
- [x] P6.2 done. Proof: evidence/P6.2-cli.md (doctor 7/7 pass against jackin-local; `ls --json` matches `openshell sandbox list`; role check exits 0 on a valid manifest and 1 on an unknown key, a missing field, and a bad stage). Defects moved to P6.1b.

## Phase P7. Write the construct image and the sentinel role

P7.1 can start after P0.3. It runs in parallel with P2 to P6. The image drafts that P7.1 and P7.4 wrote now live in `crates/jackin-image/images/` and `crates/jackin-core/src/assets/` (P3 fix round); the scope lines below are historical.

### P7.1 Write the construct image and the derived layer

- **Repo:** jackin. **Owner:** `agent-sandbox-engineer`. **Depends:** P0.3, A2.
- **Scope:** `rewrite/drafts/construct.Dockerfile`, `rewrite/drafts/derived.Dockerfile`, `rewrite/drafts/jackin-start`, `rewrite/drafts/agentspec-codex.toml`.
- **Steps:** Finish the D06 construct image, the D05 derived layer, and the D09 `jackin-start` launcher from the P0.3 drafts. Write the Codex `AgentSpec` row values in `agentspec-codex.toml`. P4.3 moves them into `jackin-image`.
- **Proof:** `evidence/P7.1-image.md` shows the output of `id` and `codex --version`. It shows that a derived image has no `sudo` and no `docker`.
- [x] P7.1 done — 349e8d3a, evidence/P7.1-image.md (uid 1000 sandbox, codex-cli 0.166.0, no sudo, no docker; jackin-start markers proven)

### P7.2 Rewrite the sentinel role

- **Repo:** jackin-sentinel. **Owner:** `agent-sandbox-engineer`. **Depends:** P7.1, P7.4, P6.1, P0.4.
- **Steps:**
  1. Delete every Claude file, old hook, old script, and the old manifest.
  2. Write a `Dockerfile` whose final stage starts `FROM construct`. It installs `jackin-sentinel-probe` in a `RUN` layer, so the build output is visible.
  3. Write `jackin.role.toml` (D11), `policy.yaml` (D12), `hooks/setup.sh`, and `hooks/preflight.sh`. The preflight hook fails when `SENTINEL_FAIL_PREFLIGHT=1`.
  4. Write `jackin/rewrite/probe-schema.json` first. Then write the probe. It prints one JSON object with the result of each in-sandbox journey step, including the J15 and J16 Docker steps.
  5. Rewrite `README.md`.
- **Proof:** G-ROLE passes after P6.1. `evidence/P7.2-sentinel.md` shows a build by hand in the P0.3 order. It shows the probe output in a sandbox.
- [x] P7.2 done. Proof: sentinel effbe8b, jackin 2f052bd7; G-ROLE exit 0; evidence/P7.2-sentinel.md (build in the P0.3 order, role layer CACHED on rebuild, probe in a jackin-local sandbox: J00 J04 J05 J06 J10 J11 J12 J13 J14 J16 PASS, J07 J08 J09 J15 SKIP for no credential or no Docker capability; schema validates).

### P7.3 Review the images, the OpenShell provider profile, and the policy

- **Owner:** `security-reviewer`, then the orchestrator runs `pstack:interrogate`. **Depends:** P7.2.
- **Proof:** the verdicts in the chat. No open blocking finding.
- [x] P7.3 done. Proof: security-reviewer CHANGES_REQUIRED (/private/tmp/claude-501/plans/P7.3-security.md, F1 blocking) with the policy fix (66602557) and the image fix (b3087a4e), each red/green KEEP; pstack interrogate (Sonnet, Opus; Haiku stopped) with five verified findings I1-I5 (/private/tmp/claude-501/plans/P7.3-interrogate.md) fixed at bdedc47b with red/green KEEP after R1 and R2; the role-context symlink fix (32f89501); live BuildKit proofs and the real sentinel build in evidence/P7.3-image.md; G-FAST 1054 passed.

### P7.4 Prove the jackin-start supervisor

- **Repo:** jackin. **Owner:** `agent-sandbox-engineer`. **Depends:** P0.4, P7.1.
- **Scope:** `rewrite/drafts/jackin-start`, `rewrite/drafts/derived.Dockerfile`, `rewrite/drafts/agentspec-codex.toml`, new `rewrite/evidence/P7.4-supervisor.md`.
- **Steps:** Rewrite `jackin-start` to the D09 supervisor. Remove the hooks from it. Prove each fact on `jackin-local` with the derived image:
  1. The state goes `waiting`, then `running <attempt>` after jackin writes the attempt ID, then `exited <attempt> <code>` after Codex quits. The sandbox stays `Ready` after Codex exits with 0 and with a non-zero code. Exec and `git bundle` still work.
  2. A second attempt ID starts Codex again. A stale attempt ID does nothing.
  3. Ctrl-C while `jackin-start` waits does not end it. A stop and a start give `waiting` again.
  4. Codex as the child gets the PTY: the first prompt appears on attach, and a resize reaches it.
  5. A hook under `/usr/local/lib/jackin/hooks` runs through a non-PTY exec with the `[env]` values. A failing hook returns its exit code, and the sandbox stays `Ready`. Closing the exec stream stops a long hook.
  6. A file written through exec stdin arrives with mode 0600 and the exact bytes.
- **Proof:** `evidence/P7.4-supervisor.md` shows each step with raw output.
- [x] P7.4 done — 4a5c599a, evidence/P7.4-supervisor.md (six steps PASS: state machine, fencing, signals, Codex on the PTY, hooks through exec, 0600 stdin writes)

## Phase P8. Prove the sentinel journey

This phase is the first end-to-end proof.

### P8.0 Look for newer stable releases

- **Owner:** `docs-researcher`. **Depends:** P7.3.
- **Steps:** Look for newer stable releases of the D02 tools. For each newer release, add a task that updates the pin and runs the affected live proofs again.
- **Proof:** a trail row with the versions found.
- [x] P8.0 done. Proof: decisions.tsv row 2026-11-06 P8.0 and /private/tmp/claude-501/plans/P8.0-releases.md (newer: cargo-nextest 0.9.151, hk 2.11.0, baseline velnor-actions 0.1.72, zizmor 1.32.0; OpenShell has no stable release after 0.1.6). Task P8.0a bumps them.

### P8.0a Bump the pins that P8.0 found

- **Owner:** `cicd-architect`. **Depends:** P8.0.
- **Scope:** the pins in jackin, jackin-sentinel, and terminal-components-baseline (`mise.toml`, `mise.lock`, `hk.pkl`, the nextest pin, the D02 table row by the orchestrator) and the Velnor-generated workflows.
- **Steps:** Bump cargo-nextest to 0.9.151 in every repository, hk to 2.11.0 with the `hk.pkl` package URLs, velnor-actions to 0.1.72 in the baseline with regenerated workflows, and zizmor to 1.32.0 where it is pinned. Run each repository's gate.
- **Proof:** each repository's gate passes on the bumped pins; regenerated workflows show no diff on a second run.
- [x] P8.0a done. Proof: jackin (cargo-nextest 0.9.151, hk 2.11.0, zizmor 1.32.0, velnor-actions 0.1.72 with a root zizmor.yml ignoring only self-repository in ci.yml, which the 0.1.72 generator leaves without its inline ignore) G-FAST 1034 passed and `hk check --step zizmor` exit 0; baseline 3a7c020 (cargo-nextest 0.9.151, zizmor 1.32.0, velnor-actions 0.1.72) functional suite 106/106 before and after; sentinel already current; checksums match the release assets; generate twice gives no diff.

### P8.1 Build xtask e2e

- **Repo:** jackin. **Owner:** `rust-engineer`. **Depends:** P6.1, P7.2, P8.0.
- **Scope:** `crates/xtask/src/e2e/`. Drive the PTY steps J11 and J12 with the `portable-pty` crate. Validate the probe output against `rewrite/probe-schema.json`.
- **Steps:** Run each journey step in order. Compare the host observation with the probe output. Print one line for each step. The line has the ID, `PASS`, `FAIL`, or `SKIP`, and the reason. Exit non-zero if a step fails.
- **Proof:** a run against `jackin-local`.
- [x] P8.1 done. Proof: merged at 8ccca7f60; G-FAST 1164 passed; red/green by test-reviewer in two rounds (/private/tmp/claude-501/plans/P8.1-redgreen.md, 181 named breaks) with `Gateway::sandbox_environment` (SandboxSpec.environment, proto field 5); live run against jackin-local with every step J00-J17 PASS (/private/tmp/claude-501/e2e/run5.out).

### P8.2 Run the journey

- **Owner:** orchestrator. **Depends:** P8.1, A1.
- **Steps:** Run G-E2E. Fix each `FAIL` through the code task pipeline. Run G-E2E again.
- **Proof:** every journey step prints `PASS`. Copy the full output into `evidence/P8.2-journey.md` and into the chat.
- [x] P8.2 done. Proof: evidence/P8.2-journey.md, G-E2E at 483bc5ab8: J00-J17 PASS, RESULT PASS=19 FAIL=0 SKIP=0, CLEAN PASS, A1 host login unchanged, exit 0.

### P8.3 Repeat the journey independently

- **Owner:** orchestrator runs `pstack:swarm` with three `verifier` lanes. **Depends:** P8.2.
- **Steps:** Each lane starts from a fresh clone of the head SHA, a new workspace name, and no old sandboxes. Each lane runs G-E2E. With A1 source 3, the lanes run one after the other.
- **Proof:** three PASS outputs in `evidence/P8.3-lanes.md`.
- [x] P8.3 done. Proof: evidence/P8.3-lanes.md, three verifier lanes from fresh clones of 8c1f604b0, each RESULT PASS=19 FAIL=0 SKIP=0 and exit 0 (181.1 s, 183.2 s, 182.6 s).

### Sentinel journey

| ID | Step | PASS when |
|---|---|---|
| J00 | Image build | `jackin build sentinel --rebuild` builds the image and shows the sentinel `RUN` output. A second `jackin build sentinel` uses the cache. The derived image has Codex at the pinned version at `/usr/local/bin/codex`. |
| J01 | Doctor | `jackin doctor` reports OpenShell at the pinned version and a healthy gateway. |
| J02 | Strict manifest | `jackin role check` on a manifest with an unknown field exits non-zero and names the field. |
| J03 | One sandbox per launch | `jackin load` gives one sandbox with all the D07 labels and a name of at most 19 characters, and the image label matches the first 32 hex characters of the J00 image ID. A retry with the same launch ID gives no second sandbox. |
| J04 | Sandbox identity | The probe reports the user `sandbox`, no working `sudo`, and no `docker` binary. |
| J05 | Upload | The digests of the probe match the digests of `git ls-files` on the host. An ignored file is absent. `git status --porcelain` on the host is the same before and after. |
| J06 | Hooks | `setup` ran once across two starts. `preflight` ran at each start. With `SENTINEL_FAIL_PREFLIGHT=1`, the launch stops with the hook error, the sandbox stays `Ready`, and `jackin export` works. A retry without the variable starts Codex. |
| J07 | Codex | Codex at the pinned version runs as the child of `jackin-start`, the sandbox main process. `codex exec` answers the fixed prompt. After Codex exits, the sandbox stays `Ready` and `jackin attach --restart` starts it again. |
| J08 | Credential | The sandbox environment has only a placeholder. The key is absent from the sandbox files, `jackin inspect`, and the captured stderr of each jackin command in the journey. The probe reads the `env-kinds` record: `SKIP` without a fresh valid record, `FAIL` for a malformed record or any `other` kind. The host side checks the environment of the create request and the env-file bytes too. A provider host does not echo a resolved credential back to the agent (P7.3 F3). |
| J09 | Egress | Codex reaches its API. A host outside the policy is denied. A process that is not Codex or started by Codex, such as `curl` in `jackin shell`, gets no credential. `jackin role check` rejects a role policy with a rule for a Codex provider host (D10). |
| J10 | Filesystem | A write outside `/sandbox` and `/tmp` is denied. |
| J11 | Detach and reattach | After a detach, the Codex process is still alive. `jackin attach` replays the output. |
| J12 | Shell | `jackin shell` opens a second process. The Codex process does not change. |
| J13 | Export | A commit made in the sandbox appears at `refs/heads/jackin/<sandbox>/<branch>` on the host. The host working tree does not change. |
| J14 | Lifecycle | Stop and start keep the files. `purge` refuses unexported work. `purge --force` deletes the sandbox. Other sandboxes do not change. |
| J15 | Docker capability on | With `SENTINEL_DOCKER=1`, the workspace has `docker = true`. In the sandbox, `docker run --rm hello-world` passes. The container exists only in the chosen engine. |
| J16 | Docker capability off | Without the capability, the sandbox has no `docker` binary, and a connection to the engine endpoint is denied. |
| J17 | Accounts and usage | With no `--account` flag, `jackin load` uses the workspace binding. Without a workspace binding, `jackin load` uses the role binding. Without both bindings, it uses the provider default. The Codex accounts other than the A1 account use a fixture login whose refresh token does not work, so they never rotate the A1 token (D10). J17 observes the selection through the account label and the OpenShell provider of the sandbox. `jackin usage --json` has a record for each account that has credentials. Each Codex record has every window Codex reports for that plan (the live A1 account reports a weekly window and no 5-hour window, evidence/P6.3-usage-codex.md), each with the percentage used and the reset time, or a typed reason when Codex gives no data. |

## Phase P9. Build the console

P9.1 and P9.2 change the baseline repository. They can start after P2.1 and run in parallel with P3 to P8. Each baseline task obeys the baseline `AGENTS.md`.

### P9.1 Move the jackin-preview screens into baseline blocks

- **Repo:** terminal-components-baseline. **Owner:** `rust-cli-tui-engineer`. **Depends:** P2.1.
- **Scope:** the baseline `src/` and the `jackin-preview` binary. Never the files under `tests/functional/baselines/`.
- **Steps:**
  1. Run the full baseline suite and record the result as the before state.
  2. Move each D27 screen pattern into a reusable block behind a data interface. Make `jackin-preview` use the blocks with its demo data.
  3. Add the external message input, the terminal pane block that renders a session from a byte stream (D27), and the `clap` feature (D27, D28).
  4. Run the full baseline suite again.
- **Proof:** the full baseline suite passes. `git diff --stat -- tests/functional/baselines` prints nothing. Record both outputs in `jackin/rewrite/evidence/P9.1-baseline.md`.
- [x] P9.1 done — baseline 2565a0c, evidence/P9.1-baseline.md (93/93 before and after; baselines diff empty vs 95bbcbb and f7f5769)

### P9.2 Add the new jackin screens to the baseline

- **Repo:** terminal-components-baseline. **Owner:** `ux-specialist` writes the screen specs. `rust-cli-tui-engineer` builds them. **Depends:** P9.1.
- **Steps:** Specify and build the role editor, the role binding, the Docker setting, and the doctor panel with baseline components. Follow the demo navigation and key bindings (D28). Add them to `jackin-preview`. Approve one new tuiscotti baseline for each new screen.
- **Proof:** the full baseline suite passes. The existing baseline files have no diff. `jackin/rewrite/evidence/P9.2-baseline.md` lists the new baseline files.
- [x] P9.2 done — baseline 0734f95, evidence/P9.2-baseline.md (8 new baselines; 106/106; no existing baseline changed)

### P9.3 Wire the console to jackin data

- **Repos:** terminal-components-baseline (the baseline steps), then jackin (the jackin steps). **Owner:** `rust-cli-tui-engineer`. `ux-specialist` reviews every new baseline look. `test-reviewer` approves each test plan and records the red and green runs. **Depends:** P8.2, P9.2.
- **Rule (D27, D28):** jackin has one console application, `jackin-console`, from `https://github.com/tailrocks/terminal-components-baseline`, branch `feature/openshell-native`. It runs as the demo `jackin-preview` and as the jackin product. Jackin depends on `junie-tui` and `jackin-console` only as git dependencies with that URL, a full `rev` on that branch and `default-features = false`. A path dependency, a copy, another URL, a branch or tag reference, and a `[patch]` of these crates are errors, and the `baseline` step of `xtask` fails on each. The demo data, `SimBackend`, the scripted sessions, the fixtures and every demo-only code path stay in the baseline behind the Cargo feature `demo`, which is off by default. No demo data and no demo symbol reaches the jackin build, and the `demo-gate` step of `xtask` fails if one does. No screen code, pixel or screen text is written in jackin: a missing screen, control, word or behavior is added to the baseline. Jackin Capsule runs on the host, and its panes use `termpane` through `junie-tui`.
- **Scope:** the baseline `apps/jackin` and `src/` for the baseline steps. In jackin: `crates/jackin-tui/`, the console entry in `crates/jackin/`, `crates/jackin-testkit/`, `crates/xtask/`, `deny.toml`. The design is `P9.3-design-v7`.
- **Steps:**
  1. Baseline, one app in two modes (B1b, B2c, B3b, B5b, B8a, B6, B7a): the feature `demo`, `default = []`, the bin `jackin-preview`; `Entry` and `AppOptions` instead of `Scenario`; the live-data constructor `Surfaces::openshell`; the demo literals, the scripted pane player, the simulated outcomes and the simulated launch move into `fixtures` and `sim`; effects carry a payload with its base and failure replies, and a successful reply carries the new snapshot, so no screen writes `World` after an effect. Proof: the full baseline suite with `--all-features` and the product-shape job with `--no-default-features --features product` pass, and `git diff --stat -- tests/functional/baselines apps/jackin/tests/functional/baselines` prints nothing.
  2. Baseline Fixed controls (B4a, B4b, B4c): every control is live, mapped, or fixed with a visible reason. Each new look has one new baseline reviewed by `ux-specialist`.
  3. Baseline terminal block (B8-0, B8-1, B8b, B5, B7b): `termpane` replaces `vt100` under `junie_tui::terminal`; `showcase_page_terminalpane` and `showcase_terminal_replay_mid_sequence` leave the baselines byte-identical; the terminal viewport (selection, copy, scroll); the terminal pane with its seven link states, focus events and attention; pending results; the launch views.
  4. Jackin prerequisites and the testkit (J0a, J0b): the session, core and testkit changes in `API.md` sections 1 to 4 and 7 that the console needs.
  5. Pin and wire (J1, J1b): the two dependency lines of D27, the `baseline`, `screen` and `demo-gate` steps, `deny.toml`, `build_console` and `run_console`, the read-only projection of config, fleet and gateway health, and the doctor panel.
  6. Data and persistence (J2, J3): persist every live edit through `ConfigStore::edit`; show the D24 Docker setting, the D25 usage windows and the D26 accounts, defaults and bindings; the 1Password flow up to usage.
  7. Launch (J4): the cockpit and the launch events from `Session::launch`.
  8. Capsule (J5): the D28 session tabs and panes with the P0.6 transport, the byte path through `termpane`, replies, scrollback, selection and copy, the clipboard policy, export. `jackin attach` and `jackin shell` use the same sessions (D03).
  9. Entry (J6): `jackin` with no arguments opens the console on a terminal. `xtask product-check` passes on the release build.
- **Proof:** G-FAST passes at each step. Tests drive key sequences and assert the resulting jackin config and session state. Each test has an independent red run and green run.
- [x] P9.3 done. Proof: baseline feature/openshell-native at 912b0a8 (steps B1b, B2c, B3b, B4a-B4c, B5, B5b, B6, B7a, B7b, B8-0, B8-1, B8a, B8b, BG, BG2, BG3, BG4, BG5, each red and green in docs/P9.3-red-green.md, batch code reviews, red/green reviews and UX reviews with one fix round each; full baseline suite and the product shape pass, approved baselines unchanged except the reviewed re-takes); jackin at 0405be170 (J1a, J1, J1b, JBG, J2, J3, J4, J5, JBG4, J6 and the J1, J2-J4 fix rounds, red and green in rewrite/reconciliation/P9.3.md; batch reviews P9.3-J1-review.md, P9.3-J2-J4-review.md, red/green P9.3-J1-redgreen.md and P9.3-J2-J5-redgreen.md APPROVED); G-FAST 1761 passed (pre-push); xtask product-check exit 0 on the release build, no demo canary in target/release/jackin; `jackin` with no arguments opens the console on a TTY.

### P9.4 Prove that the console is identical to the baseline

- **Repo:** jackin. **Owner:** `rust-cli-tui-engineer`, with `test-reviewer` for the red and green record. **Depends:** P9.3.
- **Steps:** Add the D27 identity gate to the nextest suite. The gate runs the `console-fixture` binary of `jackin-testkit`, which builds the pinned `jackin-console` through `assemble`, the same path the product's `build_console` takes (design 4.2, G16), with the baseline feature demo enabled only in that test binary; `build_console` itself projects live jackin data and cannot show the fixture worlds. It runs the jackin console in a PTY on the baseline fixture data. It drives each demo scenario and each key-driven demo state with the demo key sequence (D28). It verifies each result against its `jackin-preview` tuiscotti baseline.
- **Proof:** G-FAST passes. `test-reviewer` records a red run after a one-cell color change and a green run after the revert. xtask demo-gate passes.
- [x] P9.4 done. Proof: identity target in crates/jackin-testkit/tests/identity compares 41 journeys and 54 approved jackin-preview files of the pinned baseline (adbbfe8) through tuiscotti; test-reviewer red after a one-cell colour change (`1 changed cells ... fields ["fg"]`) and green after revert (/private/tmp/claude-501/plans/P9.4-redgreen.md), its FIX (colour test bypassing the visual path) fixed with the weakening break red; load flake fixed (0 of 160 under 24 burners); xtask demo-gate exit 0; G-FAST passes.

### P9.5 Drive the console live

- **Owner:** `verifier` at `model: sonnet, effort: high`, with the `run` skill. **Depends:** P9.4.
- **Steps:** Start the console in a PTY against `jackin-local`. Follow the demo flows with real data. Create a workspace. Add a Codex account and an account of one other provider. Set a default account. Bind accounts to the sentinel role and to the workspace. Open the usage panel. Launch the sentinel role with the Codex login from --chatgpt-from. Open a shell pane next to the Codex pane. Detach. Attach again from the list. Purge.
- **Proof:** terminal captures in `evidence/P9.5-console.md`.
- [x] P9.5 done. Proof: evidence/P9.5-console.md (run 2 at 1f04feb8c against jackin-local on OpenShell 0.1.7: all 12 steps PASS with no retry, Codex ChatGPT login and an env-source Claude account, launch with the Codex pane attached, shell pane `echo p95-shell-ok`, Ctrl+B d detach with the sandbox kept, reattach with both panes, purge; cleanup left no sandbox and no provider). Run 1 failed steps 8 and 9 (panes could not attach after a launch); the P9.5 fix round fixed the cause.

## Phase P10. Add the ChatGPT login mode

### P10.1 Prove credential mode B

- **Repo:** jackin. **Owner:** `agent-sandbox-engineer`. **Depends:** P8.2.
- **Steps:** P4.1, P5.1, and P6.1 build mode B, because the journey in P8.2 needs it (D10). This task proves it in isolation and hardens it. Prove D10 mode B. `jackin account add codex --chatgpt-from <codex-home>` imports the login into the OpenShell provider. The sandbox gets a placeholder `auth.json`. Device login is interactive only and is not part of the proof.
- **Proof:** G-FAST passes. J07 and J08 pass with a mode B account. `evidence/P10.1-chatgpt.md` shows the output.
- [x] P10.1 done. Proof: evidence/P10.1-chatgpt.md; G-E2E at bec2f810b with the mode B ChatGPT account: J00-J17 PASS, RESULT PASS=19 FAIL=0 SKIP=0, a1_host_login=unchanged, exit 0; G-FAST 1542 passed at d0219aac4 (pre-push gate).

### P10.2 Review mode B

- **Owner:** `security-reviewer`, then the orchestrator runs `pstack:interrogate`. **Depends:** P10.1.
- **Proof:** the verdicts in the chat. No open blocking finding.
- [x] P10.2 done. Proof: security-reviewer APPROVE (/private/tmp/claude-501/plans/P10.2-security.md; F1 echoed values, F2 placeholder check, F3 sync lock fixed in 9a2042556); pstack interrogate with three adversaries (sonnet, opus, haiku), every finding rerun by the orchestrator (/private/tmp/claude-501/plans/P10.2-interrogate.md: account edits planned before the lock, value echo outside --credential, secret-shaped env and file credentials), all fixed at 5af16cf6c with red and green in rewrite/reconciliation/P10.2.md; G-FAST 1567 passed; no open blocking finding.

## Phase P11. Rewrite Agent Smith and The Architect

P11.1 and P11.2 run in parallel.

### P11.1 Rewrite Agent Smith

- **Repo:** jackin-agent-smith. **Owner:** `agent-sandbox-engineer`. **Depends:** P8.2.
- **Steps:** Apply D20. Delete every Claude file and the marketplace scripts. Write the Dockerfile, manifest, policy, review instructions for Codex, and README.
- **Proof:** G-ROLE passes. `evidence/P11.1-smith.md` shows that `jackin load agent-smith` builds the image and starts Codex.
- [x] P11.1 done. Proof: jackin-agent-smith feature/openshell-native (Codex role, FROM construct, github.com-only git policy, review AGENTS.md installed by preflight); G-ROLE exit 0; repo gates pass; evidence/P11.1-smith.md shows `jackin load agent-smith` building the image and starting Codex 0.166.0, instructions loaded, A1 login unchanged.

### P11.2 Rewrite The Architect

- **Repo:** jackin-the-architect. **Owner:** `agent-sandbox-engineer`. **Depends:** P8.2.
- **Steps:** Apply D20. Delete `jackin-toolchain/`, `scripts/update-jackin-toolchain.rs`, `AGENTS.md.d/`, `docs/pr-workflow.md`, `caveman-config.json`, and every Claude file. Install the D02 Rust toolchain and mr-boxington in the image. Write the policy for crates.io, GitHub, and the mise download hosts.
- **Proof:** G-ROLE passes. In a sandbox of this role, G-FAST passes on a clone of jackin. Record it in `evidence/P11.2-architect.md`.
- [x] P11.2 done. Proof: jackin-the-architect feature/openshell-native fe88f07 (role ffc8735: Codex role, locked toolchain under root-owned /usr/local with jackin's pins, egress crates.io read-only and github.com git fetch only); G-ROLE exit 0 (`jackin role check .`, `mise run test`); live G-FAST in a sandbox of this role on jackin 24d77b490: exit 0, 1215/1215 passed; code-reviewer fixed, security-reviewer APPROVE; evidence/P11.2-architect.md.

## Phase P12. Rewrite the documents

### P12.1 Rewrite the jackin documents

- **Repo:** jackin. **Owner:** `docs-writer`. **Depends:** P9.5.
- **Scope:** `README.md`, `CONTRIBUTING.md`, `COMMITS.md`, `docs/content/`.
- **Steps:** Write the install guide, the gateway how-to from `rewrite/drafts/gateway.md`, the role author guide, the CLI reference, and the console guide. Mark ADR-005, ADR-007, and ADR-011 as superseded by DESIGN.md.
- **Proof:** the docs build in G-FULL passes. No page mentions a Docker or Apple runtime for agents.
- [x] P12.1 done. Proof: install guide, gateway how-to (with the 0.1.7 upgrade section), role author guide, generated CLI reference, console guide from the P9.5 live captures, README, CONTRIBUTING, COMMITS; ADR-005, ADR-007, ADR-011 marked superseded; the landing page, diagrams, og image and brand page describe jackin on OpenShell; the stale docs/PULL_REQUEST_TEMPLATE.md is deleted; G-FULL docs step exit 0 (99 static routes), docs bun test 19 pass, `rg -i -w 'dind|bollard' docs/src` prints nothing.

### P12.2 Rewrite the role documents and the project note

- **Owner:** `docs-writer` for the role READMEs. The orchestrator updates `~/.claude/project-notes/jackin.md` and the index row for the three role repositories. **Depends:** P11.1, P11.2.
- **Proof:** the diffs.
- [x] P12.2 done. Proof: role READMEs rewritten and pushed (jackin-sentinel 1e3c5c1, jackin-agent-smith 79f5e13, jackin-the-architect f9ff481; each repo's `mise run test` and `jackin role check .` exit 0); `~/.claude/project-notes/jackin.md` updated (console git dependencies, full G-E2E command, backlog and red/green records, signed merges); the index row in `~/.claude/project-notes/README.md` already covers the three role repositories.

## Phase P13. Finish and open the pull requests

### P13.0 Look for newer stable releases

- **Owner:** `docs-researcher`. **Depends:** all tasks of P0 to P12.
- **Steps:** Do the P8.0 steps again.
- **Proof:** a trail row with the versions found.
- [x] P13.0 done. Proof: /private/tmp/claude-501/plans/P13.0-releases.md (checked 2026-11-04) and decisions rows 2026-11-08T23:30 and 2026-11-09T10:30: OpenShell 0.1.7, velnor-actions 0.1.73, Docker CLI 29.9.3, buildx 0.38.2, russh 0.65.1, toml 1.2.0, mr-boxington 1.29.2, uv 0.13.7, prek 0.5.10, mise 2026.11.0, rustup 1.29.3, jactionlint 2.4.0 landed in every repo (jackin ad7d09824, sentinel f8a254d, agent-smith e852386, architect f040d07, baseline 916de2c); reqwest 0.12 and noyalib 0.0.53 held by OpenShell 0.1.7's requirements; termpane held at its git rev until 0.1.1 is published.

### P13.1 Run the full gate

- **Owner:** `verifier`. **Depends:** P13.0 and the tasks that it added.
- **Steps:** Run G-FULL. Run `rg -l -i -w 'bollard|dind' --glob '!rewrite/**' --glob '!docs/content/**/adr*/**' --glob '!deny.toml'` and verify that it prints nothing (`deny.toml` bans the `bollard` crates, which is the D01 rule itself) (`capsule` is no longer forbidden: D28 makes Jackin Capsule the host multiplexer).
- **Proof:** G-FULL passes on the head of jackin. G-ROLE passes in each role repository. The full baseline suite passes in terminal-components-baseline, and every approved baseline file that differs from `95bbcbb` is a re-take or addition recorded with its UX review in `docs/P9.3-red-green.md`.
- [x] P13.1 done. Proof: G-FULL (`xtask ci`) passed on jackin e0776e988 (1937 of 1937 tests, then deny, shear, comments, layers, pins, containers, errors, baseline, screen, demo-gate, reuse and docs; exit 0; log /private/tmp/claude-501/plans/gfull.log); the `rg -l -i -w 'bollard|dind'` check printed nothing; G-ROLE (`jackin role check .`) exit 0 in jackin-sentinel 96c530a, jackin-agent-smith e852386 and jackin-the-architect f040d07; the full baseline suite passed in terminal-components-baseline a4e81a4 (nextest 776 of 776, clippy, fmt; log /private/tmp/claude-501/plans/baseline-final.log); since `95bbcbb` no approved baseline file is modified (9 renames with identical content, 56 additions, each recorded with its UX review in `docs/P9.3-red-green.md` or, for the five P9.2 new screens, in `docs/specs/jackin-console-new-screens-review.md` and `rewrite/evidence/P9.2-baseline.md`).

### P13.2 Repeat the journey from fresh resources

- **Owner:** orchestrator runs `pstack:swarm` with three `verifier` lanes, as in P8.3. Each lane starts with an empty image cache. **Depends:** P13.1.
- **Proof:** three PASS outputs in `evidence/P13.2-lanes.md` and in the chat.
- [x] P13.2 done. Proof: `rewrite/evidence/P13.2-lanes.md`: three lanes from fresh GitHub clones of jackin `e4fd71f18` and jackin-sentinel `ad4776d` ran G-E2E one after the other on the real gateway `jackin-local` (OpenShell 0.1.7); each lane gave J00 to J17 and CLEAN, 19 PASS, 0 FAIL, 0 SKIP, with no sandbox before the lane, the A1 host login unchanged and no token-shaped output. The commits after `e4fd71f18` change only scanner baselines and documents.

### P13.3 Review the whole change

- **Owner:** one `code-reviewer` for each repository on `origin/main..feature/openshell-native`. One `security-reviewer` across all five. Then the orchestrator runs `pstack:interrogate` on the credential, policy, and image diffs. **Depends:** P13.2.
- **Steps:** Fix each accepted finding through the code task pipeline. Write the evidence for each rejected finding.
- **Proof:** every verdict in the chat. No open blocking finding.
- [x] P13.3 done. Proof: `rewrite/evidence/P13.3-reviews.md` gives every verdict: code review APPROVE for the three roles and CHANGES then fixed for jackin (`bda1988fd`) and the baseline (`a4e81a4`); security review APPROVE for the three roles and CHANGES then fixed for jackin and the baseline (feed guard `a4e81a4`); interrogate A1 HIGH, A2, C1, C2 and C3 MEDIUM fixed (`3a038b688`, `de31dcf57`, `c8c07a600`, `d80a66fb0`) and the policy adversary found nothing. Four PR review rounds followed, every thread resolved. No blocking finding is open, and the D27 identity gate passed in G-FULL on the jackin head `81b1a8dfb`.

### P13.4 Open the pull requests

- **Owner:** orchestrator. **Depends:** P13.3.
- **Steps:** Open one ready pull request for each repository from `feature/openshell-native` to `main`. The title is `feat!: rebuild jackin on OpenShell`, or the equivalent for the role or the baseline. The body opens with the goal and names the evidence files. After all five exist, edit each body to link the other four. Do not merge (A3).
- **Proof:** the output of `gh pr view <url> --json url,state,isDraft,baseRefName,headRefName` for each of the five pull requests, in the chat.
- [x] P13.4 done. Proof: each of the five pull requests is open and ready, from `feature/openshell-native` to `main`, and its body opens with the goal, names the evidence files and links the other four (edited 2026-11-04). `gh pr view <url> --json url,state,isDraft,baseRefName,headRefName`:
  - `{"baseRefName":"main","headRefName":"feature/openshell-native","isDraft":false,"state":"OPEN","url":"https://github.com/jackin-project/jackin/pull/1264"}`
  - `{"baseRefName":"main","headRefName":"feature/openshell-native","isDraft":false,"state":"OPEN","url":"https://github.com/jackin-project/jackin-sentinel/pull/158"}`
  - `{"baseRefName":"main","headRefName":"feature/openshell-native","isDraft":false,"state":"OPEN","url":"https://github.com/jackin-project/jackin-agent-smith/pull/217"}`
  - `{"baseRefName":"main","headRefName":"feature/openshell-native","isDraft":false,"state":"OPEN","url":"https://github.com/jackin-project/jackin-the-architect/pull/481"}`
  - `{"baseRefName":"main","headRefName":"feature/openshell-native","isDraft":false,"state":"OPEN","url":"https://github.com/tailrocks/terminal-components-baseline/pull/1"}`
