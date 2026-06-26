# Cleanup requires explicit operator approval

Status: accepted

An instance carrying dirty or unpushed isolated work is removed only by an explicit operator decision — an intentional delete, or an explicit `discard` outcome. Any interruption, crash, or abort — including one that happens while the operator is partway through inspecting the changes — leaves every instance preserved as resumable dirty state, treated exactly like a crashed instance. There is no implicit cleanup path for at-risk work. (A clean, fully-pushed session still auto-cleans, because it has nothing to lose.)

## Why this is recorded

The natural implementation instinct is to clean up on the way out — on exit, on error, on a timeout — to avoid leaving orphaned containers and directories. For at-risk work that instinct is wrong here, and a future contributor adding an "auto-prune on crash" or "clean up on abort" path would silently destroy unpushed work. This records that preservation-on-interruption is deliberate, not an oversight.

## Consequences

- A crash during the dirty-inspection drill-down (repo list → file tree → diff) loses nothing: the instance remains a restore candidate.
- The cost is that interrupted sessions accumulate as preserved instances until the operator deletes them. That is acceptable: the operator always has the launch/console surfaces to review and delete them deliberately, and `dirty_exit_policy = "discard"` is the opt-in for workspaces that want automatic discard.
- This bounds what the cleanup and prune paths may remove without approval to: clean + fully-pushed sessions, and instances the operator explicitly deleted or discarded. Everything else is retained.
