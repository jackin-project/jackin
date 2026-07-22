# Plan 002: Prepare the Homebrew tap to validate native casks without write credentials

> **Executor instructions**: This plan executes in the separate public repository `jackin-project/homebrew-tap`, not this checkout. Read that repository's `AGENTS.md` before changes. Do not add a placeholder cask with a fake URL or checksum. After the tap PR lands, report its URL and merge SHA to the jackin❯ program owner; do not modify this repository's plan index from the tap branch.
>
> **Drift check (run first in the tap checkout)**: record `git rev-parse --short HEAD`, then compare current `AGENTS.md`, `.github/workflows/`, `Formula/`, and `REUSE.toml` with the current-state facts below. Stop if a cask workflow or `Casks/jackin-usage-menu-bar.rb` now exists; reconcile rather than duplicate it.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none; can run parallel with Plan 001
- **Category**: security / dx / tests
- **Planned at**: jackin❯ repository commit `7a52a273`, 2026-07-21; re-stamp with the tap SHA at execution

## Why this matters

The tap has formula validation conventions but no `Casks/` directory, no cask precedent, and only REUSE CI. The upstream release workflow currently opens and immediately merges formula PRs. A first native cask must instead be fetched, audited, installed, signature/notarization-checked, launched, and uninstalled by a read-only macOS job before a human approves its structure; the Apple-secrets/tap-writer job must not validate its own output.

## Current state

- Public `jackin-project/homebrew-tap/AGENTS.md` requires immutable artifact URLs, URL/SHA updates together, `brew fetch` for changed `Formula/*.rb`, credential scanning, PR-only changes, and Conventional Commits. Its required loop does not include `Casks/*.rb`.
- `.github/workflows/reuse-compliance.yml` is the only tap workflow and checks only REUSE compliance on PR/push.
- `Formula/jackin-preview.rb` demonstrates immutable/checksummed generated package metadata, but the rolling preview URL is inappropriate for the stable cask.
- `Casks/` does not exist. There is no existing cask token, `app` stanza, `zap`, `livecheck`, or native launch test to preserve.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Workflow lint | `actionlint .github/workflows/*.yml` | exit 0 |
| REUSE | `reuse lint` | compliance success |
| Credential scan | use the exact staged-diff scan from tap `AGENTS.md` | no output |
| Cask checks once Plan 003 opens the first cask PR | `brew readall --aliases && brew style --cask Casks/jackin-usage-menu-bar.rb && brew audit --cask --new --strict --online Casks/jackin-usage-menu-bar.rb && brew fetch --cask --retry --force ./Casks/jackin-usage-menu-bar.rb` | all exit 0 |

## Scope

**In scope in `jackin-project/homebrew-tap`:** `AGENTS.md`, one necessary `.github/workflows/cask-validation.yml`, and licensing metadata only if REUSE requires it.

**Out of scope:** adding a cask before a real immutable notarized artifact exists, changing formula content, preview casks, tap credentials, upstream release workflow, speculative `zap`, Sparkle/livecheck, or auto-merging the first cask.

## Git workflow

- Use a tap branch such as `chore/native-cask-validation`; all tap changes go through a PR to `main`.
- Follow the tap's Conventional Commit rules. Do not push or merge without the operator's instruction; no force-push.
- This plan never modifies the jackin❯ checkout. The jackin❯ program owner updates the shared index after receiving the merged tap PR URL and SHA.

## Steps

### Step 1: Extend tap policy to casks

Update `AGENTS.md` so immutable URL/SHA coupling, staged fetch validation, credential scanning, and review rules explicitly cover both `Formula/*.rb` and `Casks/*.rb`. Add native-app rules: a cask may reference only an immutable tagged release asset; the cask SHA must equal the release sidecar; Developer ID signature, notarization staple, bundle ID, arm64 slice, and minimum OS are verified after fetch; structural cask changes require human review; only mechanical version/SHA bumps may later use auto-merge.

**Verify**: review the staged shell loops with filenames containing spaces safely handled; the credential scan produces no output.

### Step 2: Add read-only, path-routed macOS cask validation

