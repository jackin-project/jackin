<!-- SPDX-FileCopyrightText: 2026 Alexey Zhokhov -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# jackin-process-directory

Unix descriptor cwd for std/Tokio commands leaves parent cwd unchanged; renames or pathname replacement cannot redirect it. Reject pathname cwd and non-directory descriptors. Duplicate above fd2 with CLOEXEC; retain until command drop, close on exec. The audited pre_exec callback uses only async-signal-safe fchdir and errno conversion, without allocation. Spawn errors propagate; callers own timeout, cancellation and reaping.

macOS peek_exited_child (P_ALL) and peek_specific_exited_child (positive P_PID) use WEXITED|WNOHANG|WNOWAIT. Neither reaps nor signals; both validate PID, SIGCHLD and terminal status. Hold the shared ownership lock through peek, owner check and selected-PID wait. ACLs and exclusive wait ownership are not checked.

Workspace unsafe_code=deny remains; cwd and waitid each have a function-local reviewed expect. waitid zeroes ABI storage, reads no error output and returns only checked PIDs; no crate-wide relaxation.

No native Windows cwd support: reject unsupported pinned cwd; never fall back to a pathname.
