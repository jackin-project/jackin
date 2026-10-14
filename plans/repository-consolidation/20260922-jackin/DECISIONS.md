# Decision ledger

## Baseline

- Audit date: 2026-09-22.
- Canonical checkout at start: `/Users/donbeave/Projects/jackin-project/jackin`.
- Starting checkout: `feat/multi-account-support` at `2a318440ce2e02a15a76530812f375ee4998a0f3`; clean except ignored `target/`.
- Fresh upstream baseline after non-pruning fetch: `origin/main` at `df4671e4d9f2860e90a5c71d8d0bd85b23d23291` (`docs(ci): carried-failures report with recommendations (#1053)`).
- Current upstream after accepted batches: `origin/main` at `fd14ac6a7e0642842e66eeeb31d4b589d07148e1` (squash merge of #1077 after #1071–#1076).
- Repository policy observed live: default branch `main`; squash merges enabled; merge commits and rebase merges disabled; merged PR branch deletion enabled.

## Initial source classes

- Local Git: 101 local heads, 266 tags, 1090 fetched `origin` heads, 1062 fetched PR-head refs, 80+ registered linked worktrees, no stashes.
- Remote PRs at initial capture: 1062 total; 15 open, 1047 closed; 925 merged and 122 closed-unmerged. Subsequent consolidation PRs are recorded below.
- The full machine-readable source inventory is `INVENTORY.json`; raw GitHub API captures and ref snapshots remain in the external recovery directory.

## Operating decisions

- Use one consolidation branch/worktree from the fresh `origin/main`; preserve the original `feat/multi-account-support` ref until every unique commit and local state is dispositioned.
- Treat same-tip refs as aliases in the change map, but retain each ref/path as a separate source record.
- Merged PRs and ancestor-only branches require implementation evidence before `ALREADY_PRESENT`; PR state alone is not proof.
- Closed-unmerged, open, detached, reflog-only, and dirty sources require explicit logical-change dispositions.
- Do not touch `.github/**` directly; generated workflow changes must be traced to Velnor inputs or rejected as repository-specific generated edits.
- Do not delete any branch, worktree, clone, stash, or remote ref until the cleanup manifest names its exact object/state fingerprint, recovery evidence, landed disposition, and current-state recheck.

## Pending

- Verify the final landed `main`, repeat discovery, and execute only manifest-approved cleanup.

## Recorded dispositions

- PR #1002 / `feat/multi-account-support` — `SUPERSEDED`: selective product integration already landed through #1013 and later security/lifecycle follow-ups. Preserve the branch/ref until final cleanup.
- PR #1004 / `codex/verify-preview-package` — `SUPERSEDED`: its useful release work was freshly ported by #1066; stale source remains recoverable in the captured refs and is not merged.
- PR #1066 / `integrate/branch36-verify-preview` at `85a7bd39a0d466b692af39f08ceb2f1fa267b240` — `ACCEPT_AS_IS`: selective release verification/SBOM/legacy-preview migration port; live required checks were green and the exact head SHA was supplied to the squash merge API. Squash commit: `e50c6e9ba3930a67c93e8b85b3d2233f00dfc5d7`.
- PR #1067 / `docs/report-live-results` — `ACCEPT_AS_IS`, merged as `d53b1a517e2752b74109ebe3df817a565c7d5152`; documentation-only and no generated `.github` files changed.
- PR #1063 / `fix/usage-broker-fallback` — `REJECT_AS_SUBMITTED`: P1 live-container/socket cleanup leak and forbidden in-process fallback. Safe capsule diagnostics, Keychain consent handling, and cleanup semantics are being reimplemented as separate current-main adaptations; original ref remains preserved until final cleanup.
- PR #1071 / `chore/preview-verification-safety-20260922` at `3ad15ba3f0ade7907effa42952cea59c068de405` — `ACCEPT_AS_IS`; live remote provenance, index-flag rejection, downloaded package-content verification, and run-scoped staging boundaries. Merged squash commit `0f5eff7869751e3f03f77f04cd9956b48e8e127b` after 43 required checks passed.
- PR #1072 / `chore/console-identity-20260922` at `110ea3a385aecfab49cd438955feeb3833e2745c` — `ACCEPT_AS_IS`, focused identity projection fix; merged squash commit `7f8c7036d5c72231aa7d576f0a2e19cf047a51bf` after all required checks passed.
- PR #1073 / `chore/usage-presentation-20260922` at `a9ba4568e853a68a2e8b1a7d10700d7681f006fd` — `ACCEPT_AS_IS`; preserve structured spend units and propagate broker failure state to retained rows. Merged squash commit `113786e5aa58b5357f6fd60d18df5dc1a0c4aea9`.
- PR #1074 / `fix/capsule-manifest-errors-20260922` at `f25a6fa22ee5646b40c507b0c4c83b03249d7195` — `ACCEPT_AS_IS`; channel-aware signed capsule-manifest diagnostics with stable/preview tests. Merged squash commit `e3415dada382723f4ce116277576208cf8dd5177` after exact-head checks passed.
- PR #1075 / `fix/keychain-consent-diagnosis-20260922` at `e90a37980249d4031d4d7155d697d6ebcb2a321b` — `ACCEPT_AS_IS`; narrow Keychain `-25308` consent classification and tests. First artifact upload hit intermediary `403 Forbidden`; failed jobs were rerun successfully, then exact head merged as squash commit `866dd90ae316604078b14f9e547f1deee39bdd0b`.
- PR #1076 / `fix/launch-cleanup-evidence-20260922` at `67579b0685a2e0381c74a132afb07a88d1a02ea8` — `ACCEPT_AS_IS`; state-gated cleanup preserves failed-launch evidence. Merged squash commit `de046d3345bb2ec44f9749655cd29164b6e4e4c9` after required checks passed.
- PR #1077 / `fix/preview-sorted-iteration-20260922` at `d19b0602e0b2e6145720d2d06b54758c01fe5429` — `ACCEPT_AS_IS`; routes five preview/package directory scans through `read_dir_sorted`, removing platform-dependent validation order. Local strict lint clears the unsorted-iteration gate; other baseline lint ratchets remain. All protected checks passed; exact-head squash merge is `fd14ac6a7e0642842e66eeeb31d4b589d07148e1`.
- PR #1007 — `REJECTED_AS_SUBMITTED`; stale generated tree and instruction-file/schema regressions. Closed with evidence comment; refs preserved.
- PR #1044 — `BLOCKED/CLOSED`; generic Velnor arm64 mold defect requires upstream capability and regenerated outputs. Closed with evidence comment; refs preserved.
- PR #1064 — `REJECTED_AS_SUBMITTED/CLOSED`; current generated collector requires unavailable Velnor `actions: read` capability and has provenance/classification gaps. Closed with evidence comment; refs preserved.
- PR #1065 — `REJECTED_AS_SUBMITTED/CLOSED`; incomplete FFI tool closure and shared Velnor mold blocker. Closed with evidence comment; refs preserved.
- PRs #1030, #1045, #1058, #1060 — `DO_NOT_MERGE/CLOSED`; proof artifacts only.
- PRs #1068, #1069, #1070 — `SUPERSEDED/CLOSED`; paused handoffs preserved as source refs and recovery records.

## Dirty-source dispositions

- Apple usage relay, auth-source identity, restore identity, usage credential routing, and legacy usage-presentation worktrees — `ACCEPT_WITH_ADAPTATION` only; their useful ideas were stale or multi-authority and were not merged wholesale. Recovery patches remain preserved.
- Console identity worktree — narrow current-main projection was ported and merged as #1072; original dirty state remains preserved until cleanup.
- Launch-security repair worktree — `REJECT_AS_SUBMITTED`; stale API assumptions, unrelated identity edits, formatting/runtime failures, and the name-based container race remained. Anscombe's review found no safe product patch; recovery patch remains unchanged.