Add a least-privilege workflow triggered by PR changes to `Casks/**`, itself, and applicable policy metadata, plus `workflow_dispatch`. Define a dispatch input `cask_path` whose default is `Casks/jackin-usage-menu-bar.rb`; dispatch checks out the selected workflow ref, requires that path to exist, and fails with one actionable error when it does not. PR mode discovers changed `Casks/*.rb`; a workflow-only PR with no cask reports an explicit successful no-candidate skip. Pin third-party actions to full SHAs. The job uses `permissions: contents: read`, checks out the PR, and runs Homebrew readall/style/audit/fetch checks. For `jackin-usage-menu-bar.rb`, download/extract the declared artifact and independently compare its SHA with both the cask and release `.sha256`; require one `JackinUsageMenuBar.app`; run `codesign --verify --deep --strict`, `spctl --assess --type execute`, `xcrun stapler validate`, `lipo` architecture checks, and exact plist checks for bundle ID and macOS floor.

Install the cask, verify the app exists under `/Applications`, launch it in the GitHub-hosted Aqua session without provider credentials, poll that it remains alive, terminate it, uninstall the cask, and verify removal. Do not add write permissions, tap tokens, Apple credentials, or auto-merge logic. Upload diagnostic logs on failure without including host credentials.

Because no cask exists yet, test PR no-candidate discovery and the dispatch missing-path error by extracting the candidate-selection shell into a directly invoked step/test with fixture paths; do not invent a placeholder cask or perform network fetches. Plan 003's first cask PR is the first full dynamic artifact proof.

**Verify**: `actionlint` and `reuse lint` pass. Open the tap PR and confirm existing required checks pass; dynamic cask validation is expected to skip on this workflow-only PR.

### Step 3: Configure the future merge boundary

Document the exact check name that Plan 003 must require before merge. If the repository ruleset is managed elsewhere, stop and report the required Terraform/configuration change rather than mutating GitHub settings ad hoc. The first cask must remain human-approved even if mechanical future bumps later receive auto-merge.

**Verify**: `gh pr checks <PR>` shows REUSE plus workflow lint/appropriate cask-validation status; read back the repository ruleset and record whether the named check is enforced.

## Done criteria

- [x] Tap policy applies immutable URL/SHA and staged verification rules to casks.
- [x] A path-routed GitHub-hosted macOS workflow can fetch, inspect, install, launch, and uninstall the future cask without any write/Apple secret.
- [x] The workflow independently checks release sidecar, cask SHA, Developer ID/notarization, arm64 slice, plist identity, and macOS floor.
- [x] No placeholder cask, fake URL, fake checksum, tap token, or auto-merge is introduced.
- [x] Required-check ownership/configuration is documented for Plan 003, and the tap PR URL/merge SHA is reported to the jackin❯ program owner for index reconciliation.

## Execution status

- Tap PR https://github.com/jackin-project/homebrew-tap/pull/417 **merged** squash commit `e091a86f0da93e865982f68efd9bf8359025f039` (2026-07-21).
- Required check name for Plan 003: `cask-validation / Cask validation` (workflow + checks green before merge).
- Ruleset still only `protect-main` + `protect-tags` — enforcing cask-validation as required remains a `jackin-github-terraform` follow-up (documented, non-blocking for plan 002 content).

## STOP conditions

- A cask or cask-validation workflow landed after this plan was written. Re-audit and tighten the existing path; do not create a duplicate.
- The tap token is required merely to validate a public artifact. That is a design error; keep validation read-only.
- GitHub-hosted macOS cannot launch GUI apps in its session. Preserve all static/install checks and report the exact launch limitation for the Plan 004 runner decision; do not claim launch proof.
- Required checks are owned by infrastructure outside the tap repo. Report the exact external change and leave auto-merge disabled.

## Maintenance notes

The cask workflow is a supply-chain boundary. Review every future relaxation of online audit, checksum comparison, Gatekeeper, or signature checks. Do not add `zap` paths until the app deliberately owns documented user-removable state; uninstalling the cask must not guess at credential locations.
