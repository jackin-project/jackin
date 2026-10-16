# Checks: the minimum useful proof

## Check policy

Use one reusable fixture and the focused checks below.
Do not build a large certification framework for the MVP.
Run each local check after its step.
Run the complete real-agent demonstration once at the final integration boundary.
Repeat only the affected check after a fix, unless the change affects the complete path.

Required repository CI still applies.
A fake agent can check state transitions. It cannot prove real session or authentication support.
A skipped check, zero selected tests, or an unavailable environment is not a pass.

## The check set

| ID | Run at | Minimum check |
|---|---|---|
| K0 | Step 0 | Check local Markdown links. Run one existing affected test. Record current source and tools. |
| K1 | Step 1 | Use real authenticated Claude and Codex. Submit two prompts to one Claude session. Observe that session. Test exclusive input ownership. |
| K2 | Step 2 | Use one Docker fixture to check private writes, final resource limits, denied network access, checkpoint capture, backup, and preservation. |
| K3 | Step 3 | Use a small table of Pkl inputs. Check one valid plan, invalid constraints, denied reads, and unchanged plan hashes. |
| K4 | Step 4 | Reopen saved state. Inject a lost prompt receipt. Confirm that recovery does not send the prompt again. |
| K5 | Step 5 | Check a failing round, one repair, a passing round, the repair limit, and rejection of stale evidence. |
| K6 | Step 6 | Check CLI responses and one Console view. Exercise manual launch and original-session attach. |
| K7 | Step 7 | Run the real repair demonstration with operator control, one host daemon restart, and checkpoint backup. |

## K2: use one sandbox fixture

Use a small repository and two isolated workers.
Attempt a write to the task mount and to the other worker's path.
Confirm that both writes fail.
Confirm that the host repository remains unchanged.
Check that no writable shared Git directory exists.

Inspect the final CPU, memory, process, and privilege settings.
Attempt one disallowed network connection.
Check all enabled IP families for the selected network profile.
Confirm that an allowed agent request still works.
The same fixture checks a child writer during capture.
The source must stop changing before the copy starts.
The sealed checkpoint must not change after the worker resumes.

Attempt cleanup while a preservation hold exists.
Confirm that the source and checkpoint remain available.
Push the checkpoint to a temporary bare Git remote with a test grant.
Repeat the same push request. Confirm that the ref still names the same commit.
Reject a conflicting ref and a ref outside the permitted prefix.
This test does not claim that GitHub account authentication works.

## K3, K4, and K5: use small table-driven tests

Use table rows instead of separate test harnesses.
K3 needs valid input, an invalid repair limit, a denied file read, and a changed input hash.
K4 needs one normal recovery row and one uncertain-delivery row.
K5 needs pass, repair, exhausted limit, malformed review, and stale-candidate rows.

Add a regression row for each defect found during implementation.
Do not add unrelated permutations before the MVP works.

## K7: final real-agent demonstration

Use an approved local fixture repository with a small known defect.
For example, its `add(2, 3)` function returns `-1` instead of `5`.
Keep the trusted test outside the implementation worker's writable files.
Use event-driven fixture controls for the restart and takeover.
Trigger the restart after the repair receipt and a working event.
Hold acceptance until the operator-control check ends.
Do not use long sleeps to keep the demonstration active.

1. Start the Pkl workflow with real Claude Code and Codex accounts.
2. Give Claude an initial inspection task that explicitly forbids source changes in the first turn.
3. Capture the known failing source after that real turn.
4. Run the trusted test and the independent Codex review.
5. Confirm that both results name the same checkpoint.
6. Deliver the recorded findings to the original Claude session.
7. Let Claude repair the defect.
8. Restart only the Jackin host daemon during a confirmed active repair turn.
9. Recover the same Capsule and agent session identities.
10. Confirm that the prompt journal contains no duplicate automatic submission.
11. Attach an observer and confirm that observation does not grant input ownership.
12. Take control with one operator client.
13. Confirm that a second input owner and automatic input are rejected.
14. Return control through the explicit resume action.
15. Capture the repaired source and run the required test and review again.
16. Accept only the exact current passing candidate.
17. Check the local checkpoint and authorized test-remote ref.
18. Export the Markdown report and inspect its evidence references.
19. Run the manual launch and attach smoke check.

The first turn deliberately preserves a known defect to exercise the repair path.
Do not claim that this measures open-ended coding quality.
Do not replace a real test failure or real review with invented findings.
A vendor interruption can make the run need operator action. Record that result accurately.

## Authentication evidence

Record one row for each advertised combination:

```text
host OS and architecture
Docker version and profile hash
agent binary version and digest
requested model and any vendor-reported model
account ID and authentication mode; no secret values
initial authenticated request result
same-session follow-up result
refresh support: tested, existing trusted path, or unsupported
network profile and evidence path
```

Subscription success does not prove API-key support, or the reverse.
The MVP requires the actual selected modes to work.
Unselected modes remain unverified.

## Release decision

Mark `MVP PASS` only when K0 through K7 have valid evidence.
K7 must use the final source commit or a later commit with only documentation changes.
A changed runtime, Capsule, or workflow path requires its affected checks again.

Record real-remote support separately from the temporary bare-remote test.
A local-only release must say `remote transport not verified` when no authorized remote test ran.
Record any hard quota or network enforcement limit from `SPEC.md`.
Do not convert missing evidence into a success claim.
