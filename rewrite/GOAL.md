# Goal prompt

To start the program, open Claude Code in the jackin repository. Use Opus 5.5 at effort `high` and a permission mode without prompts. Run these commands:

```sh
cd /Users/donbeave/Projects/jackin-project/jackin
claude --model opus --permission-mode bypassPermissions
```

In the session, run `/effort high`. Then paste the command below. The condition has less than 4000 characters, which is the `/goal` limit. [PLAN.md](PLAN.md) holds the durable instructions, so a context compaction does not delete them.

```text
/goal You are the orchestrator of the jackin OpenShell rewrite. Read /Users/donbeave/Projects/jackin-project/jackin/rewrite/README.md, DESIGN.md, PLAN.md, and the last rows of decisions.tsv. Obey them. Work on the branch feature/openshell-native in the five repositories under /Users/donbeave/Projects/jackin-project. They are jackin, jackin-sentinel, jackin-agent-smith, jackin-the-architect, and terminal-components-baseline. Do jackin and jackin-sentinel first. The jackin console must look identical to terminal-components-baseline (decision D27). A baseline change must never change an approved baseline visual.

Run silently. Never ask a human. Never use AskUserQuestion. Never stop to wait for a reply. If you are not sure, start analysis subagents on different AI models. Decide from their evidence, write a row in rewrite/decisions.tsv, and continue. Follow the Autonomy section of PLAN.md.

Do not write product code yourself. Give each task to the subagent that PLAN.md names. Write a brief that obeys the brief contract in ~/.claude/CLAUDE.md. Run independent tasks in parallel. Give each parallel writer its own git worktree. Use the pstack skills at the points that PLAN.md names. Use the latest stable version of every tool. Merge verified work into feature/openshell-native. Commit with git commit -s as Alexey Zhokhov <alexey@zhokhov.com>. Push after each commit.

After each task, mark its box in PLAN.md and add a trail row. Paste into this chat the task ID, the commit SHA, the last line of each gate, and each reviewer verdict. If OpenShell lacks a capability, write a blocker report in rewrite/blockers/ and continue with independent tasks. Jackin builds images with BuildKit. Never run an agent, a sandbox, or a container with Docker. Never add a host fallback or a weaker verification.

The goal is met only when all of these are true and the proof is in this chat:
1. Every task box in rewrite/PLAN.md is marked with its proof. A task can stay BLOCKED only if it has a blocker report and an independent reviewer subagent has confirmed in this chat that no way inside DESIGN.md exists.
2. The G-FULL gate of PLAN.md passes on the head of jackin feature/openshell-native. Its output is in this chat.
3. The G-E2E gate runs the sentinel journey against a real OpenShell gateway. Every journey step from J00 to J17 prints PASS. The output is in this chat. J15 can be SKIP only when task P0.5 is BLOCKED under rule 1. Three independent swarm lanes repeat the journey with PASS (task P13.2).
4. The final @"tailrocks-code-reviewer (agent)" and @"tailrocks-security-reviewer (agent)" verdicts for all five repositories have no open blocking finding. The D27 identity gate passes.
5. One open pull request exists for each of the five repositories from feature/openshell-native to main. The output of gh pr view --json url,state,isDraft,baseRefName,headRefName for each one is in this chat. Do not merge.
After 1500 turns, if the goal is not met, stop and report the state.
```

## After a stop

The `/goal` evaluator clears the goal after an authentication failure, an empty credit balance, a context overflow, or an unavailable model. To continue, paste the same `/goal` command again. The orchestrator finds the first unfinished task from the marked boxes in PLAN.md and the trail.
