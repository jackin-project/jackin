# A8 console and roles: evidence

## Baseline pull request

- [PR 4](https://github.com/tailrocks/terminal-components-baseline/pull/4) merged to baseline `main` as `f539d9a61b1fe1e9fab13f4e976c872b09b8fb9a`.
- A8 commits on that PR: `bdc4473` (Picker multi-select), `ce37e34` (tuple launch, multi-agent manager picker, per-instance panes and task panel), `08ef3db` (workflow regeneration), and one unused-import fix.
- Required checks passed. The SonarCloud check fails because the SonarCloud project does not exist; it failed the same way before the A8 commits and is not a code result.
- Jackin pins the baseline at the merge commit `f539d9a` (Cargo.toml, both `jackin-console` and `junie-tui`).

## Console behavior

- The manager launch form lists one row per agent that a role declares. Supported tuples can be checked; unavailable and blocked-vendor tuples show disabled with the catalog reason.
- More than one agent can be checked. The picker runs once per checked agent, in check order. The lead is the first checked agent.
- The console opens one pane per running agent instance. The pane title holds the agent label.
- The task panel shows the task text and one status per agent instance. It reads `/sandbox/.jackin/task/task.md` and `/sandbox/.jackin/task/status/<agent>.md` through the gateway.
- Identity gate: the two new baseline journeys run in jackin and match the approved captures: `functional/jackin_manager_multi_launch`, `functional/jackin_terminal_task_panel`. All other captures are byte-identical.

## Roles

- `jackin-sentinel` commit `55d2d14`, `jackin-agent-smith` commit `7d6e09c`, `jackin-the-architect` commit `b021144`: each declares `[agents.codex]` and `[agents.omp]` with one instruction file per agent under `instructions/`.
- `jackin role check .` passes in each role.
- A role without a mapping for a selected agent fails the launch plan with `ManifestError::NoInstructionMapping`. A role cannot declare an agent that has no supported tuple.

## Gates

- Baseline: full workspace gate green by the implementer (1023 tests, clippy, fmt); CI required checks green on PR 4.
- Jackin: `mise exec -- mbx +1.99.1 xtask ci --fast` green at the A8 head (last line: REUSE compliance). Full gate runs again in A9.
