# Parity slice design (agent 16, 2026-09-21, read-only)

Read-only sweep complete. No writes made anywhere. All findings verified in source.

# 1. Current merge-protection state (live, verified)

## 1.1 Ruleset `protect-main` (id 14746904, enforcement `active`)

`gh api repos/jackin-project/jackin/rulesets/14746904` (target `branch`, `~DEFAULT_BRANCH`):

| Rule | Parameters |
|---|---|
| `deletion` | — |
| `pull_request` | `required_approving_review_count: 0`, `dismiss_stale_reviews_on_push: true`, no code-owner review, methods `[merge, squash, rebase]` |
| `required_status_checks` | contexts **`DCO`** (app 974774), **`Policy`** + **`ci-required`** (app 15368 = GitHub Actions); **`strict_required_status_checks_policy: true`**, `do_not_enforce_on_create: false` |
| `non_fast_forward` | — |
| bypass | `RepositoryRole` admin (`actor_id 5`), `bypass_mode: always` |

Rule types present: `deletion, pull_request, required_status_checks, non_fast_forward` — **no `merge_queue` rule**. Legacy branch protection: 404 (`Branch not protected`) — rulesets are the only gate. Merge queue: `mergeQueue: null` (GraphQL). Tag ruleset `protect-tags` (15179226): deletion + non-ff only, irrelevant here.

Effective merge method is **squash-only**: repo settings show `allow_squash_merge: true`, merge/rebase `false`, narrowing the ruleset's three-method list.

## 1.2 Generated gate-emitted checks vs required contexts

Check-run names are emitted **bare** (job `name:`, no `workflow /` prefix) — proven live on main HEAD `fce94cea`:

- `ci-required` ← `ci-pr.yml:2638-2639` (`name: ci-required`), needs `[plan, <40 unit callers>]`, gate script `ci-pr.yml:2651-3461`. Also emitted by `ci-main.yml:2750` on main SHAs (post-merge signal, not merge-blocking).
- `Policy` ← `ci-policy.yml:17-18` (`pull_request_target` + dispatch). Also `ci-main.yml:107` on main SHAs.
- `DCO` ← external DCO-2 app (no workflow file).
- Mirror `Control / Required` (`ci-pr.yml:3463`) is **not** a ruleset context — pure UX duplicate.

Coverage is enforced both ways by the Policy validator (`/tmp/velnor/crates/velnor-workflow/src/policy.rs:1708-1792`): every `[policy] ruleset_required_status_checks` entry must be a job name in `ci-pr.yml`, and live ruleset contexts must equal declared `ruleset_required_status_checks + ruleset_external_status_checks + Policy-entrypoint names`, else Policy fails the PR. Jackin declares `ruleset_required_status_checks = ["ci-required"]`, `ruleset_external_status_checks = ["DCO"]` (`.github-gen/velnor-workflow.toml:25-27`).

## 1.3 What blocks merging a red/stale PR today

