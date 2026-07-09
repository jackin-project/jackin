# AGENTS.md — jackin-runtime

Container bootstrap pipeline — the launch orchestrator.

## Hard rules (this crate)

- **Tier & dependencies:** L1 application / orchestration. Allowed workspace deps: `jackin-core`, `jackin-config`, `jackin-env`, `jackin-manifest`, `jackin-docker`, `jackin-image`, `jackin-diagnostics`, `jackin-launch-tui`, `jackin-host`, `jackin-protocol`, `jackin-isolation`, `jackin-instance`, `jackin-tui`, `jackin-build-meta`. This is the integration crate; do not let it grow *new* kinds of responsibility — decompose instead.
- **Keep `README.md` current:** update it when structure, public API, launch phases, or backends change (see `crates/AGENTS.md`).
- **Extract by phase contract, not line count.** Launch/body extraction proceeds by named phases (validation, materialization, trust checks, image, env/auth, Docker run, wait, teardown, attach, cleanup) — see the runtime/launch behavioral spec. Do not refactor by chopping for line-count.
- **Characterization first.** Add/keep fast tests around observable launch behavior before extracting a body. The behavioral spec is the oracle.
- **Move shared ports down.** When lower crates need a port/fake currently living here, move it down rather than adding an upward dev-dependency.

## What lives here vs elsewhere

- This crate owns: the launch pipeline + phases, backend clients, host exec, isolation integration, reactive daemon, wait-for-state.
- Image build lives in `jackin-image`. Isolation materialization lives in `jackin-isolation`. Instance lifecycle lives in `jackin-instance`. Launch *presentation* lives in `jackin-launch-tui`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
