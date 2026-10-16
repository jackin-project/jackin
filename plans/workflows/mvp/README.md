# Jackin Workflows: MVP build pack

**Document version:** 2.0  
**Date:** 15 October 2026  
**Status:** Final MVP build contract. The product is not implemented by this document.  
**Source check:** `jackin-project/jackin` at `0e11cf80957365317cdf48efe4dc5eea239357d7`.

MVP means minimum viable product. This pack replaces the earlier build order for the first release.
The full Workflows specification remains the later product target.
Do not treat MVP completion as completion of that full specification.

## Product result

One typed Pkl file starts one controlled repair workflow.
Claude Code implements the task in a private Jackin instance.
Trusted tests and a separate Codex instance check an exact checkpoint.
Jackin returns failures to the same live Claude Code session.
The operator can observe that session, take control, and return control.
Jackin keeps checkpoints, results, and incomplete work after a host daemon restart.

## Start the work

1. Copy this folder to `plans/workflows/mvp` in the Jackin checkout.
2. Read [SPEC.md](SPEC.md).
3. Open the checkout in a coding-agent CLI that supports `/goal` and subagents.
4. Submit the command in [GOAL.md](GOAL.md).

The prompt directs the coding agent to implement the MVP.
This pack does not execute that prompt or change the GitHub repository.
Use the configured permissions. Do not enable unrestricted access to run the prompt.

## Read these files

| File | Purpose |
|---|---|
| [GOAL.md](GOAL.md) | A paste-ready `/goal` command and the rules for subagents. |
| [SPEC.md](SPEC.md) | Required product behavior and explicit scope limits. |
| [PLAN.md](PLAN.md) | Eight small build steps with one local check per step. |
| [CHECKS.md](CHECKS.md) | The small check set and the final live demonstration. |
| [REVIEW.md](REVIEW.md) | A decision for each useful review finding. |
| [EXAMPLE.md](EXAMPLE.md) | The proposed Pkl contract and an operator procedure. |
| [TERMS.md](TERMS.md) | Fixed technical terms used in this pack. |
| [SOURCES.md](SOURCES.md) | Review references, current source findings, and external references. |
| [PROGRESS.md](PROGRESS.md) | The implementation record. All product checks start as NOT RUN. |
| [VALIDATION.md](VALIDATION.md) | Checks performed on this document pack. |

## Component files

Each component has its own purpose, API, behavior, build instructions, and local check.
These are logical components. Do not create one crate for each document.

| ID | Component |
|---|---|
| C1 | [Pkl definition and plan](components/01-definition-and-plan.md) |
| C2 | [Agent sessions and operator control](components/02-sessions-and-control.md) |
| C3 | [Sandbox and authentication](components/03-sandbox-and-auth.md) |
| C4 | [Checkpoints and backup](components/04-checkpoints-and-backup.md) |
| C5 | [Run state and recovery](components/05-run-state-and-recovery.md) |
| C6 | [Tests, review, and repair](components/06-verification-and-repair.md) |
| C7 | [Host API and operator interface](components/07-host-api-and-interface.md) |

## Work rules

Make one small change. Check that change. Fix a failed check. Commit the verified result. Continue to the next step.
Use existing Jackin services before you add code.
Run required repository checks at their required boundary.
Do not run the full test suite after every small edit.

## Document authority

Use `SPEC.md` to decide MVP scope. Use `PLAN.md` to decide build order.
Use the component files to implement the interfaces.
Use `CHECKS.md` to decide whether the MVP passes.
Use current repository policies for tool use, code changes, and required repository checks.

The task permits internal API changes. It does not permit an unapproved change to documented manual behavior.
Do not retain obsolete compatibility code only to preserve an internal API.
Keep manual launch, roles, workspaces, Console, and Capsule behavior intact.

All new interfaces and workflow commands in this pack are target interfaces.
Check the current checkout before you add them.
