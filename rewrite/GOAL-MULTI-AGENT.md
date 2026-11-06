# Goal: add all agents to jackin on OpenShell

Status: instruction for the executor. This file does not change the design.
The orchestrator changes the design in task A0. No other task changes the design.

## The /goal command (paste this)

```text
/goal Work in /Users/donbeave/Projects/jackin-project/jackin on branch feature/openshell-native. Read AGENTS.md, rewrite/DESIGN.md, rewrite/PLAN.md, rewrite/GOAL-MULTI-AGENT.md, and rewrite/research/agent-provider.md. Obey them. GOAL-MULTI-AGENT.md holds the durable task list A0 to A9, task A3M, and the 12 tuples T01 to T12. Do the tasks in order. A0 first. One sandbox must run many agents at the same time on one task, issue, or goal, each with its own process, terminal, home, provider, and branch. Prove that model in A3M before you build on it. Never break D10: the gateway holds each real credential, and a sandbox holds only a placeholder. Never run an agent or a sandbox with Docker, Podman, or Apple container. Only jackin-image starts docker, and only with buildx. Run silently. Never ask a human. If you are not sure, read the code and the research, decide, and record the decision in rewrite/decisions.tsv. Give each code task to a subagent with a brief. Run independent tasks in parallel, each writer in its own worktree. Before a test, get its plan approved by @"tailrocks-test-reviewer (agent)", and keep it only after red-then-green proof. Run gates from GOAL-MULTI-AGENT.md: G-FAST after each unit, G-FULL at the end of each task, G-E2E on a real gateway for each tuple you mark supported. Mark a tuple supported only with live evidence that the subscription paid and no paid fallback can occur. If a tuple is blocked, set it unavailable with one reason, write rewrite/blockers/, and go to the next tuple. Never weaken D10 and never add a fallback or a shim. Commit with git commit -s as Alexey Zhokhov <alexey@zhokhov.com> and push after each commit. Merge at once each pull request you create after its gates and reviews pass; never end a session with your own pull request open. Do not merge PR 1264; the operator merges it. The goal is met only when GOAL-MULTI-AGENT.md Definition of done is true and you paste in this chat: each A task with its commit SHA, the last line of G-FULL, one evidence file per supported tuple, and both final reviewer verdicts with no open blocking finding.
```

## STE rule for this file

Use short sentences. Use one instruction in each sentence.
Use the present tense. Use the active voice.

## Product in one paragraph

Jackin is a Rust CLI and a terminal console.
Jackin builds role images with Docker BuildKit.
Jackin runs one or more AI coding agents in one NVIDIA OpenShell sandbox.
The agents in one sandbox work on the same task, issue, or goal.
Each agent keeps its own process, terminal, state, account, and work branch.
See the section `Many agents in one sandbox`.
OpenShell is the only execution engine.
The gateway holds the real credential.
The sandbox holds only a placeholder.
Jackin never writes a real token to a sandbox, an image, a log, or a label.
This rule is decision D10 in [DESIGN.md](DESIGN.md). Do not break this rule.

## Current state

Read these facts before you start work.

