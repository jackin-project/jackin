# PR #1283 Per-File Disposition — 2026-11-14

**Date:** 2026-11-14
**Subject:** `jackin-project/jackin` PR #1283 (`chore(dirty): integrate the retained jackin-core, jackin-session, jackin-tui, jackin and xtask changes`)
**PR state:** CLOSED, unmerged, at 2026-11-13T22:04:01Z; the head branch `chore/dirty-slice-1` was deleted with it. The source diff of the deleted branch is still reachable through the durable PR ref `refs/pull/1283/head` (head `72401bf5e78c630f6fc2765e6391bb9ff7188635`); see the verification command in the Counts section.
**Head commits:** `6d44f820576ca20df6587e7c6bae1785c7a05682` (the T08 promotion) + `72401bf5e78c630f6fc2765e6391bb9ff7188635` (the `account_add` assertion).
**Merge base:** `78d3b86ed520c1a333b4152bdbaeba94ff8e4ba2`. **Compared against:** `origin/main` = `fc00663fbbf7c16e4519bf21ce4981043cfeb1a0` (#1282).
**Replacement PR:** `chore/restore-1283-remainder` (this branch), which restores only the sound remainder.

## Class definitions

- **VALUABLE** — content that is **not** in `main` and carries unique value. It is restored here, in a corrected form, so it is not lost with the deleted branch.
- **CONTAINED-proven** — the file's content is already in `main`, with the merged PR and SHA recorded. Nothing is lost by dropping it.
- **DEBRIS** — logs, scratch output, generated artifacts, or duplicated scratch work. No unique product value.
- **REJECTED-with-reason** — the change encodes or depends on the **T08 promotion**, which is **held pending the supplementary legs**. Dropping it keeps `main` consistent with the recorded decision; it is not lost content, it is a decision that was never granted.

## The promotion decision that governs this disposition

The T08 Kimi Code promotion is **not** granted. `rewrite/decisions.tsv:510` records that the combined-diff reviewer required vendor-side usage attribution with no-overage proof, a supervised interactive task, and live secondary-home execution, none of which the 2026-11-10 one-shot proof covers. `rewrite/decisions.tsv:511` records that the supplementary attempt stopped on the Kimi weekly (7-day) quota. `rewrite/decisions.tsv:514` records the restoration of `rewrite/blockers/T08-kimi-code.md` while the tuple stays unavailable. `rewrite/evidence/A14-t08-supplementary.md:16-28` records the quota failure and names the three legs that remain unproven.

Therefore `Agent::KimiCode` stays `TupleState::Unavailable` on `main` with the reason `native refresh and final live proof pending`, and `rewrite/blockers/T08-kimi-code.md` stays present. Every file below that flips, encodes, or asserts that promotion is **REJECTED-with-reason**.

## Disposition table (one row per file of the 12)

| file | class | evidence |
|---|---|---|
| `crates/jackin-core/src/agent.rs` | REJECTED-with-reason | The only product-code change of the PR: the T08 tuple row flips `TupleState::Unavailable` + `Some("native refresh and final live proof pending")` to `TupleState::Supported` + `None`. That is the promotion itself, and the promotion is held pending the supplementary legs (`rewrite/decisions.tsv:510-511,514`; `rewrite/evidence/A14-t08-supplementary.md:16-28`). `main` (`fc00663fb`) keeps T08 `Unavailable`, and `crates/xtask/tests/repo_gates.rs` requires the T08 blocker to exist while the tuple is unavailable. Dropped; no Rust file in this PR differs from `main`. |
| `crates/jackin-core/tests/catalog.rs` | REJECTED-with-reason | The catalog `ROWS` entry for T08 follows the flip: `state: TupleState::Supported, reason: SUPPORTED`. It is the test mirror of the promotion and fails against the `main` state (`rewrite/decisions.tsv:510-511,514`; `rewrite/evidence/A14-t08-supplementary.md:16-28`). Not in `main` (`git diff --stat $(git merge-base origin/main refs/pull/1283/head) refs/pull/1283/head -- crates/jackin-core/tests/catalog.rs` against the closed PR ref `refs/pull/1283/head` = 4 changed lines) and not restorable without the promotion, so it is dropped rather than restored. |
| `crates/jackin-core/tests/native_credentials.rs` | REJECTED-with-reason | The change deletes the T08 case from `native_subscription_tuples_stay_unavailable_with_oauth_reasons`, so the "native OAuth tuples stay unavailable" gate would no longer cover T08 while the tuple is still `Unavailable` (`rewrite/decisions.tsv:510-511,514`). Dropped; the refusal case stays in `main`. |
| `crates/jackin-session/tests/plan/availability.rs` | REJECTED-with-reason | Moves T08 from `REFUSALS` to `SUPPORTED` (`const SUPPORTED: [Case; 15]` -> `[Case; 16]`, `const REFUSALS: [Refusal; 7]` -> `[Refusal; 6]`) and adds the `agents.kimi` role mapping. It asserts the promoted availability, which the supplementary legs have not proven (`rewrite/decisions.tsv:510-511,514`; `rewrite/evidence/A14-t08-supplementary.md:16-28`). Dropped. |
| `crates/jackin-session/tests/plan/static_files.rs` | REJECTED-with-reason | The unavailable-tuple refusal case for `Agent::KimiCode, Provider::Kimi` is swapped for `Agent::MimoCode, Provider::Mimo` and the expected reason changes to `native login has no refresh grant`. Swapping the T08 refusal away is promotion-coupled (the promotion is held, `rewrite/decisions.tsv:510-511,514`), and the substituted Mimo case adds nothing that `main` does not already assert. Dropped. |
| `crates/jackin-session/tests/plan/tuple.rs` | REJECTED-with-reason | Rewrites `unavailable_native_lead_checks_credential_before_availability` into `native_lead_requires_native_credential_and_plans`, turning the T08 `PlanError::TupleNotSupported` refusal into a successful-plan assertion. The plan only succeeds when T08 is `Supported` (`rewrite/decisions.tsv:510-511,514`; `rewrite/evidence/A14-t08-supplementary.md:16-28`). Dropped; the refusal test stays in `main`. |
| `crates/jackin-tui/tests/launch/tuples.rs` | REJECTED-with-reason | Removes the T08 entry (`ConsoleAgent::KimiCode` / `ConsoleProvider::Moonshot`, reason `native refresh and final live proof pending`) from `unavailable_and_blocked_tuples_retain_exact_reasons_without_effects`, leaving only the Claude/Anthropic case. Dropping the T08 refusal is promotion-coupled (held, `rewrite/decisions.tsv:510-511,514`). Dropped; the refusal stays in `main`. |
| `crates/jackin/tests/cli/load.rs` | REJECTED-with-reason | Three coupled changes: the `t08_load_refusal` entry is deleted from `unavailable_tuple_tests!`, the `tuple_account_config` helper grows the Kimi host-login credential fixture (`kimi-home/credentials/kimi-code.json`, `FAKE-KIMI-ACCESS` / `FAKE-KIMI-REFRESH`) and takes an `Agent` argument, and T08 is added to the supported-tuple launch list. The fixture and the supported-list entry only make sense once T08 is `Supported` (held, `rewrite/decisions.tsv:510-511,514`). Dropped; no Rust file differs from `main`. |
| `crates/jackin/tests/cli/account_add.rs` | REJECTED-with-reason | Commit `72401bf5` replaces `assert!(world.gateway.provider(&provider).is_none())` with an assertion that the gateway provider **exists** and carries `FAKE-KIMI-ACCESS` / `FAKE-KIMI-REFRESH`. While T08 is `Unavailable` the native `--from-host-login` kimi account is not publishable, so this assertion fails on `main`. It depends on the promotion, which is held (`rewrite/decisions.tsv:510-511,514`; `rewrite/evidence/A14-t08-supplementary.md:16-28`). Dropped; the pre-promotion assertions stay in `main`. |
| `crates/xtask/tests/repo_gates.rs` | REJECTED-with-reason | The `promotion_records` gate is updated for the promotion: `A14-t08-model-live.md` is added to the required evidence files and `T08-kimi-code.md` is moved from the required-blockers list to the promoted-blockers list (must not exist). This is the promotion-record update; the promotion is held (`rewrite/decisions.tsv:510-511,514`), so the blocker must stay and the gate must keep requiring it. Dropped; the gate file is byte-identical to `main`. |
| `rewrite/blockers/T08-kimi-code.md` (deleted by the PR) | REJECTED-with-reason | The PR deletes the blocker. The blocker is the on-disk record of the hold: it names the Kimi weekly quota as the exact missing input and lists the unproven legs, and `rewrite/decisions.tsv:514` records its restoration after a freshness-recheck commit deleted it while the tuple stays unavailable. `crates/xtask/tests/repo_gates.rs` on `main` requires the file to exist. The deletion is promotion-coupled, so the file stays on `main` unchanged. |
| `rewrite/evidence/A14-t08-model-live.md` (new, +188) | VALUABLE | The factual live-proof record of the 2026-11-10 one-shot lane: native Kimi Code 2.5.2 answered the exact prompts and completed a real tool call on `jackin-docker` and `jackin-local`, with the placeholder, supervision, containment, export, host-invariance and guarded-purge checks recorded. It is **absent from `main`** (`git ls-tree origin/main rewrite/evidence/` lists `A14-t08-live.md` and `A14-t08-supplementary.md` but not `A14-t08-model-live.md`), so closing #1283 with the branch deleted would have destroyed the only copy of this proof. Restored here, with the wording corrected so it states that the promotion remains **pending the supplementary legs** and cites `rewrite/evidence/A14-t08-supplementary.md` and `rewrite/decisions.tsv:510-511,514`. |

## Counts

- VALUABLE: 1 (`rewrite/evidence/A14-t08-model-live.md`)
- CONTAINED-proven: 0
- DEBRIS: 0
- REJECTED-with-reason: 11 (the 10 Rust files and the blocker deletion)

No file of the 12 is CONTAINED-proven: the source of the classified diff is the closed PR #1283 ref `refs/pull/1283/head` (head `72401bf5e78c630f6fc2765e6391bb9ff7188635`), which GitHub keeps even though the branch `chore/dirty-slice-1` was deleted, so a fresh clone reproduces it with no local ref at all: `git fetch origin refs/pull/1283/head:refs/pull/1283/head` and then `git diff --name-status $(git merge-base origin/main refs/pull/1283/head) refs/pull/1283/head -- <path>` is non-empty for all 11 changed paths, and the one added path (`rewrite/evidence/A14-t08-model-live.md`) is absent from `origin/main`. No file of the 12 is DEBRIS: all 11 changed paths carry product code, gate code, or a live-evidence record, and the added path is a reviewable evidence record, not scratch output.

## What this branch restores

The branch `chore/restore-1283-remainder` carries exactly three files against `origin/main`:

1. `rewrite/evidence/A14-t08-model-live.md` — the restored and corrected live proof record.
2. `rewrite/triage/pr-1283-disposition-2026-11-14.md` — this disposition.
3. `rewrite/decisions.tsv` — one added row recording the standing rule that a PR carrying unmerged valuable commits must not be closed with its branch deleted without either a replacement PR or a recorded per-file disposition.

No Rust file differs from `main`.
