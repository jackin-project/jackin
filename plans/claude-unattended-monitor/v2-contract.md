# Observer and dispatch policy contract

Contract freeze: 2026-10-10. Implementation and verification are in progress.
These commands must not be presented as installed until binary proof passes.

## Commands

All commands use the matching installed CLI/broker pair and an explicit data
directory. Existing service, auth preparation, doctor, status, refresh, watch,
wait, spend recording and monitor stop remain on the `usage` surface.

```
usage binding confirm --provider claude --account ACCOUNT --operator-label LABEL --confirm
usage policy approve --binding ID --binding-revision N --goal GOAL --policy strict-sgd --budget-sgd 50 --operator-label LABEL --confirm
usage policy approve --binding ID --binding-revision N --goal GOAL --policy quota-only --operator-label LABEL --confirm --acknowledge-no-sgd-cap
usage monitor observe --provider claude --session SESSION --idempotency-key KEY
usage monitor observe --provider claude --binding ID --binding-revision N --idempotency-key KEY
usage monitor start --provider claude --binding ID --binding-revision N --goal GOAL --policy-revision N --idempotency-key KEY
usage statusline ingest --session-only
usage statusline ingest --binding ID --binding-revision N
usage statusline compose --session-only --settings FILE
usage statusline compose --binding ID --binding-revision N --settings FILE
```

Account-bound observers/dispatch guards can optionally filter `--session`;
observers/guards can optionally configure `--expected-model`. Policy approval
may carry `--expected-revision` for optimistic concurrency. `--format json`
and `--data-dir PATH` apply consistently; watch uses `--format jsonl`.

Binding confirmation and policy approval are separate operator commands. All
three standard descriptors must be TTYs before they contact a broker. The
broker also validates explicit operator confirmation, exact scope/revisions,
and the quota-only acknowledgement. This is a same-user local trust boundary,
not cryptographic proof of human presence. Private state and host-only protocol
operations remain protected from Capsule relay callers. No headless approval
bypass flag is provided. Monitor start cannot select or downgrade policy.

Bindings are operator-selected account labels, not authenticated provider IDs.
Scopes are typed: unbound session, or confirmed binding/revision with optional
session filter. Unbound ingress remains in a separate partition and is never
retroactively merged into an account aggregate.

## Protocol and persistence

Broker wire becomes v6 and monitor/store schema becomes v2. `MonitorPurpose`
is `observe_only` or `dispatch_guard`. `MonitorConfig` carries provider,
purpose, scope, optional goal, optional model guard and optional policy revision.
Dispatch guards require account binding, goal and approved policy revision;
observers require neither a goal nor policy and cannot authorize dispatch.

Start has a required durable idempotency key. Same key/config returns the
original record, including after stop; a different config with that key fails.
Failed strict activation does not reserve a key, allocate an ID, or persist a
partial activated goal. Intentional new runs use new keys without resetting
goal spend or account quota pause state.

Binding and policy records are separate from activated goals. Approval records
carry exact scope, operator label, prior/new policy, time, revision and origin.
Quota-only requires explicit acknowledgement and reports disabled spend
enforcement and unknown spend. Strict SGD is fail-closed and requires fresh
compatible operator-attested baseline admission before first activation.
Unsupported/missing policy never means quota-only. Existing strict goals cannot
silently downgrade or lose historical uncertainty.

Statuses retain authoritative `runnable` and separately expose tracking, quota,
budget and dispatch readiness, purpose, scope and policy. Observation-only
dispatch readiness is always not-authorized and `runnable` is always false.
Observer creation succeeds with exit 0 when durable tracking starts, even with
missing quota. Dispatch/status/wait exit 0 only for permitted runnable work,
exit 2 for blocked/degraded states, and exit 3 for unavailable/invalid requests.
Successful passive doctor does not establish usage or spend readiness.

V1 migration is an explicit one-time decoder/converter, not DTO defaults or
compatibility aliases. It preserves counters, records, evidence age, decision
sequences, barriers and cumulative spend exactly. Old effective policies stay
strict with migration provenance, without inventing operator approval or
confirmed bindings. Old absent baseline remains absent; later receipts do not
retroactively make its history known. Unsupported/corrupt state fails closed.
Allocator counters must exceed all persisted identifiers; exhausted counters
fail before insertion. Migration never guesses an unrecorded callback time.
Policy `recorded_at_epoch` is optional: an operator revision records its time,
while a migrated V1 policy reports null because V1 recorded no such time.
Unconfirmed migrated bindings likewise have a null confirmation time. Migrated
policies retain strict history but require a new explicit operator approval
before dispatch. `v1-migrated-` is an internal reserved idempotency-key prefix;
caller starts reject it instead of claiming an old record as a new request.

## Source and decision rules

`claude_code_version` records the bounded optional reported client version,
separate from Jackin schema version. Claimed quota fields before Claude Code
2.1.80 are rejected. Missing version is unknown, not authenticated compatibility.
No metadata field is used to manufacture quota observation time or account ID.

Independent quota/model evidence aging remains conservative. Missing/stale
required fields block dispatch. Repeated identical callbacks, timer reruns,
sibling changes and future reset epochs never renew provider evidence. Reset
and model validity audit must preserve that fact and disclose any conservative
limits. Reset grace and fresh-confirmation guards, weekly/model/spend constraints,
threshold sequences and persisted retry deadlines remain enforced.
Window `reset_validity` reports `unknown`, `future`, or `due` independently of
the reset evidence's age. Status includes `expected_model` and
`model_guard_validity` (`not_configured`, `unknown`, `match`, `mismatch`);
match/mismatch use fresh scoped model evidence. These audit fields never
renew evidence or override `runnable`.

Observation is possible without billing evidence; dispatch under quota-only
requires explicit operator acceptance of narrower protection. Neither mode
claims current data during inactivity, automatic reset confirmation, unknown
extra-usage permission, or a hard per-goal SGD billing cap.
