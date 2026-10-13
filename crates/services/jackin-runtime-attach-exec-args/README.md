# jackin-runtime-attach-exec-args

Role exec arg
builders.

## What this crate owns

- Args
  (`exec_args`):
  `insert_run_as_user`,
  `host_alt_screen_exec_flag`,
  `set_role_terminal_title` —
  exec argv + title
  shaping.
- Policy
  (`exec_args`):
  `git_policy_env_pairs` —
  git toggle env
  pairs, single
  source of truth.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`exec_args.rs`](src/exec_args.rs) | arg builders | hub `attach` suite |

## Public API

`exec_args::git_policy_env_pairs`,
`exec_args::host_alt_screen_exec_flag`,
`exec_args::insert_run_as_user`,
`exec_args::set_role_terminal_title`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-attach-exec-args
cargo clippy -p jackin-runtime-attach-exec-args --all-targets -- -D warnings
```
