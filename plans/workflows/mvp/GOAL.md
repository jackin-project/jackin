# Goal: implement the Jackin Workflows MVP

## Use the command

Copy this pack to `plans/workflows/mvp` before you submit the command.
Start the coding-agent CLI in the Jackin checkout.
Use a build that supports `/goal` and subagents.
Submit the complete block below as one message.
Do not submit the Markdown fence marks.

```text
/goal Implement the Jackin Workflows MVP in plans/workflows/mvp/SPEC.md. Complete all eight steps in plans/workflows/mvp/PLAN.md. Prove all MVP requirements with plans/workflows/mvp/CHECKS.md. Deliver working code, a runnable Pkl example, and an accurate PROGRESS.md. Do not claim completion from documentation or mocked agents.

Read the root AGENTS.md and applicable directory rules first. Read README.md, SPEC.md, PLAN.md, CHECKS.md, and REVIEW.md in plans/workflows/mvp. Read each component file before you change that component. Check the current source and tool configuration. Do not use a deleted policy file as a current instruction.

Delegate first. Use at most two subagents at one time. Assign one subagent to build the current step. Assign another subagent to inspect the relevant interfaces or independently check the finished step. Give each subagent the step, permitted files, required result, and exact local check. Permit only one writer in a shared checkout. Give an independent verifier an exact commit in a separate read-only checkout. Do not count a builder's own report as independent verification. Do not add a new agent framework.

Move in small verified steps. Run the named local check after each step. Run only affected checks during repair. Fix the cause of a failure before the next dependent step. Commit each meaningful verified change. Push the implementation branch only under existing permission. Do not force-push, delete user work, or merge without specific permission. Run required repository checks before any authorized merge.

Prove real Claude Code and Codex sessions first. Use the existing Jackin runtime, Capsule, and credential services. Prove two prompts in one live implementation session. Prove that the operator observes that same session. Test the selected subscription or API-key profile inside the restricted worker. Do not claim one profile proves another.

Build the smallest complete product path. Use one host, one active run, one repository, one Claude implementer, and one separate Codex reviewer. Use a fixed repair sequence and at most two repair turns. Use private clones. Add only the Jackin changes needed for safe input ownership, strict permission limits, consistent checkpoints, durable state, and work preservation.

Use a brief Docker pause to test checkpoint capture first. Preserve the original processes. Do not build a snapshot platform. Return feedback only to the same healthy session. Save prompt intent before delivery. Never resend uncertain input automatically. Keep backup separate from acceptance. Reject a requested guarantee that the backend cannot enforce.

Prove the final path with real agents. Start from a known failing fixture. Capture the first checkpoint. Run trusted tests and a separate review. Send real findings to the original implementer. Verify the repaired checkpoint. Demonstrate observe, exclusive takeover, return of control, backup, and one host daemon interruption. Test uncertain delivery with the focused fault case. Keep incomplete work.

Do not build the deferred scope in SPEC.md. Do not add broad certification matrices, a visual editor, distributed scheduling, or more adapters. Preserve documented manual behavior. Use short STE sentences and the terms in TERMS.md for new MVP documentation.

Record the commit, command, result, and evidence for each completed step in PROGRESS.md. A skipped or unavailable check is not a pass. When access, credentials, or a required backend are missing, record BLOCKED and the exact missing prerequisite. Continue independent work that remains safe. Do not widen permissions or repeat the same blocked attempt. End with verified results, remaining blockers, and the exact next action. Declare MVP PASS only when every required check has evidence.
```

## Completion authority

The `/goal` evaluator is not an independent product test.
The implementer must provide the test results in the conversation and in `PROGRESS.md`.
The product runner must make its own acceptance decision from checkpoint evidence.
Do not use a vendor `/goal` loop inside product workers in the MVP.

## References

See [SOURCES.md](SOURCES.md), E1 and E2, for the current vendor command documentation.
The command is below the documented 4,000-character Claude Code goal limit.
The installed CLI must still support the command.
