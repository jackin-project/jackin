# Plan: eight small build steps

## Working method

Use one coordinator and at most two subagents.
One subagent builds the current step. Another inspects interfaces or checks a completed step.
Give each subagent a bounded task and an exact file set.
Only one agent can write to a shared checkout at one time.

Use a separate read-only checkout for independent verification of a commit.
Give the verifier a separate writable build directory and evidence directory.
Do not share a writable build cache with the builder.
Independent research can run during implementation.
Dependent implementation must wait for the required interface or proof.
The coordinator integrates changes and maintains [PROGRESS.md](PROGRESS.md).

For every step, use this sequence:

1. Read the relevant source and component contract.
2. Make the smallest complete change.
3. Run the named local check.
4. Fix a failed check.
5. Commit the verified change.
6. Record the result and start the next step.

A blocked live check is not a pass.
Continue unrelated work only when the missing proof cannot change its design.
Do not build the full scheduler before the session and checkpoint checks pass.

## Step 0: align the checkout and the documents

**Owner:** Coordinator with a read-only research subagent.  
**Depends on:** None.  
**Result:** One accurate starting point.

1. Record the current commit and existing local changes.
2. Read root and applicable directory `AGENTS.md` files.
3. Read `mise.toml`, `.velnor/config.toml`, and the current CI inputs.
4. Confirm the current database dependency and test commands.
5. Select the existing workspace, role, Claude account, and Codex account.
6. Select the local Docker backend and the test repository.
7. Record the remote backup choice and authorization.
8. Add this MVP folder to `plans/workflows`.
9. Update the Workflows README to direct MVP work here.
10. Replace active instructions that name deleted policy files.
11. Keep valid commit-pinned historical links as historical references.
12. Add one local-link check to the existing documentation check path.

Do not restore all omitted research files.
The current README already avoids those files.
Keep implementation-relevant findings in `REVIEW.md` and `SOURCES.md`.

**Verify:** K0. Check local links and run one existing affected test as the baseline.
Do not run every repository test to start a documentation change.

**Finish when:** The selected environment and current commands are recorded. No mandatory reading link is broken.

## Step 1: prove the real sessions before engine work

**Owner:** Session subagent. Independent checker observes the result.  
**Depends on:** Step 0.  
**Components:** C2 and C3.  
**Result:** One reusable real-session fixture.

1. Use existing programmatic launch to start Claude in a restricted private instance.
2. Use the selected real authentication profile.
3. Submit a small initial instruction through Capsule.
4. Record the instance, Capsule epoch, session ID, and agent process identity.
5. Observe the original TUI from a second client.
6. Detect the end of the first turn through the tested adapter.
7. Submit a second instruction to the same session.
8. Confirm that the second result comes from that same physical session.
9. Start a separate Codex instance with its selected credentials.
10. Obtain a structured review result from a small exact revision.
11. Add the smallest input-owner check needed for safe observation and takeover.
12. Reject automatic input while an operator owns the session.

Use an existing restrictive profile for this first fixture.
Move a required small grant fix from Step 2 into this step when safe launch needs it.
Record the profile used. Repeat only affected checks after the final grant changes.

Use native runtime events when available.
A tested readiness fallback can permit input. It cannot prove product acceptance.
Do not replace the interactive CLI with an unrelated headless session.

Record authentication refresh behavior when the existing service supports it.
Do not force expiry of a real account token to test refresh.
Use one fake-expiry case for the refresh or reauthentication branch.
A profile without refresh support must stop safely when its credential expires.

**Verify:** K1. Two prompts, one physical implementer session, one observer, and one separate authenticated reviewer.

**Finish when:** The selected real sessions work inside the stated restrictions.
A missing account or incompatible transport blocks this step. It does not justify open networking.

## Step 2: prove isolation, checkpoints, and early backup

**Owner:** Runtime and Git subagent.  
**Depends on:** Step 1.  
**Components:** C3 and C4.  
**Result:** A safe source checkpoint without loss of the live agent.