- **Red**: any of DCO/Policy/ci-required missing or non-success on the PR head. Live proof: PR #1014 (`6474f6fb`) has `Policy`+`DCO` success but **no `ci-required` run yet** → `mergeStateStatus: BLOCKED`, `mergeable: MERGEABLE`. (PR #1007 all-green → `CLEAN`.)
- **Stale**: `strict_required_status_checks_policy: true` = "require branches up to date". Behind-base PRs must update; old greens don't carry.
- **Conflicting**: `DIRTY` (#1009, #1005) via mergeability + `non_fast_forward`.
- **Reviews**: nothing (0 approvals required).
- **Hole**: admins bypass everything (`bypass_mode: always`); no merge queue means two simultaneously-merged green PRs never test their combination (TOCTOU; strict policy only serializes, doesn't combine-test).

# 2. Slice plans

## Slice (a): generated `merge_group` / merge-queue support

**Current state (all verified):** `scope_for_event_values("merge_group", None) → full`, rejects `affected` (`runtime.rs:1627-1634`, tests `runtime.rs:1750-1765`); the `AutomaticEvent::MergeGroup` model already asserts lane selection (`primitives/ir.rs:2921-2938` + `debug_assert`s at `ir.rs:5934-5938, 5967-5971`); but rendering omits the trigger everywhere, locked by tests: `lib.rs:16143-16224` `generated_pr_workflow_omits_merge_group`, `s2/mod.rs:16360` mirror, `tests/velnor_first_ci.rs:239`, `lib.rs:16094, 16273, 19510`, save-gate tests `lib.rs:16983-16993`. D8 decision (`/tmp/velnor/plans/2026-09-16-ci-workflow-and-cache-plan.md:227`): trigger removed as dead until a queue exists; re-add to **job admission only, never the save gate**.

**Velnor generator changes** (live tree is non-`s2`: `lib.rs:5696` dispatches to `s2` only on providers-flag; generated Jackin YAML matches the non-s2 gate shape at `primitives/ir.rs:4300-4306`):
1. Config knob (default off, preserves D8): e.g. `[workflow] merge_queue = true` in `src/config/` + `ProjectConfig`/`WorkflowIr` plumbing.
2. `aggregate_triggers` PR arm (`primitives/ir.rs:3138-3153`): emit `on:\n  pull_request:\n  merge_group:\n…` when enabled.
3. `automatic_event_expression` (`ir.rs:5946-5976`): append `|| github.event_name == 'merge_group'`; same for `velnor_automatic_event_expression` (`ir.rs:5918-5944`) — the existing `debug_assert!(MergeGroup selects lane)` lines become live truth.
4. `base_sha_expression` (`ir.rs:5978-5983`): insert `github.event.merge_group.base_sha ||` after `pull_request.base.sha` (merge_group payload has no `pull_request` object; plan `BASE_SHA` env is rendered from this — cf. `ci-pr.yml:72`).
5. Concurrency: `aggregate_concurrency_group` PR arm (`ir.rs:3091-3094`) already degrades to `github.ref` (`gh-readonly-queue/…`, unique per group) — keep; keep `cancel-in-progress: true` (verify live, see below).
6. **Do not touch** the trusted save gate (cache saves must never mention `merge_group`; keep `lib.rs:16983-16993` green).
7. Invert the forbidding tests: `generated_pr_workflow_omits_merge_group` splits into (i) default fixture still omits everywhere, (ii) `merge_queue=true` fixture asserts `merge_group:` on PR render + admission clauses present + save gate absent; same for the `s2/` mirror and `velnor_first_ci.rs` (`pull_request_publish_required` gains a queue-enabled sibling). Velnor-only surface keeps omitting (no GitHub lane to validate the queue — existing assertion stays unconditional).

**Jackin input changes:** `.github-gen/velnor-workflow.toml` gains `merge_queue = true` under `[workflow]`; regenerate; ruleset change via API: add a `merge_queue`-type rule to `protect-main` (grouping/build parameters) alongside the existing `required_status_checks`.

**Blocking sub-problem to resolve in-slice — DCO + Policy are not queue-capable:** required checks must report on the merge-group SHA. `ci-required` will (new trigger), but `Policy` runs on `pull_request_target` (`ci-policy.yml:4-7`), which has no `merge_group` analogue, and `DCO` is an external app whose merge-group behavior is unknown. Options: (i) make Policy's validator runnable on `merge_group` as a second generated file/job (payload has no PR head/base — needs design), (ii) confirm DCO app reports on merge SHAs, else (iii) shrink the queue-blocking set. Do not enable the queue until CI proves all three contexts go green on a `merge_group` SHA.

**Generic-fixture requirements:** extend the existing `scanned_fixture`/`fixture_root` harness (`lib.rs` test utils) with the `merge_queue` flag — no Jackin paths, units, or branch names in assertions; assert on trigger keys, admission substrings, and `merge_group.base_sha` in the plan env.

**Live-CI verification:** (1) `cargo test -p velnor-workflow` (both trees + integration); (2) regen Jackin tree, `git diff` shows only `merge_group:` + admission + base expr deltas; (3) open PR, add `merge_queue` rule to a *copy* ruleset scoped to a scratch branch (or off-hours toggle + revert), enqueue two PRs, assert: `merge_group` run executes full scope (`scope=full` in plan log), `ci-required` posts success on the group SHA, queue merges; (4) negative: push conflicting second PR, assert queue re-validates rather than fast-merging.

## Slice (b): empty-selection guard in `ci-required`

**Current hole (verified):** gate builds `selected=",$SELECTED_UNITS,"` (`ci-pr.yml:2661`, rendered by `render_required_caller_verdicts`, `primitives/ir.rs:3617-3637`) and each caller block's `else` accepts `success|skipped`. With `units=""` every block takes `else`, all callers skip, plan succeeded → **exit 0, zero verification, merge allowed**. No min-units check exists anywhere in `ci-required` (`ci-pr.yml:2638-3462`).

**Exhaustive empty paths** through `plan` (`runtime.rs:1505-1566`) → `selection_for_diff` (`runtime.rs:2229-2311`) → `selection_for_lanes` (`runtime.rs:1688-1720`):
1. `changed.is_empty()` → `units=[], full_units={}` (`runtime.rs:2245-2250`). Only reachable with an empty diff (base==head; e.g. dispatch `base_sha=HEAD`, empty PR edge). **Legitimate.**
2. Lane filtering (`selection_for_lanes`) can empty a non-empty selection (documented `runtime.rs:1678-1687`: velnor-only dispatch drops unrunnable kinds; `full_units` filtered too, `runtime.rs:1714-1718`). Only on multi-lane dispatches (Jackin is `runners="github"`, `VELNOR_LANES` unset → `Both` → keeps all). **Legitimate iff scope==affected.**
3. Every other path returns non-empty (`full_selection`, version-bump requires non-empty allowlist, unmatched file → full, matches only add, `expand_affected_units_with_full` only grows, `runtime.rs:2416-2448`). So: **scope==full + units=="" is always a planner/lane bug**; **affected + units=="" + full_units≠"" is always inconsistent.**

**Exact guard condition** (rendered bash, placed right after `selected=",$SELECTED_UNITS,"`, needs two new gate env vars from existing plan outputs `scope`/`full_units`, `ci-pr.yml:44,48`):
```bash
if [[ -z "$SELECTED_UNITS" ]]; then
  if [[ "$SCOPE" == affected && -z "$FULL_UNITS" && \
      "$EVENT_NAME" == pull_request || ... ]]; then
    echo "::notice::legitimate empty selection (empty diff / fully lane-filtered affected scope)"
  else
    echo "ci-required: empty unit selection outside the legitimate-empty class (scope=$SCOPE full_units=$FULL_UNITS event=$EVENT_NAME)" >&2
    exit 1
  fi
fi
```
i.e. fail iff `units=="" AND NOT (scope==affected AND full_units=="" AND event∈{pull_request, workflow_dispatch})`. Deliberate behavior change: full-scope lane-filtered-to-empty (wrong-lane dispatch on an unrunnable repo) now fails loudly instead of silently green — currently-green-by-design per the `selection_for_lanes` comment, but silent green on an explicit full-scope request is exactly the vacuity §6 forbids.

**Owned files:** Velnor `primitives/ir.rs` (gate render `render_required_gate` ~`ir.rs:4300-4335` + `render_required_admission_env` pattern for the new env) + `src/config`? none. Jackin input: none — ships automatically on regen. Mirror in `s2/primitives/ir.rs:4394+` gate render.

**Generic fixtures:** render-level test with a 2-unit Rust fixture asserting the guard block text + the three env bindings; planner tests pinning `changed.is_empty() → (units="", full_units="")` and `full ⇒ non-empty`.

**Live verification:** (1) generator tests; (2) regen diff shows only the guard; (3) positive-legitimate: `workflow_dispatch` on `ci-pr.yml` with `scope=affected`, `base_sha=<HEAD>` → empty diff → `ci-required` green with the notice line; (4) regression: normal PR + full-scope dispatch both green with unchanged caller verdicts (guard inert when `units` non-empty); (5) fail-closed proof has no live trigger by construction (full+empty is unreachable without a planner bug) — covered by fixture tests + `shellcheck` on the rendered snippet.

## Slice (c): desktop / release-prep in the pre-merge contract

**Current state (verified):** `desktop-ci` (`mise.toml:221-233`: bindings-check, generate, format-check, lint, test, build, test-swift, verify) is referenced by **no workflow**; `desktop-merge` (`mise.toml:235-241` = desktop-ci + test-ui) runs only on `push: [main]` (`desktop-merge.yml:5-8`); `desktop-scheduled` (+ deadcode) Monday cron only; release drill (`desktop-release-env, desktop-build, desktop-verify, desktop-release-state`) is dispatch-only (`release.yml` build job `if: workflow_dispatch`). PR #1014's head has **no** `Desktop merge cadence` check run. Note: `status = "required"` on a check profile (`config/mod.rs:3403`) means fail-closed *within its file* (`continue-on-error` off), **not** merge-blocking — merge-blocking comes only from ruleset contexts on the PR head.

**Which tasks, unprivileged feasibility (all verified secret-free):** `desktop-merge.yml`/`desktop-scheduled.yml` reference zero `secrets.*`/`vars.*`; `run-ui-tests.sh` needs only a macOS host (builds, `pgrep`-scoped kills of repo apps, lock dir); `desktop-release-env` dispatch branch is a fixed fixture (`mise.toml:411+`), `desktop-release-state` is read-only prints, `desktop-verify` without `--release` needs no Apple credentials. `contents: read` + `macos-26` hosted is sufficient for: `desktop-ci` ⊂ `desktop-merge` (PR graph + UI tests) and the release drill minus `desktop-sign-notarize` (which alone needs the `release-macos` environment + Apple secrets — stays tag-gated). Fork-PR safe (no secrets, GitHub-hosted, `persist-credentials: false`).

**Design (two tiers):**
- C1 — desktop pre-merge (needs a **small generator affordance OR the external-checks escape hatch**): add `pull_request` to the `desktop-merge.yml` declare row's `events` (accepted: `push/pull_request/workflow_dispatch`, `check_profiles.rs:185`) → job `Desktop merge cadence` runs on PR heads. Merge-blocking requires the ruleset to require context `Desktop merge cadence`, and Policy's live-diff (`policy.rs:1769-1782`) requires the tree to declare it — but anything in `[policy] ruleset_required_status_checks` must be emitted by `ci-pr.yml` (`policy.rs:1727-1733`), which a scheduled-checks job never is. So either (i) declare it in `ruleset_external_status_checks` (passes Policy today; semantically "generated-but-out-of-aggregate"), or (ii) generator slice: allow scheduled-checks contexts in the required set with cross-file emission lookup. Prefer (ii) long-term; (i) is the no-generator-change path (§3).
- C2 — release-prep pre-merge: new `check_profile` (e.g. `desktop-release-drill`, runner `macos`, tasks `desktop-release-env/desktop-build/desktop-verify/desktop-release-state`, PR trigger, no secrets/env) reusing the release build job's task list minus publish. Pure Jackin input + regen; optionally ruleset-required via the same C1 mechanism.

**Generic fixtures:** scheduled-checks render tests with synthetic profiles asserting `pull_request:` trigger emission + `contents: read`-only + no secret refs when tasks declare none; Policy test for the (ii) cross-file emission lookup using fixture filenames only.

**Live verification:** (1) generator tests + regen diff; (2) open a PR touching `native/**`, assert `Desktop merge cadence` (and drill) check runs appear on the head and ride to green; (3) negative: break Swift formatting on the PR branch, assert the desktop context fails and `mergeStateStatus` stays `BLOCKED` (once ruleset-required); (4) fork-PR drill: same from a fork, assert no secret access and green/red correctness.

# 3. Near-term hardening WITHOUT generator changes (Jackin-side only)

All executable today — input TOML + regen + `gh api` ruleset edits (regeneration is byte-deterministic; Policy's D19 pin check keeps the tree honest):

1. **Desktop pre-merge signal (advisory first):** add `pull_request` to `events` on the `desktop-merge` declare row (`.github-gen/velnor-workflow.toml:94-101`) and regen. Desktop runs on every PR head immediately; not yet merge-blocking, zero ruleset risk. Promote to blocking via `ruleset_external_status_checks = ["DCO", "Desktop merge cadence"]` + ruleset context add once flake rate is proven (see C1(i) caveat above).
2. **Release-drill pre-merge (advisory):** add a `desktop-release-drill` check profile + `pull_request` declare row cloning the release build task list (`velnor-workflow.toml:192-201` minus publish). Catches release-only breakage (version parsing, `release-state`, verify strictness) before tags.
3. **Close the admin-bypass hole administratively:** remove or scope the `RepositoryRole=admin / always` bypass on `protect-main` (ruleset PATCH), or at minimum require it never be used for red CI — today a single admin click merges past all three required checks.
4. **Tighten the merge surface:** allowed-methods already squash-only via repo settings — align the ruleset's `[merge, squash, rebase]` list down to `[squash]` so the declared contract matches enforcement; keep `strict_required_status_checks_policy: true` (it is the only serialization primitive until slice (a) lands).
5. **Docs-only / no-op PR hygiene:** no change needed — unmatched files already fail closed to full scope (`runtime.rs:2301-2303`); the only vacuous-green path is the empty diff, which slice (b) classifies. Until (b) lands, treat any `ci-required` green with `units=""` in the plan log as suspect (auditable per-run today).

**Residual risks to name:** (i) merge-combination TOCTOU persists until slice (a) + queue; (ii) vacuous-green on empty selection persists until slice (b); (iii) desktop/release stay post-merge-only until (c)/§3-1/2 land — a red `main` from desktop breakage is currently the detection path.