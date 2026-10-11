# jackin-runtime-launch-load-options

Caller-supplied
`jackin load` options
bag and validation.

## What this crate owns

- Options
  (`load_options`):
  `LoadOptions`,
  `LaunchedInstance`,
  `IdentitySink`,
  `LoadOptionsError` —
  the launch decisions
  bag, its programmatic
  validation, and the
  identity a launch
  reports back.
- Lane env
  (`lane_env`):
  `lane_agent_env` +
  model/effort env
  consts — container
  env pinning the
  launched agent's
  model and reasoning
  effort.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`load_options.rs`](src/load_options.rs) | options + validation | hub `launch::programmatic` suite (`case_01`, `case_02`) + hub `launch` suite |
| [`lane_env.rs`](src/lane_env.rs) | lane env mapping | hub `launch::programmatic` suite (`case_02`) |

## Public API

`load_options::LoadOptions`,
`load_options::LaunchedInstance`,
`load_options::IdentitySink`,
`load_options::LoadOptionsError`,
`lane_env::lane_agent_env`,
`lane_env::CODEX_LANE_MODEL_ENV`,
`lane_env::CODEX_LANE_EFFORT_ENV`,
`lane_env::CLAUDE_MODEL_ENV`,
`lane_env::CLAUDE_EFFORT_ENV`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-launch-load-options
cargo clippy -p jackin-runtime-launch-load-options --all-targets -- -D warnings
```
