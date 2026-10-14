# jackin-workflow

Durable host-side workflow run state and recovery (MVP component C5).

The crate owns one fixed workflow's run state in a dedicated Turso file
(`workflow-runs.db`, opened only through
`jackin_usage_store_backend::connect_local`), a fixed stage reducer, a
single active-run lock with an owner epoch, command delivery tracking,
a small effect dispatcher (launch/capture/backup intents with stable
operation IDs), and the recovery table from the C5 plan.

Only the host service applies state transitions. Workers submit
observations; they cannot mark a run accepted. See
`plans/workflows/mvp/components/05-run-state-and-recovery.md`.
