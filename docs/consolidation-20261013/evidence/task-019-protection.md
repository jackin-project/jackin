# TASK-019 protection gate

**Verdict:** BLOCKED

**Missing step:** Specific authorization of a squash pull request from `integration/branch-consolidation-20261013` into protected `main`. That authorization is absent. Stop at the reviewable integration branch. Opening a pull request and merging to protected `main` stay unauthorized.

## Authorization

Current local instruction: merging to protected main is not authorized; stop at the reviewable branch if a squash PR was not specifically authorized.

| Action | Authorized | Basis |
| --- | --- | --- |
| Open a pull request | No | A squash PR was not specifically authorized. This check also forbids creating a PR. |
| Merge a pull request into protected `main` | No | Merging to protected main is not authorized. |

No push, pull request, merge, or git config change was performed. `gh` was already authenticated as `donbeave` and was used only for read APIs.

## Ruleset 14746904 `protect-main`

Observed 2026-10-13 via `GET /repos/jackin-project/jackin/rulesets/14746904`. The ruleset is the only active branch ruleset. Enforcement is `active`. It includes `~DEFAULT_BRANCH`. The repository default branch is `main`. `bypass_actors` is empty. `current_user_can_bypass` is `never`. `updated_at` is `2026-10-02T19:36:59.877+07:00`.

| Requirement | Still required | Evidence |
| --- | --- | --- |
| Squash-only PRs | Yes | `pull_request.parameters.allowed_merge_methods` is `["squash"]`. Repository settings: `allow_squash_merge=true`, `allow_merge_commit=false`, `allow_rebase_merge=false`. |
| Linear history | Yes | Rule type `required_linear_history` is present. |
| DCO | Yes | `required_status_checks` includes context `DCO`. |
| Required check | Yes | `required_status_checks` includes context `Required`. |

The same pull-request rule also sets `required_review_thread_resolution` to true, `required_approving_review_count` to 0, and `require_extra_approval_for_unattributed_changes` to true. Deletion and non-fast-forward are also denied. Server approval count zero does not authorize a merge.

## Refs

- Remote: `https://github.com/jackin-project/jackin.git` (`jackin-project/jackin`)
- Checkout: `/Users/donbeave/Projects/jackin-project/wt-integration-20261013`
- Local HEAD: `7a05f3aebda4a73e183b665702f00bb7a38ce36f` on `integration/branch-consolidation-20261013`, tracking `origin/integration/branch-consolidation-20261013`
- Remote integration SHA: `7a05f3aebda4a73e183b665702f00bb7a38ce36f`
- Remote `main` SHA: `1c3dd1be4a3b1ed518133ec3899c084c792370ff`
- Pull requests from `jackin-project:integration/branch-consolidation-20261013` into `main`: 0
- Pull request created by this check: no
