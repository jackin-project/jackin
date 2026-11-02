# P3 fix round, writer C: xtask container and shell rules

The orchestrator folds this record into `rewrite/API.md` section 9 (Rules and Test plan outline). It replaces nothing in the existing rules; it adds to the `containers` and `comments` bullets.

## Rules

- `containers` command aliases. `use std::process::Command as X;` and `use tokio::process::Command as X;` make `X::new(...)` checked exactly like `Command::new(...)`: a literal `docker`, `podman`, or `container` program is flagged in every crate, and a program that is not one string literal is flagged outside `crates/xtask/`. The step reads the use trees of the file, including nested groups (`use std::{process::{Command as X}};`). `as _` is ignored. The `use` line itself is never flagged. An alias defined in another file is not tracked.
- `containers` program type aliases. `type X = Program;`, `type X = jackin_core::Program;`, and `type X = crate::Program;` are flagged outside `jackin-image`, `jackin-core`, and `jackin-testkit`, because the alias hides `X::Docker(sub)`. `type X = DockerSubcommand;` is flagged once, by the existing `DockerSubcommand` rule. `type X = ProgramRun;` and `type X = Vec<ProgramSpec>;` pass.
- `containers` function values. `Command::new` (or an alias `X::new`) that is not followed by `(` is flagged outside `crates/xtask/`, in every other crate including `jackin-image`, `jackin-core`, and `jackin-testkit`. `let f = Command::new;` and `.map(Command::new)` can hide a computed program.
- `comments` shell lexer. `$((` ... `))` is arithmetic, with nested parentheses, and is never a heredoc: `x=$((1<<2))` does not open a heredoc with delimiter `2`. A real `<<EOF` heredoc still hides its body.

## Messages

- Alias or value: the message of `Command::new` is reused for an alias call. The value message is `Command::new is used as a value; a function value can hide a computed program (D01)`.
- Type alias: `` `type X = Program` hides Program::Docker `` followed by the standard `outside jackin-image, jackin-core, and jackin-testkit; only jackin-image starts docker (D01)` suffix.

## Tests

`crates/xtask/tests/repo_gates.rs`: `containers::command_aliases_are_checked_like_command_new`, `containers::type_aliases_of_program_are_reported_outside_the_allowed_crates`, `containers::command_new_used_as_a_value_is_reported`, `comments::shell_arithmetic_shift_is_not_a_heredoc`. The look-alikes in `containers::permitted_docker_use_and_look_alikes_pass` gained `use std::process::Command as Cmd;`, `Cmd::new("git")`, `type Run = ProgramRun;`, `type P2 = Program;` (image), and `pub type Alias = Program;` (core).

## Deviation from the approved plan

T-C1 listed `C::new("docker")` in `jackin-image` as ok. The existing rule flags a literal `docker` program in every crate, including `jackin-image` (`docker_started_outside_jackin_image_is_reported`), because `Program::Docker` is the only way jackin-image starts docker. An alias is checked exactly like `Command::new`, so the alias line is flagged too. The test expects `jackin-image/src/b.rs` lines 3, 4, and 5.

## Residual risks

- A Command alias defined in another file, or by `type X = Command;`, is not tracked.
- A `use` of a non-process `Command as X` (for example `clap::Command`) is not an alias.
- Bash `(( ... ))` arithmetic commands are not covered by the `$((` fix.
