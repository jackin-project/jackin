# A01 — Claude Code workflow adapter

**Status:** Proposed Jackin workflow adapter contract. Upstream documentation is not a passed Jackin runtime certification. Shared purpose/API semantics are defined by [C05](../components/05-agent-adapters.md), [C06](../components/06-capsule-session-control.md) and [C07](../components/07-input-leases-attachment.md).

## Purpose and observed upstream surface

Use the real Claude Code CLI with the original interactive terminal retained in Capsule. Official documentation distinguishes interactive startup/session resume from print/stream modes. Print-mode structured input/output flags must not be assumed to work identically in the interactive TUI. [E04](#evidence-and-references).

## API and capability mapping

Implement C05’s `probe`, `prepare_session`, `prepare_input`, `interpret_event`, `interrupt` and `recover` operations around the existing built-in installation/auth infrastructure. The descriptor pins binary digest/version, model/provider, auth selection, mode and adapter-fixture version.

| Capability | Target behavior / certification requirement |
|---|---|
| Original interactive TUI | Capsule owns the actual interactive Claude process and PTY from startup. |
| Initial prompt | Choose one certified startup or PTY submission path; never submit through both. |
| Long instructions/attachments | Version-tested file/context or literal-paste transport; exact digest and no silent truncation. |
| Follow-up | Submit to the same healthy Capsule/vendor session, under current lease and safe turn boundary. |
| Structured events | Use supported events for provenance where available; distinguish print-mode-only surfaces. |
| Resume | Record explicit vendor session ID; certify recovery and new physical binding after a process loss. |
| Active steering / goal | Unavailable until certified for the exact interactive mode; no brand-level assumption. |
| Acceptance | Never derived from a Claude message, stop event or process exit. |

## Required functionality

Keep project instructions, user configuration, agent tools and vendor hooks inside the authorized worker boundary. Disable or constrain unapproved plugin/MCP/network/tool paths through host enforcement, not merely prompt instructions. An auth sync snapshot must not accidentally include unrelated host secrets or publishing credentials. The role/model/account must be fully resolved before non-interactive startup.

Prefer prompt file/reference delivery when a certified mode supports it and instructions are large. The file is an immutable read-only artifact; its contents and submitted reference are recorded. If PTY paste is used, treat content and submit key as one fenced transaction and test control-character safety. Unsupported permission/trust dialogs become operator-required or preflight errors, not automatic unrestricted approval.

Do not use “continue most recent conversation” as the authority when multiple sessions can exist. Bind recovery to the recorded session ID and exact working repository. Vendor conversation recovery is not recovery of a dead original process; preserve the earlier replay segment and record the new Capsule epoch.

## Build instructions

Inspect the current Claude built-in adapter and launch call sites at the implementation commit. Add workflow metadata and a certified interactive profile without duplicating installation/auth code. Add PTY fixtures for startup, prompt submission, busy/blocked states, interruption and resume. Integrate structured observations only when their session identity is demonstrably the same. Print-only execution may be a separately declared non-interactive mode, not a substitute for interactive acceptance requirements.

## Acceptance tests

`CLAUDE-01`: initial prompt delivered once and exact references visible. `CLAUDE-02`: follow-up uses the same healthy logical/vendor session. `CLAUDE-03`: operator observes/takes control of the original TUI. `CLAUDE-04`: busy-turn/permission dialogs do not cause uncontrolled input. `CLAUDE-05`: crash/resume behavior matches the declared capability and no stale session is chosen. `CLAUDE-06`: unsupported goal/steer/structured modes are reported explicitly.

## Evidence and references

- **E04 — [Claude Code CLI reference](https://code.claude.com/docs/en/cli-reference).** Official interactive, resume and print/stream modes. Modes and flags require exact-version certification. Accessed 2026-10-14.
- **J06 — [Programmatic launch options](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/services/jackin-runtime-launch-load-options/src/load_options.rs).** Lines 1–300. Non-interactive launch, programmatic constructor, identity sink, inputs and validation error vocabulary. Deeper launch call paths were not executed.
- **J17 — [Existing sealed agent runtime trait](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/crates/core/jackin-core/src/agent/runtime.rs).** Lines 1–260, including the complete trait. Installation, auth and state behavior, not workflow-turn control. Built-in adapter count in comments is stale; no count inferred from it.