1. Add a final restrictive workflow grant to the runtime launch path.
2. Reuse private-clone materialization.
3. Remove writable links to the host repository and shared Git metadata.
4. Add workflow ownership labels and cleanup protection.
5. Add the brief pause, safe source copy, and resume procedure.
6. Create a commit in a separate host-owned checkpoint repository.
7. Save the checkpoint record before releasing the capture operation.
8. Add exact-ref remote backup through existing host Git and credential services.
9. Preserve local data on backup failure.
10. Test the selected network profile and enforced resource limits.

Use the existing isolation record for capture intents and preservation holds in this step.
Step 4 adds workflow database references to these records. Do not build a second temporary engine.

Use one small fixture with a child process that writes repeatedly.
Do not build a filesystem snapshot service.
Do not add cross-platform capability abstractions beyond the existing runtime boundary.

**Verify:** K2. Check denied writes, a stable checkpoint, unchanged session identity, safe backup, and cleanup protection.

**Finish when:** The selected backend provides the stated guarantees.
Report unsupported guarantees explicitly. Do not replace a hard requirement with an unmarked soft check.

## Step 3: add the smallest typed Pkl plan

**Owner:** Compiler subagent.  
**Depends on:** Steps 1 and 2.  
**Component:** C1.  
**Result:** `validate` and `plan` for the fixed repair template.

1. Add `jackin-workflow-spec` under the current core crate layout.
2. Add the typed local schema from `EXAMPLE.md`.
3. Use pinned `pklr` with explicit evaluator capabilities.
4. Permit only supplied local modules and the versioned schema.
5. Deny ambient environment reads and remote imports.
6. Resolve task files, roles, binaries, images, accounts, and repository inputs.
7. Freeze approved input bytes and selected runtime facts.
8. Validate every required capability against the Step 1 and Step 2 profiles.
9. Create canonical plan bytes and a cryptographic plan hash.
10. Add thin `jackin workflow validate` and `plan` handlers.

Use a killable evaluation helper with time and memory limits.
A Tokio timeout alone does not stop synchronous evaluator work.
Do not add a new helper daemon or general package manager.

**Verify:** K3. One valid input, invalid constraints, denied input access, and stable plan hashes.

**Finish when:** A run can reference frozen plan content without reopening mutable configuration.

## Step 4: connect one durable host run

**Owner:** Runner subagent.  
**Depends on:** Step 3.  
**Components:** C5 and C7.  
**Result:** One run survives a host daemon restart.

1. Add `jackin-workflow` under the current service crate layout.
2. Keep the database code in its `store` module initially.
3. Use the existing `turso` dependency and connection pattern.
4. Add the small run, session, command, checkpoint, evidence, event, and effect tables.
5. Add one active-run lock and a persisted owner epoch.
6. Save each effect intent before execution.
7. Add the fixed stage reducer.
8. Connect it to the existing host daemon through an injected service boundary.
9. Add `run`, `status`, `pause`, `resume`, and `cancel`.
10. Recover matching instances by exact ownership labels.
11. Mark ambiguous prompt delivery as `needs_operator`.
12. Keep work on restart, cancellation, and failure.

Do not add a queue, distributed leases, a generic plugin runner, or a separate storage crate without a dependency need.
Do not create a dependency cycle through the runtime and daemon crates.

**Verify:** K4. Reopen the database, compare live state, and exercise one uncertain-delivery fault case.

**Finish when:** The host can continue a safe run without duplicate automatic input.

## Step 5: complete tests, review, and same-session repair

**Owner:** Verification subagent.  
**Depends on:** Step 4.  
**Components:** C5 and C6.  
**Result:** One complete repair workflow.