1. The branch is `feature/openshell-native`.
2. Pull request [PR 1264](https://github.com/jackin-project/jackin/pull/1264) holds this branch.
3. Decision D04 permits only one agent: OpenAI Codex.
4. The file `crates/jackin-core/src/agent.rs` defines `enum Agent { Codex }`.
5. The same file defines one `AgentSpec` table for Codex.
6. The file `crates/jackin-core/src/provider.rs` defines `enum Provider`.
7. The `Provider` enum has `Claude`, `Codex`, `Grok`, `Kimi`, `Minimax`, `Muse`, `Antigravity`, and `Zai`.
8. The `Provider` enum has no `Mimo` variant. Add it in task A2.
9. Research exists in `rewrite/research/agent-provider.md`.
10. Read that file before you write code. It holds the evidence for each pair.
11. Research exists in `rewrite/research/usage-reporting.md`.
12. Usage is not in scope. Do not change usage behavior in this program.

## Target: 12 tuples

A tuple is one agent, one provider, one auth method, and one platform.
Support only these 12 tuples. Reject all other tuples with a typed error.

| No. | Agent (CLI only) | Provider | First auth method to prove |
|---|---|---|---|
| T01 | Codex (`codex`) | OpenAI | ChatGPT login, gateway refresh. This tuple works now. Keep it. |
| T02 | Claude Code (`claude`) | Anthropic | Host Claude login. See the Claude block rule below. |
| T03 | Grok Build (`grok`) | xAI | Host Grok login from `~/.grok/auth.json`. |
| T04 | Muse Code (`muse`) | Meta (Facebook) | Muse subscription login from onboarding. |
| T05 | Antigravity (`agy`) | Google Gemini | Google account login. See the Google block rule below. |
| T06 | MiniMax Code (verify binary name) | MiniMax | MiniMax Code OAuth login, then plan key if OAuth fails. |
| T07 | MiMo Code (`mimo`) | MiMo (Xiaomi) | Token Plan key, then MiMo Code login if key fails. |
| T08 | Kimi Code (`kimi`) | Kimi (Moonshot) | Kimi Code membership key, then device login if key fails. |
| T09 | omp (`omp`) | Z.AI | Z.AI coding plan key. |
| T10 | omp (`omp`) | Kimi | Kimi Code membership key. |
| T11 | omp (`omp`) | MiniMax | MiniMax coding plan key. Use `minimax-code`, not generic MiniMax. |
| T12 | omp (`omp`) | MiMo | MiMo Token Plan key. Use the region token-plan provider, not generic Xiaomi. |

Do not add OpenCode in this program. The operator did not list OpenCode.
Do not add Cursor, Amp, Hermes, OpenRouter, or Gemini CLI in this program.

## Many agents in one sandbox

One sandbox holds many agents. The agents work on one task, one issue, or one goal.
This is a core product rule. Do not build a one-agent sandbox and add this rule later.

### Terms

1. A **sandbox launch** is one workspace, one role, one gateway, and one task.
2. An **agent instance** is one agent, one provider, one account, and one model in that sandbox.
3. The **lead instance** is the first instance in the launch list.
4. A launch has at least one instance. A launch has at most one instance of each agent type.
5. Two instances of the same agent type in one sandbox are not in scope. Reject that launch.
6. Each instance has its own process, terminal stream, state directory, home directory, provider, and branch.
7. All instances share one task text, one uploaded project, and one sandbox network policy.
8. The union of all instance provider hosts forms the sandbox policy. Apply the D10 host closure rule to the union.

### Task input

1. The operator gives the task in one of three forms: `--task <text>`, `--task-file <host file>`, or `--issue <url>`.
2. For `--issue`, jackin reads only the issue title and body through `gh issue view <url> --json title,body`. Jackin writes no comment to the issue.
3. Jackin writes the task text to `/sandbox/.jackin/task/task.md` in the sandbox. The file has mode `0444`.
4. Jackin gives the same task text to each instance at first start. Each instance also gets its instance name and the list of all instance names.
5. Jackin creates `/sandbox/.jackin/task/status/`. Each instance may write only its own file, `status/<agent>.md`. Each instance may read all files in that directory.
6. Jackin has no chat bus between instances in this program. Coordination uses the task file, the status files, and git branches only.

### Work separation

Agents that edit one working tree at the same time corrupt each other. Prevent this by structure, not by instruction alone.

1. The lead instance works in `/sandbox/project`.
2. Each other instance works in `/sandbox/work/<agent>/project`.
3. Each other work directory is a git linked worktree of `/sandbox/project`, on branch `agent/<agent>`.
4. Create each worktree from the uploaded HEAD before the instance starts. If HEAD is unborn, the instance works in an empty directory and the lead exports its files through the normal export check.
5. Each instance sets its own agent home. Example: `/sandbox/.jackin/homes/<agent>/`. No instance reads or writes the home of another instance.
6. `jackin export` must export the lead branch and each `agent/<agent>` branch. Use the existing linked-worktree export rules in D08.
7. The lead instance integrates the other branches only when the operator runs `jackin integrate <sandbox>`. That command merges each `agent/<agent>` branch into the lead branch in the sandbox and stops on the first conflict. It never resolves a conflict by itself.
8. Do not add an auto-merge, an auto-rebase, or a file lock daemon in this program.

### Process and terminal model: prove it first

The current design has one problem. `jackin-start` runs one agent on the one main PTY.
Decision D09 says the main process must never exit. Decision D28 puts the multiplexer on the host.
Do not guess the multi-process design. Prove it in task A3M before you write product code for it.

The proof must select one of these two routes with live evidence:

1. Route 1, supervisor route. `jackin-start` keeps one control directory per instance at `/sandbox/.jackin/run/<agent>/`. It starts, stops, and restarts each instance alone. Each instance writes its output to its own stream that the host can attach to without mixing with another instance.
2. Route 2, exec route. The main process only keeps the sandbox alive. Jackin starts each instance with `ExecSandboxInteractive` and one PTY per instance. The jackin host process holds each stream. Prove that detach in the console does not kill the instance, or state that limit in the design and in the console.

Select Route 1 if both routes work. It survives a host process exit.
Record the selected route, the rejected route, and the evidence in `rewrite/decisions.tsv` and in `rewrite/evidence/A3M-multi-agent.md`.
If neither route keeps one instance alive while another instance restarts, stop. Write a blocker file. Do not build a shared-PTY design. Mixed output from two agents in one stream is a defect, not a feature.

### Labels and retry

1. The current labels `jackin.dev/agent` and `jackin.dev/account` hold one value. Replace them.
2. Write one label `jackin.dev/agents`: the first 32 hex characters of the SHA-256 of the sorted instance list. One list item is `<agent>:<provider>:<account>:<model>`. Sort the items as text. Join them with one comma and no spaces. Hash the joined text as UTF-8 with no final newline. The lead is the first instance of the launch list as the operator gave it. Sorting changes only the hash and `agents.json`, never the lead.
3. Write the full sorted instance list, with no secret, to `/sandbox/.jackin/agents.json` at launch.
4. On retry, compare the label and `agents.json` with the planned list. If one item differs, stop with `StaleSandbox`. Change nothing.
5. Keep the other D07 labels. Keep the sandbox name rule. The name does not hold an agent name.

### CLI form for many agents

1. `--agent <name>` is repeatable. Each use adds one instance. Example: `jackin load --role sentinel --agent codex --agent omp --task-file task.md`.
2. For each instance, resolve provider, account, and model by the selection order in task A3. The order runs once per instance, not once per launch.
3. Add `--provider-for <agent>=<provider>`, `--account-for <agent>=<account>`, and `--model-for <agent>=<model>` for an explicit choice per instance.
4. Reject a repeated agent, an unknown agent, and a tuple that is not `supported`, before you build an image.
5. The image for a launch installs each selected agent binary. The cache key holds each agent version and checksum, in sorted agent order.
6. `jackin attach <sandbox>` attaches to the lead instance. `jackin attach <sandbox> --agent <name>` attaches to that instance.
7. `jackin stop` and `jackin start` act on the sandbox. `jackin agent restart <sandbox> <agent>` restarts one instance and no other instance.
8. The console shows one pane for each instance. This uses the host multiplexer of D28. Add no in-sandbox multiplexer unless Route 1 needs one inside `jackin-start` itself.

## Three auth classes

Put each tuple in one class. Do the classes in this order: S, then O, then B.
This order gives the shortest time to working agents.

### Class S: static subscription key

The vendor gives a key for the subscription plan.
The key is not a pay-as-you-go API key.
Examples: Z.AI plan key, Kimi Code membership key, MiniMax plan key, MiMo Token Plan key.
The gateway holds the key. The sandbox holds a placeholder in an environment variable or a header.
This class needs no refresh. This class is the easiest class. Do it first.

### Class O: OAuth login with refresh

The CLI logs in with a browser or a device code.
The login gives an access token and a refresh token.
The gateway holds both tokens. The gateway refreshes the access token.
The sandbox holds a placeholder auth file, as Codex does now.
Do this class second. Prove refresh for each agent. Do not copy the Codex proof to another agent.

### Class B: blocked tuple

The vendor terms or the vendor login do not permit the gateway flow yet.
Mark the tuple `unavailable` with a short reason. Show the reason in the CLI and in the console.
Do not copy a real credential to the sandbox to make a blocked tuple work.
Do not use an API key to make a blocked tuple work.
Claude Code with Anthropic starts in class B. See the Claude block rule.
Antigravity starts in class B. See the Google block rule.

### Claude block rule

Anthropic permits a person to sign in to the unmodified Claude CLI.
Anthropic restricts a third party that collects and routes Claude.ai credentials.
It is not clear that the jackin gateway flow is permitted.
Do these steps in order.

1. Ask Anthropic for written clarification. Record the answer in `rewrite/research/`.
2. Before the answer, build only the synthetic test. Use a fake credential. Use a local gateway.
3. Do not do a live Claude subscription test before the answer, unless the operator orders it in writing.
4. If Anthropic permits the flow, move T02 to class O and do the class O steps.
5. If Anthropic does not reply, keep T02 as `unavailable: vendor clarification pending`.

The host Claude login is in the macOS Keychain, or in `~/.claude/.credentials.json`.
Never read the Keychain value into a log. Never print the value.
A copied refresh token can log out the host. Prove host and sandbox refresh at the same time, or do not ship T02.

### Google block rule

Google terms for Antigravity restrict third-party tools that use Antigravity OAuth.
It is not clear that an unmodified `agy` in a private OpenShell sandbox is permitted.
Do the same five steps as the Claude block rule, with Google as the vendor.
Keep T05 as `unavailable` until Google replies or a test proves a permitted login.

## Host auth: how it must work

Do these steps for each tuple. Do not change the order.

1. The operator runs `jackin account add` on the host.
2. Jackin reads the host login, or asks for the plan key, or reads the key from 1Password with `op`.
3. Jackin creates one OpenShell provider on the gateway. The name is `jackin-<account>-<agent>`.
4. The OpenShell provider holds the real credential. The jackin config holds no secret.
5. Jackin builds the role image. The derived layer installs the agent binary from a pinned file with a checksum.
6. Jackin creates the sandbox. Jackin writes a placeholder auth file in the sandbox with mode `0600`.
7. The agent sends the placeholder to the provider host. The gateway replaces the placeholder with the real credential.
8. The agent never sees the real credential. A process that the agent starts can use the credential only on the permitted hosts.
9. On launch retry, jackin checks the sandbox labels. If one label differs, jackin stops with `StaleSandbox`. Jackin changes nothing.

Use the Codex path as the model. Read `rewrite/evidence/P0.3-codex.md` before you start.

## Repositories: check before task A0

1. The jackin repo is `/Users/donbeave/Projects/jackin-project/jackin`. Work there.
2. The baseline repo is at `/Users/donbeave/Projects/tailrocks/terminal-components-baseline` on this machine. Jackin pins it by git URL and rev in the root `Cargo.toml`. Do not use a path dependency.
3. The role repos `jackin-sentinel`, `jackin-agent-smith`, and `jackin-the-architect` are not on this machine. The plan uses `../jackin-sentinel` as a path.
4. If a role repo is absent, clone it: `git clone git@github.com:jackin-project/<repo>.git /Users/donbeave/Projects/jackin-project/<repo>`. Then run `git checkout feature/openshell-native` in it.
5. Do role work only in task A8. Do no role work before task A4 passes.

## Work order: do the tasks in this order

Do task A0 first. Then do A1, A2, A3. These three tasks unlock all tuples.
Then do the omp tasks. One omp adapter gives four tuples.
Then do the native key tasks. Then do the native OAuth tasks.
Do not start a later group before the earlier group passes its gate.

### A0. Amend the design (orchestrator only)

1. Add one row to `rewrite/decisions.tsv`. Give it the next decision ID. State: D04 and D16 change; jackin supports the 12 tuples in this file; Codex is no longer the only agent.
2. Edit decision D04 in `rewrite/DESIGN.md`. Replace the Codex-only rule with the 12-tuple catalog rule.
3. Edit decisions D07, D09, and D26 in `rewrite/DESIGN.md`. State the many-agents model of this file: one sandbox holds many instances; each instance has its own process, terminal, home, provider, and branch; the `jackin.dev/agents` label replaces the single agent and account labels. Keep the rule that the main process never exits.
4. Edit decision D16 in `rewrite/DESIGN.md`. Remove `every agent except Codex` from the delete list.
5. Edit decision D26 in `rewrite/DESIGN.md`. State: an account of a provider in the catalog can have an OpenShell provider, and one launch selects one account for each instance.
6. Edit `AGENTS.md` in this repo only if the repo rule `Codex is the only agent` blocks the work. Keep the edit to one line. Record it in the same commit.
7. Add phases A0 to A9 and A3M to `rewrite/PLAN.md`. Copy the task texts from this file.
8. Commit with `git commit -s`. Push the commit. Open no pull request for this task. Before the commit: run `git status --short`, name each file that you stage, and stop if a change is not yours and you cannot explain it. Verify the committer is Alexey Zhokhov <alexey@zhokhov.com>. The `/goal` command in the executor session is the explicit order for this commit and push.

Gate: a second person or reviewer subagent reads the diff and finds no conflict with D01, D03, D05, D10, or D27.

### A1. Prove each artifact (research, no product code)

For each agent in the target table, record these facts in `rewrite/research/agent-artifacts.md`:

1. The exact CLI name and the vendor download URL.
2. The latest stable version. Do not use a prerelease.
3. The Linux arm64 file and the Linux amd64 file.
4. The SHA-256 checksum for each file, from the vendor checksum file.
5. The command that prints the version, and its exact output.
6. The state directory, the auth file path, and the config file path in the sandbox.
7. The launch command for interactive use. The flag that turns off the agent sandbox inside OpenShell, if the agent has one.
8. The flag that turns off automatic update, if the agent has one.

If you cannot find a stable Linux file for an agent, mark that tuple `unavailable: no stable Linux artifact`. Go to the next agent.
Gate: the file holds one complete section for each of the 8 agents. A reviewer checks two URLs at random.

### A2. Generalize the catalog in jackin-core

1. Add `Mimo` to `enum Provider` in `crates/jackin-core/src/provider.rs`. Add it to `Provider::ALL` and to `as_str` as `mimo`.
2. Add the new variants to `enum Agent` in `crates/jackin-core/src/agent.rs`: `ClaudeCode`, `GrokBuild`, `MuseCode`, `Antigravity`, `MinimaxCode`, `MimoCode`, `KimiCode`, `Omp`.
3. Keep `Agent::ALL` complete. Keep each `match` on `Agent` exhaustive. The compiler must find each missing arm.
4. Change `AgentSpec`. Remove the Codex-only fields `code_mode_host` and `CodexAuthMode` from the common row. Put agent-only fields in a per-agent sub-struct.
5. Add these fields to each row: binary path, state directory, launch command, install data (version, URL template, checksums), provider, auth class (S, O, or B), placeholder file path and mode, list of provider hosts, credential environment names, and the allowed model names with one default model. A model that is not in the list of its tuple is a typed error. This rule defines the unknown-model test of step 9.
6. Add one tuple table. One row holds one tuple from the target table, its auth class, and its state: `supported`, `unavailable`, or `blocked-vendor`.
7. At this step, mark only T01 as `supported`. Mark T09 to T12 as `unavailable`. Mark T02 and T05 as `blocked-vendor`. Mark the rest as `unavailable`.
8. Change `Provider::agent()` so it does not claim one agent for one provider. Add a function that returns all tuples for one provider.
9. Write the test plan. Give the plan to @"tailrocks-test-reviewer (agent)". Wait for approval. Then write tests for: valid tuple, invalid tuple, tuple state, provider with two agents, and unknown model.
10. See each new test fail. Then make it pass. Record the red and green result in the trail.

Gate: G-FAST passes. See the Gates section.

### A3. Generalize image, policy, and launch

Do this task in three units. Finish one unit before you start the next unit.

Unit 1, image (`jackin-image` only):

1. Read the Codex install code. Find each place that names `codex`.
2. Replace the Codex name with data from the `AgentSpec` row of the selected agent.
3. The derived layer installs the agent binary at the path in the row. The scan checks that binary, not a Codex binary.
4. Add the agent version and the artifact checksums to the image cache key.
5. Keep the rule: only `jackin-image` starts `docker`, and only with `buildx`.

Unit 2, policy (`jackin-openshell` only):

1. Build one OpenShell provider profile file for each tuple. Put the files in `crates/jackin-core/src/assets/`.
2. Each profile names only the agent binary in `binaries`, only the provider hosts in `endpoints`, and one credential.
3. For class O, add the refresh block with the vendor token URL. For class S, add no refresh block.
4. Keep the D10 host closure rule. Only the generated provider rule may match a provider host. Reject a role policy that breaks this rule.

Unit 3, launch (`jackin-session` only):

1. Read `crates/jackin-session/src/plan.rs`, `accounts.rs`, `files.rs`, and `launch/run.rs`.
2. Replace each Codex-only step with a step that reads the `AgentSpec` row and the tuple row.
3. Select the account in this order: `--account` flag, workspace binding, role binding, provider default, the only account of the provider.
4. Add CLI flags to `jackin load`: `--agent`, `--provider`, `--account`, `--model`. Reject a tuple that is not `supported` before you build an image.
5. Write the placeholder auth file for the selected agent. Use mode `0600`. Write no real value.
6. Keep `jackin-start` generic. It runs the launch command from the row. It does not name Codex.

Gate: G-FAST passes. The Codex tuple still works in the hermetic tests. No test names a new agent as supported yet.

### A3M. Many agents in one sandbox

Do this task after A3. Do no task after this task before this task passes.
Use the model in the section `Many agents in one sandbox`. Do each unit in order.

Unit 0, proof (no product code):

1. On a real gateway at the pinned OpenShell version in `compatibility.lock.toml`, start one sandbox with the current image.
2. Prove Route 1 and Route 2 from that section, with two sleep processes in place of two agents.
3. For the selected route, prove these facts: two processes run at the same time; each output stream stays separate; one process restarts and the other process does not stop; the sandbox phase stays `Ready` after one process exits.
4. Write `rewrite/evidence/A3M-multi-agent.md`. Add one decision row to `rewrite/decisions.tsv`.
5. If neither route proves all four facts, write `rewrite/blockers/A3M-multi-agent.md` and stop this program. Do not invent a third route in this task.

Unit 1, core and labels (`jackin-core` only):

1. Add a type `AgentInstance` with agent, provider, account, model, and work branch.
2. Change the launch plan type to hold a list of instances. Reject an empty list and a repeated agent with a typed error.
3. Replace the labels `jackin.dev/agent` and `jackin.dev/account` with `jackin.dev/agents` as that section states. Update each reader of the old labels in this unit, or the build fails.
4. Add tests for: one instance, three instances, a repeated agent, an empty list, and a retry with a changed instance list.

Unit 2, image and policy (`jackin-image`, then `jackin-openshell`):

1. Install each selected agent binary in one derived image. Run the scan on each binary.
2. Build one OpenShell provider for each instance. Name it `jackin-<account>-<agent>`.
3. Merge the provider hosts of all instances into one policy. Apply the D10 host closure rule to the merged policy.
4. Add the sorted agent versions and checksums to the image cache key.

Unit 3, supervisor and work (`jackin-image`, then `jackin-session`):

1. Implement the route that Unit 0 selected. Keep the main process alive at all times.
2. Write `/sandbox/.jackin/task/task.md`, `/sandbox/.jackin/agents.json`, and one status directory, as that section states.
3. Create one worktree and one branch for each non-lead instance, and one home directory for each instance.
4. Add `jackin agent restart <sandbox> <agent>`, `jackin attach --agent`, and `jackin integrate <sandbox>`.
5. Test that one instance cannot read the placeholder auth file of another instance if the file modes permit that read. If it can, state this limit in the evidence file. Do not claim isolation between instances in one sandbox: the sandbox is the isolation unit, not the instance.

Unit 4, live proof:

1. Run one sandbox with Codex and one `supported` omp tuple at the same time, on one task file with two named parts.
2. Each agent must write its status file and commit on its own branch.
3. Restart the omp instance during the run. Codex must not stop.
4. Run `jackin export`. Both branches must arrive on the host.
5. Run `jackin integrate`. It must merge both branches or stop on a named conflict.
6. Record each step in `rewrite/evidence/A3M-live.md`.

Gate: G-FULL passes. Unit 4 evidence shows all five steps. A single-agent launch still passes the old sentinel journey J00 to J17.

### A4. omp with four providers (T09 to T12, class S)

Do the four tuples in this order: Z.AI, Kimi, MiniMax, MiMo.
Do all four in this one task. They share one adapter.

1. Install `omp` in the derived image from the A1 artifact data.
2. Write one config generator for omp. It writes the provider, the model, and the credential environment name. It writes no key value.
3. For each provider, add one OpenShell profile. The credential is the plan key of that provider. The hosts are the provider hosts from the vendor guide in `rewrite/research/agent-provider.md`.
4. Set the tuple state to `supported` only after the live gate for that tuple passes.
5. Test that omp cannot switch accounts by itself. Turn off automatic account rotation, or prove that only the selected account exists in the sandbox state.
6. Live gate for each tuple: run the Sentinel journey with `--agent omp --provider <provider>`. The agent must answer one prompt and use one tool. The usage or billing page of the provider must show the subscription, not credits. Record the result in `rewrite/evidence/A4-omp-<provider>.md`.
7. If the billing page shows credits or pay-as-you-go, set the tuple to `unavailable`. Write the reason. Go to the next provider.

Gate: G-E2E passes for each tuple that you mark `supported`. G-FULL passes at the end of this task.

### A5. Native key agents (T07, T08, and T06 if a plan key exists)

Do the tuples in this order: Kimi Code (T08), MiMo Code (T07), MiniMax Code (T06).

1. Install the agent binary in the derived image from the A1 artifact data.
2. Write the placeholder config for the agent. For Kimi Code, set the provider `api_key_env` name. Do not rely only on an exported variable.
3. Add the OpenShell profile with the plan key as the credential.
4. Run the live gate as in A4 step 6. Record the result in `rewrite/evidence/A5-<agent>.md`.
5. Prove that the plan cannot fall back to paid usage. For Kimi, check Extra Usage. For MiMo and MiniMax, check credit pack use. If you cannot turn the fallback off and prove it, keep the tuple `unavailable`.

Gate: G-E2E passes for each tuple that you mark `supported`.

### A6. Native OAuth agents (T03, T04, and T06 if A5 fails)

Do the tuples in this order: Grok Build (T03), Muse Code (T04), MiniMax Code OAuth (T06).

1. Write the host import command: `jackin account add <provider> --from-host-login`. It reads the host login file. It prints no secret.
2. Create the OpenShell provider with the access token, the refresh token, and the refresh block from the vendor token URL.
3. Write the placeholder auth file in the sandbox in the format that the agent reads. Test the format with a fake credential first.
4. Prove refresh: wait for expiry or force one refresh. The gateway must refresh. The sandbox file must not change.
5. Prove that host and sandbox work at the same time. Run the host CLI and the sandbox agent in one test. Both must stay logged in.
6. Prove that a second sandbox cannot use the credential of the first account.
7. Record the result in `rewrite/evidence/A6-<agent>.md`.

For Muse Code: if you cannot find a stable Linux artifact in A1, keep T04 `unavailable`. Do not install Muse Code with a floating installer script.
Gate: G-E2E passes for each tuple that you mark `supported`, with steps 4, 5, and 6 in the evidence file.

### A7. Claude Code and Antigravity (T02, T05)

Do this task only after the vendor reply in the block rules, or after the operator orders a live test in writing.

1. Follow the class O steps of A6 for the tuple.
2. For Claude Code, read the host login from the Keychain entry for the selected `CLAUDE_CONFIG_DIR`, or from `~/.claude/.credentials.json`. Read only that entry. Do not read the full Keychain.
3. Prove the refresh race test: host and sandbox refresh at the same expiry. If the host logs out, set T02 to `unavailable` and write the result. Do not ship a flow that logs out the host.
4. For Antigravity, prove that `agy` works with no desktop keyring in the sandbox, or set T05 to `unavailable` with that reason.

Gate: G-E2E passes, and the evidence file holds the vendor reply or the operator order.

### A8. Console and role support

1. The console source is in `terminal-components-baseline`. Read its `AGENTS.md` first.
2. Add the agent, provider, account, and model selectors to the baseline launch form. The form must permit more than one agent for one launch. Show `unavailable` and `blocked-vendor` tuples as disabled, with the reason.
3. Show one pane for each running instance, with the agent name in the pane title. Show the task text and each instance status file in one task panel.
4. Do not draw a new screen in `jackin-tui`. `jackin-tui` only sends data to the baseline console. This rule is D27.
5. Update the three roles (sentinel, agent-smith, the-architect) only after A4 passes. Add one instruction file for each agent that the role supports. If a role has no instruction mapping for an agent, the launch must fail with a typed error.
6. Run the D27 identity gate. The console must look the same as the approved baseline, plus the new selector states.

Gate: G-FULL passes in jackin and in the baseline. The baseline pull request is merged. See the pull request rule.

### A9. Final proof and close

1. Run G-FULL on the head of `feature/openshell-native`.
2. Run G-E2E one time for each tuple that you mark `supported`. Use a real gateway. Use a dedicated test account when you have one.
3. Run the reviewers: @"tailrocks-code-reviewer (agent)" and @"tailrocks-security-reviewer (agent)". Fix each blocking finding. Run G-FULL again after the fixes.
4. Update `rewrite/PLAN.md`: mark each A task with its commit, its gate output, and its evidence file.
5. Update the tuple states in the catalog to match the evidence. No tuple is `supported` without a live evidence file.
6. Merge each pull request that this program created. See the pull request rule.

Gate: each `supported` tuple has one live evidence file, G-FULL passes, and no pull request of this program is open.

## Pull request rule

Never keep a pull request of this program open after its gates pass.

1. When a task creates a pull request, add its URL to the trail in `rewrite/PLAN.md`.
2. Read each review and each comment on that pull request. Fix the accepted points. Reply with the fix commit URL.
3. When the gates pass and no blocking review point is open, merge the pull request at once. Use `gh pr merge <number> --merge`.
4. Do not end a session with an open pull request that you created, unless a gate fails or a blocking review point is open. In that case, write the blocker in the pull request and in the trail.
5. Pull request [PR 1264](https://github.com/jackin-project/jackin/pull/1264) is the program pull request for `feature/openshell-native`. The operator merges PR 1264. Do not merge PR 1264 in this program. This is the one exception to this rule.
6. If a rule in `AGENTS.md` conflicts with step 3, follow this file for pull requests that this program creates, and record the conflict in `rewrite/decisions.tsv` in task A0.

## Gates

Run each command from the jackin repository root.

| Gate | Command | When |
|---|---|---|
| G-FAST | `mise exec -- mbx +1.99.1 xtask ci --fast` | After each unit |
| G-FULL | `mise exec -- mbx +1.99.1 xtask ci` | At the end of each A task |
| G-E2E | `mise exec -- mbx +1.99.1 xtask e2e --gateway jackin-local --role ../jackin-sentinel --builder orbstack --platform linux/arm64 --engine-port 2376 --engine-certs ~/.local/share/jackin-e2e-engine/client` | For each tuple that you mark `supported` |

Add `--agent` and `--provider` to the G-E2E command when task A3 adds those flags.
Run cargo only through `mise exec -- mbx`. Run tests only with `cargo nextest` through the gates.
Do not use `cargo test`. Do not run an agent or a sandbox with Docker, Podman, or Apple `container`.
Only `jackin-image` may start `docker`, and only with `buildx`.

## Test rule

1. Before you write a test, give its plan to @"tailrocks-test-reviewer (agent)". The plan names the behavior, one positive case, one negative case, and the reason that no other test covers it.
2. Write the test. See it fail on the broken code. Then fix the code. See the test pass.
3. Test through public interfaces. Assert the final state. Do not assert the order of calls.
4. Use a fake only at these boundaries: the OpenShell gateway, the network, the clock, and the filesystem. Use the fakes in `jackin-testkit`.
5. Keep tests hermetic. Do live tests only through `xtask e2e`.

## Stop rules

Stop the current tuple, not the program, when one of these facts is true:

1. The vendor gives no stable Linux artifact.
2. The vendor terms do not permit the gateway flow, and no reply arrives.
3. The agent needs a real credential in the sandbox to work.
4. The plan can fall back to paid usage, and you cannot turn the fallback off and prove it.
5. The refresh test logs out the host login.

For each stopped tuple: set its state to `unavailable`, write the reason in one short sentence, write one blocker file in `rewrite/blockers/`, and go to the next tuple.
Never weaken D10 to unblock a tuple. Never add a fallback, a shim, or a compatibility path.

## Definition of done

The goal is met when all of these statements are true:

0. One sandbox runs two or more instances at the same time on one task. `rewrite/evidence/A3M-live.md` proves it. Each instance has its own branch, home, and terminal stream.
1. The catalog holds the 12 tuples. Each tuple has one state.
2. Each `supported` tuple has one live evidence file from a real gateway, with subscription billing proof.
3. Codex (T01) still passes G-E2E.
4. G-FULL passes on the head of `feature/openshell-native`.
5. The code review and the security review have no open blocking finding.
6. No pull request that this program created is open, except PR 1264.
7. `rewrite/PLAN.md` shows the commit, the gate output, and the evidence file for each A task.
