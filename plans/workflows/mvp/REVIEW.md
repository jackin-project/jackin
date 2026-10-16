# Review decisions

## Basis

The attached architecture review is the primary input.
It reports a source review from 14 October 2026.
It does not report a local build or real-agent test.
This update also checks selected current repository files.
See [SOURCES.md](SOURCES.md) for the exact source commit and limits.

## Decisions

| ID | Review issue | Decision and required action | Step |
|---|---|---|---|
| R1 | Keep Jackin as the host platform. | Accept. Reuse runtime, Capsule, credentials, isolation, and the host daemon. Add no separate agent platform. | All |
| R2 | The README links to omitted research files. | Partly obsolete. The current README no longer requires those files. Keep useful findings here. Do not restore the full research archive. | 0 |
| R3 | Active instructions name removed policy documents. | Accept. Replace the remaining active `TESTING.md` instruction. Use current `AGENTS.md`, tool configuration, and CI inputs. | 0 |
| R4 | Real-session operation is not proven. | Accept. Prove initial input, turn observation, follow-up input, and original-session attachment before the runner. | 1 |
| R5 | Subscription credentials can fail inside a restricted worker. | Accept with reduced scope. Test the selected authentication mode for each of two agents on one host profile. | 1 |
| R6 | A consistent checkpoint is difficult while the TUI remains alive. | Accept. Test brief Docker pause, safe source capture, and resume first. Do not require continuous output during capture. | 2 |
| R7 | A useful complete workflow comes too late. | Accept. Use one fixed sequence and eight build steps. Do not complete all former work packages first. | 0–7 |
| R8 | Vendor subagents can create uncontrolled work. | Accept. Require an explicit delegation mode. Enforce worker limits. Reject an unsupported strict prohibition. | 1, 2 |
| R9 | Remote checkpoint backup comes too late. | Accept. Add minimal exact-ref backup with checkpoint capture. Keep backup authority separate from acceptance. | 2 |
| R10 | Debug history lacks lineage and failure details. | Accept with reduced scope. Record available child links, denials, resource failures, and provider failures. Label missing observations. | 1, 6 |
| R11 | Independent clones can be expensive. | Defer optimization. Keep private mutable state now. Do not add a shared writable object cache. | Later |
| R12 | Internal breaking-change policy can conflict with manual behavior. | Clarify. Internal APIs can change. Documented manual behavior needs a separate approved migration before it can change. | 0, 6 |
| R13 | The first milestone needs real repair, attachment, and interruption recovery. | Accept. Make these parts of the final live demonstration. | 7 |
| R14 | Full editor and platform coverage are not prerequisites. | Accept. Defer visual authoring, other workflow adapters, distributed scheduling, and additional sandbox backends. | Later |

## Extra findings from this update

### The current source is newer than the review

The current source check uses `0e11cf80957365317cdf48efe4dc5eea239357d7`.
The earlier source audit used `453bf611fd974ff88f18caa97aa225af1f8edb2c`.
The current README describes the research files as outside its build contract.
The review's missing-file list must not become an instruction to restore obsolete documents.

The active build sequence still names `TESTING.md` for container test preparation.
The inspected `.github/ci/project.toml` path returned no file.
The retained `.velnor/config.toml` names `mbx` and `cargo_nextest` as the Rust drivers.
Step 0 must verify the actual test path rather than copy an old runbook.

### CI discovery does not cover every relevant leaf crate

The current `.velnor/config.toml` excludes several runtime leaf crates from automatic discovery.
These include the Docker profile and host daemon leaves.
Run focused tests for each changed leaf.
Add only the required CI entry. Do not redesign CI as part of Workflows.

### A short goal prompt is necessary

The current Claude Code documentation limits the goal condition to 4,000 characters.
`GOAL.md` stays below this limit and refers to the detailed files.
The short command does not replace the detailed implementation contract.

### Do not promise more security than the MVP supplies

A disk-use monitor is not a hard quota.
An IP allowlist is not a URL allowlist.
A process limit is not a logical subagent limit.
A PTY receipt is not proof of prompt processing.
Each stronger requested guarantee must either have enforcement or fail admission.

## Changes to the earlier plan

The earlier plan remains useful as a later product design.
Do not execute its forty work packages as the MVP backlog.
This pack combines overlapping work into seven components.
It uses one fixed workflow and one final live proof.
It retains the safety rules that protect session identity, source integrity, credentials, and incomplete work.

## Not claimed

This update does not prove that Jackin already implements the MVP.
It does not certify a sandbox, an authentication profile, an agent adapter, or a Pkl evaluator.
It defines the work and the minimum evidence required to make those claims.
