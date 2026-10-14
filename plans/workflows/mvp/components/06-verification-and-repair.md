# C6: tests, review, and same-session repair

## Purpose

Check an exact checkpoint independently of its implementer.
Return real failure findings to the same healthy implementation session.
Accept code only from current evidence.

## API

| Operation | Input | Output |
|---|---|---|
| `run_tests` | Checkpoint, frozen test policy, operation ID | `TestResult` and evidence references. |
| `run_review` | Checkpoint, task inputs, test results, reviewer selection | `ReviewResult` and evidence references. |
| `build_feedback` | Current candidate and relevant failures | One bounded feedback document. |
| `decide_candidate` | Current candidate, complete evidence, round budget | Accept eligibility, repair, fail, or operator action. |

`TestResult` contains candidate ID, commit, tree, policy hash, command ID, exit status, and bounded output references.
The host command runner records the actual exit status.
Do not accept an agent-written JSON file as proof that the test command ran.

`ReviewResult` contains candidate ID, commit, tree, reviewer session key, status, and findings.
Each finding contains its ID, severity, source path, optional line range, explanation, and suggested corrective action.
The host adds trusted source and session metadata after it validates the result.

## Test behavior

Prepare a fresh test worker from the immutable checkpoint.
The implementer cannot write that worker or its test policy.
Mount the trusted test bundle read-only outside the implementation source.
Use exact argv from the frozen policy, not an agent-generated shell command.
Execute all candidate code inside the test sandbox.

The test worker may use its own writable build directory.
It must not share writable build caches or Git state with the implementer.
Verify that its initial source matches the checkpoint manifest.
Do not reuse a modified test workspace as review source.

The authoritative tests come from the frozen trusted bundle.
New tests in the candidate can provide additional evidence.
They cannot replace or edit the required trusted checks.
A changed test policy requires a new plan and a new verification result.

Record test failures separately from unavailable tools, lost workers, and timeouts.
A test infrastructure error does not produce a passing result.
Do not retry code failures to obtain a green result.
Permit at most one clean retry for a recorded infrastructure failure in a round.
Persist that counter.

## Independent review

Start Codex in a separate instance and fresh session.
Give it the exact checkpoint, the task, the trusted test results, and the review schema.
Do not give it the implementer's conversation, private memory, or writable repository.
The source is read-only. A separate result directory can be writable.

Require one JSON review result through the adapter's tested output path.
Do not scrape an arbitrary JSON fragment from a long terminal transcript.
Validate the file or message size, schema, candidate identity, and permitted paths.
A missing or malformed review blocks acceptance.
Allow at most one correction request for malformed review output in that review session.
Persist the correction count. Further failure requires operator action.

Use these severities: `blocker`, `major`, `minor`, and `note`.
For the MVP, any `blocker`, `major`, or `minor` finding prevents acceptance.
A `note` does not prevent acceptance.
A reviewer cannot grant permissions or remove a required test.

## Feedback behavior

Build one feedback document for the current round.
Include the candidate ID, exact revision, failing checks, findings, evidence references, and remaining repair count.
Use only results from that candidate.
Do not send an outdated finding as an unqualified instruction for newer code.

Wait until the implementation turn has ended.
Confirm that no operator owns the input lease.
Save the feedback command through C5.
Submit it to the same physical implementation session through C2.
Do not start a replacement session when continuation is supported and healthy.

Increase the round only when a repair turn is authorized.
The maximum repair count is two unless the plan selects a lower value.
Manual source changes and late source changes cannot create an unlimited verification loop.
They consume a remaining round or require a new operator decision.

## Acceptance rule

The following conditions must all be true:

1. All required tests passed against the candidate.
2. The independent review passed against the same candidate.
3. The evidence uses the current test-policy hash.
4. The candidate remains current in the run record.
5. C4 confirms that the current source matches the candidate.
6. The host owns automatic input and the current candidate generation.
7. No unresolved delivery uncertainty or operator intervention blocks the decision.
8. Required backup, when selected, has completed.

Save the accepted candidate and evidence references atomically through C5.
Acceptance is a statement about that immutable checkpoint.
It does not permit a later source edit, PR creation, or merge.

## Build instructions

1. Add the trusted test runner over the existing command and runtime services.
2. Add the closed review schema and one Codex output adapter.
3. Bind results to checkpoint and policy IDs.
4. Add the bounded feedback function.
5. Add the acceptance predicate to the fixed reducer.
6. Reject stale results and exhausted repair rounds.

## Local verification

Run K5 as a table-driven test group.
Use pass, fail, exhausted budget, malformed review, and changed-candidate rows.
Run the actual Claude-to-Codex repair sequence in K7.
Do not claim that these checks prove all future agent behavior.

## Dependencies

C1 supplies the frozen test policy. C2 provides real sessions.
C3 isolates workers. C4 supplies checkpoints. C5 saves results and decisions.