1. Run the pinned test command in a fresh worker at the checkpoint.
2. Collect exit status and bounded output through the host command channel.
3. Run Codex in a separate review instance at the same checkpoint.
4. Validate the review result schema.
5. Bind every result to the run, round, candidate, and policy hash.
6. Create one feedback message from the relevant failures.
7. Deliver it after the implementer turn ends and automatic input ownership returns.
8. Keep the original implementer session.
9. Repeat within the fixed repair limit.
10. Check current source and input ownership before acceptance.
11. Reject stale, absent, malformed, or incomplete evidence.

Keep the test and review workers separate from the implementation worker.
A vendor subagent inside the implementer cannot replace the review worker.

**Verify:** K5. Reuse the seeded-failure fixture to prove repair and the latest-candidate rule.

**Finish when:** Valid evidence can accept one repaired checkpoint. A failed or stale check cannot accept it.

## Step 6: expose the product in Jackin

**Owner:** Interface subagent.  
**Depends on:** Step 5.  
**Component:** C7.  
**Result:** The operator can use and inspect the MVP.

1. Add `events`, `graph`, `attach`, and `export`.
2. Add explicit operator resolution for uncertain input.
3. Connect observation and takeover to the original Capsule session.
4. Add one Console workflow list and detail view.
5. Show stage, candidate, findings, backup state, and preserved work.
6. Add the original-session attach action.
7. Add a protected event history with bounded terminal text.
8. Show evidence quality for vendor events and unavailable observations.
9. Export a readable Markdown report and the protected evidence bundle.
10. Test the existing manual launch and attach path.

Do not build a browser editor or full terminal video replay.
Reuse existing Console widgets and Capsule rendering.

**Verify:** K6. One CLI test group, one Console view test, and one manual-path smoke check.

**Finish when:** The operator can control and inspect a run without a second workflow engine.

## Step 7: prove the complete MVP and stop

**Owner:** Independent verifier. Coordinator fixes any defect.  
**Depends on:** Step 6.  
**Result:** A release decision with real evidence.

1. Run K7 against the final implementation commit.
2. Use real Claude Code and Codex in the selected restricted profile.
3. Demonstrate the seeded failure and the automatic repair.
4. Demonstrate observation, exclusive takeover, and return of control.
5. Restart the host daemon during a confirmed active turn.
6. Verify the original session, saved state, and absence of duplicate input.
7. Check exact checkpoint backup through the authorized test remote.
8. Export the run and inspect its recorded evidence.
9. Run the current required repository checks for the changed code.
10. Record every result and remaining limit in `PROGRESS.md`.
11. Stop when all MVP requirements pass.

Run the existing full repository gate once at the required integration boundary.
Do not replace mandatory CI with this smaller local check set.
Do not start deferred work because the MVP is complete.

## Small-step test commands

These commands match the inspected `mise.toml` and `.velnor/config.toml` configuration.
Confirm them again in Step 0. New crate commands require the new crates to exist.

```sh
mise exec -- mbx +1.99.1 nextest run -p jackin-workflow-spec
mise exec -- mbx +1.99.1 nextest run -p jackin-workflow
mise exec -- mbx +1.99.1 nextest run -p jackin-capsule -E 'test(workflow_)'
mise exec -- mbx +1.99.1 nextest run -p jackin-isolation -E 'test(workflow_)'
mise exec -- mbx +1.99.1 nextest run -p jackin-runtime-docker-profile -E 'test(workflow_)'
```

Register new focused tests with the `workflow_` prefix where the commands use that filter.
A command that selects zero tests fails the check.
The current CI configuration excludes several runtime leaf crates from automatic discovery.
Run the changed leaf's focused tests explicitly. Add a small CI entry when required.
Do not expand this into a CI redesign.

## Subagent task format

```text
Step: <step number and name>
Base: <exact commit>
Read: <component and current source files>
Permitted changes: <exact directories or files>
Task: <one bounded result>
Local check: <exact command and required assertion>
Return: changed files, commit, check result, evidence path, and unresolved issue.
Do not change another owner's files. Do not merge or force-push.
```
