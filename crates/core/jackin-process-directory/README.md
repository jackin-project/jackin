<!-- SPDX-FileCopyrightText: 2026 Alexey Zhokhov -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# jackin-process-directory

Unix working-directory descriptors for existing `std::process::Command` and
Tokio commands. The parent process keeps its cwd. Renaming a repository or
replacing its original pathname cannot redirect a command configured here.

The safe API rejects pathname cwd and non-directory descriptors. Before fork,
it duplicates the descriptor above standard input/output/error with CLOEXEC.
The only unsafe boundary installs a `pre_exec` callback that calls `fchdir`
and converts errno without allocation. Ownership retains the duplicate until
the command drops; exec closes the child's duplicate. Spawn errors propagate,
while the caller retains all timeout, cancellation, and child-reaping duties.

This crate explicitly copies the workspace lint rules with `unsafe_code=deny`,
and expects unsafe code only on the reviewed registration function. All
existing crates retain the workspace `unsafe_code=forbid` policy.

Native Windows has no implementation. Callers must reject unsupported pinned
working directories rather than fall back to a pathname.
