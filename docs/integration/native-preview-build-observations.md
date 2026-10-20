# Native preview build observations

Archive builds report optional numeric host observations on stderr while Cargo
keeps its existing JSON stdout file, arguments, environment, release profile,
parallelism and inherited SDK process group. The observer cannot authorize an
artifact, change the build result, install tools, retry a task or extend the
producer deadline. The generic stdout-file helper used by authenticated GitHub
downloads is not observed.

A fixed absolute OS `ps` reader is an optional host diagnostic facility, not a
new qualified producer tool. Configured Rust, Zig and cargo-zigbuild identities
still require the native qualified-tool inventory. Diagnostic readings never
influence tool selection, archive bytes, SBOMs, receipts or native admission.
The helper has a clean locale-only environment and inherits the SDK group;
it does not use PATH, a shell, privilege elevation or untrusted command text.

The observer emits at most 64 lines and 16 KiB, including prefixes, newlines and
one truncation marker. It samples no more often than every 20 seconds. Each
sample has one 500 ms work-and-cleanup budget across all reader calls. Errors,
process disappearance and unsupported hosts produce `stats=unknown`; the
observer never turns them into a Cargo failure. Scheduling or kernel stalls
cannot be arbitrarily preempted. A killed helper that cannot reap within its
bounded cleanup remains under the inherited SDK process-group authority.

Global numeric PID/parent relations are used only in bounded memory. Selected
Cargo descendants are checked again for ancestry and start identity. Raw
process rows, paths, arguments, environments and foreign-process observations
are never printed. Only fixed tool-family roles and numeric aggregates leave
the observer. Role-array order is Cargo, rustc, rustdoc, linker,
compiler-driver, other. `ld`/`lld` are linker roles; `clang`/`cc`/`zig` are broad
compiler-driver roles and do not establish a linking phase.

`alive_cpu_ms` is sampled cumulative CPU time for processes still present, not
lifetime CPU, utilization or proof of an exact compiler phase. Rustc CPU can
include LLVM and LTO work. Short-lived children may be missed. Summed RSS may
double-count shared pages; maximum RSS is the largest observed single process.
Available parallelism is the OS-reported process availability, not a proven
physical core count. Readings and start identities have platform-dependent
resolution and are not authentication evidence.

Existing Cargo timing reports and compiler-artifact JSON remain unchanged.
A `Compiling` message indicates a start, not a completion. No hosted capacity,
all-target linking, FD peak, memory safety or publication success is established
merely by adding diagnostics. Those require the actual native build and release
evidence. Producer timeouts remain unchanged.

The three function-local blocking-method expectations are intentional synchronous
host-side xtask exceptions: archive wait polling, bounded OS-reader pipe polling,
and bounded held-child cleanup. They do not run on runtime or rendering threads.
The reviewed `clippy::disallowed_methods@jackin-xtask` ratchet bound increases from
8 to 11 for these sites, as the ratchet policy permits intentional reviewed
budget changes. Their local reasons and the global runtime/render blocking ban
remain in place; no alternative sleep spelling or broader suppression is used.
