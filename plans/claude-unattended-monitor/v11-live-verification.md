# v11 live verification

**Result: not ready for handoff.** The v11 collector completed two natural coordinator generations after the restart floor. Weekly evidence is current: the provider reports 8% remaining (92% used by complement), and the normalized projection locally sets `quota_state` to `exhausted`. The monitor still lacks current five-hour evidence: its source projection has a fresh `Session` group with 100% remaining but no reset epoch, while the monitor retains the old 54% observation and marks it stale with the reset due.

This is live evidence from the installed v11 CLI, the existing v7 state, the coordinator envelope, and root’s bounded watch capture. No fixture data was used for these observations. The full monitor status payloads are included in the JSON artifact with identity fields redacted; full raw status and projection captures remain private. Command strings in the artifact are redacted evidence templates, not directly executable verified examples. Monetary amounts are omitted from both public artifacts.

## Operator authorization and cutover

The operator authorized the replacement with: “Replace your v10 broker with v11; I’m present to approve Keychain access.” Root reported that the v10 broker returned `service_stopped`, exit 0, and its terminal ended normally with exit code 0, without Ctrl+C. Root then reported that the attended v11 bootstrap returned `service_ready`, using the matched default Claude Code Keychain source; processes 3127 and 3128 remained active.

This verifier did not access credentials, perform auth, stop a service, issue a provider refresh, or launch a watch. Native credential reads, HTTP request counts, and UI prompt counts were not instrumented; this report does not infer zero activity.

## Natural collection timeline

| Coordinator generation | Phase | Started | Provider invoked | Completed | Success deadline | Result |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| 125 | completed | 1791722934 | 1791722935 | 1791722936 | 1791723236 | Pre-cutover last-good generation |
| 126 | completed | 1791723522 | 1791723523 | 1791723524 | 1791723824 | First natural post-restart collection |
| 127 | completed | 1791723824 | 1791723825 | 1791723826 | 1791724126 | Second natural collection |

The second generation started 300 seconds after the first completed and its provider invocation was 302 seconds after the first invocation. These are two observed coordinator generations, not a measured HTTP request count. The coordinator enforces a 300-second Claude minimum-attempt interval and installs a monotonic recovery fence on reload; the exact in-process fence is not persisted as a provider invocation timestamp.

Root’s bounded watch used a 300-second timeout, exited 0 after 300.059 seconds, and emitted 16 events (sequences 173–188). Sequence 183 at epoch 1791723824 showed provider refresh in progress. Sequence 185 at 1791723826 showed current provider and weekly evidence sourced at 1791723825 and received at 1791723826; five-hour evidence remained stale. The private watch capture is `1791723608-watch.jsonl`.

A three-read passive status burst before the first due time took 0.16 seconds. All three status commands returned exit 2 with the same stale status, and the coordinator remained at generation 125 with no provider invocation timestamp change. This establishes three CLI status reads and zero coordinator-invocation delta during that burst only; it does not measure HTTP, native credential, or UI activity.

## Provider projection and monitor status

| Capture | Provider projection | Session (provider-defined) | All models (weekly) |
| --- | --- | --- | --- |
| First natural, epoch 1791723524 | Account generation 126, current | Remaining 100% (raw 100); used fields absent; reset absent; observed/fetched/last success 1791723523 | Remaining 8%; 92% used by complement; reset 1792134000; observed 1791723523, received by monitor 1791723524 |
| Second natural, epoch 1791723826 | Account generation 127, current | Remaining 100% (raw 100); used fields absent; reset absent; observed/fetched/last success 1791723825 | Remaining 8%; 92% used by complement; reset 1792134000; observed 1791723825, received by monitor 1791723826 |

The 92% weekly “used” value is both present directly in monitor evidence as 9200 basis points and derivable from the provider’s 8% remaining value. For `Session`, the provider projection contains only the two remaining fields and no reset epoch. This report does not convert that into a verified five-hour used value or claim a reset occurred.

In the post-generation monitor-status capture at epoch 1791724056, monitor schema 6 remained `needs_evidence`. Readiness was tracking `waiting`, provider `current`, quota `stale`, budget `unknown`, and dispatch `not_authorized`; `runnable` was false and the CLI returned exit 2 because of the monitor’s non-runnable readiness. Its five-hour fields still showed 54% used from source `broker_projection`, evidence and receipt epoch 1791722633, age 1423 seconds, reset epoch 1791723000 with reset validity `due`. Its weekly fields showed 92% used from `broker_projection`, source epoch 1791723825, receipt epoch 1791723826, age 231 seconds, reset epoch 1792134000, freshness `current`. Issues were `quota_stale` and `reset_due_unverified`.

The public JSON contains the complete sanitized status replies, including scope, each evidence source and age, issues, latest decision, readiness, runnable state, and exit code. Account/session/goal identities and opaque quota-window IDs are redacted. Status budget, cumulative goal spend, and period spend baseline are null.

## Spend metadata

The current projection’s typed `Extra usage spend` group has `cap`, `spent`, and `remaining` fields. Each carries currency `SGD` and exponent `2`. The public JSON records only field presence, currency, and exponent; it omits every monetary amount. This usage telemetry is not billing authority.

## Acceptance and evidence locations

Acceptance remains **false**. The provider reports 8% weekly remaining, while the normalized projection locally sets `quota_state` to `exhausted`; the monitor’s five-hour evidence is stale after its reset time. A fresh `Session` projection with no reset epoch did not update the monitor’s five-hour fields. The cause and expected contract are under contract and security review. This record documents the observation and does not characterize the behavior as correct.

Private raw captures are under `~/.local/share/jackin-claude-monitor-v11/private-evidence` (directory mode 0700, files mode 0600). The public sanitized record is [v11-live-verification.json](./v11-live-verification.json). No source files or credentials were changed or accessed by this verifier.
