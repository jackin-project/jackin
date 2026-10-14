# Task 002 — b044 function-name removal review

Verdict: **AGREE for the 15-name sample.**

This review does not approve restoring these names and does not approve a branch.

The other 191 names in the list were not re-checked.

## Scope

Read-only check against `/Users/donbeave/Projects/jackin-project/jackin`. No edits to the Jackin clone, no commit, no cargo.

Name list: `/Users/donbeave/Projects/merge/jackin-evidence-prefetch-20261013/b044-fn-names-absent-from-M-P-U.txt`.

Parent claim under test: every name’s newest `git log -S 'fn NAME'` hit in `d32e85563648931f80ffab141bb2650f478c2a60..fba69d60c2373156dc0dcecee8e085c83776966f` is `3f3ddd7c48284b648b083305f969ee3e709b74b7`, and that commit is an ancestor of main `1c3dd1be4a3b1ed518133ec3899c084c792370ff`.

## Name count

The file contains **206** nonempty lines, **206** unique names, and no blank lines. The first name is `activate_broker`. The last name is `write_isolated_config`.

`wc -l` prints 205 because the file does not end with a newline. The name count is 206.

## Ancestry

`git rev-parse main` is `1c3dd1be4a3b1ed518133ec3899c084c792370ff`.

`git merge-base --is-ancestor 3f3ddd7c48284b648b083305f969ee3e709b74b7 1c3dd1be4a3b1ed518133ec3899c084c792370ff` exited 0. `3f3ddd7c` is an ancestor of main.

`git merge-base --is-ancestor 3f3ddd7c48284b648b083305f969ee3e709b74b7 fba69d60c2373156dc0dcecee8e085c83776966f` exited 0. `3f3ddd7c` is an ancestor of policy `fba69d60`.

That commit is `feat(usage): add explicit Claude usage monitoring (#1122)`, committer date `2026-10-10 18:13:04 +0700`. The tested range contains 360 commits.

## Sample

Fifteen names, spread through the file, including the first, the last, `catalog_cas_rejects_a_stale_rotation_after_a_newer_winner` (line 36), and `canonical_projection_alias_transition_is_atomic_idempotent_and_fail_closed` (line 27). Line numbers are 1-based `splitlines` positions.

For each name, the newest pickaxe commit is the first line of:

`git log --format=%H -S 'fn NAME' d32e85563648931f80ffab141bb2650f478c2a60..fba69d60c2373156dc0dcecee8e085c83776966f`

Absence was checked on the trees of main `1c3dd1be` and policy `fba69d60` with `git grep -I -n -w -e NAME` and `git grep -I -n -F -e 'fn NAME'`. Both searches returned no hits for every sample name.

| Line | Name | Pickaxe hits in range | Newest | Older hits | On main | On policy |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `activate_broker` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700) | absent | absent |
| 16 | `canon_openai_account_never_appears_under_routed_providers` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700) | absent | absent |
| 27 | `canonical_projection_alias_transition_is_atomic_idempotent_and_fail_closed` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700) | absent | absent |
| 36 | `catalog_cas_rejects_a_stale_rotation_after_a_newer_winner` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700) | absent | absent |
| 60 | `compact_status_bar_strip_soonest_then_remaining_cap_and_separator` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700) | absent | absent |
| 74 | `detail_presentation_dto` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700) | absent | absent |
| 89 | `ensure_discovered_account` | 3 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700); `7d14da64706d6210a9de2c5220b01c22a7a8c1e0` (2026-10-07 06:55:22 +0700) | absent | absent |
| 104 | `fixture_snapshot_round_trip_via_bridge` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700) | absent | absent |
| 118 | `isolated_cli_missing_sibling_fails_with_installation_diagnostic` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `7bd6871d0b02adabf582818c1ba1bb6f7a6d7d2b` (2026-10-10 04:01:03 +0700) | absent | absent |
| 133 | `open_bridge_with_live` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700) | absent | absent |
| 147 | `provider_glance_rows_icon_keys_match_closed_desktop_domain` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700) | absent | absent |
| 162 | `reconcile_broker_catalog` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700) | absent | absent |
| 177 | `selected_account_route_dto` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700) | absent | absent |
| 191 | `status_bar_provider_glance_rows_sb3_sb17_sb19` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700) | absent | absent |
| 206 | `write_isolated_config` | 2 | `3f3ddd7c48284b648b083305f969ee3e709b74b7` | `29db1854cf694d3a3f9cdae23659ffe5ee5da3cc` (2026-10-09 17:14:50 +0700) | absent | absent |

Every older hit is earlier than `2026-10-10 18:13:04 +0700`. No sample name failed.

## Limit

Agreement covers these 15 names only. The remaining 191 names were not re-checked. This file does not approve restoring the names and does not approve a branch.
