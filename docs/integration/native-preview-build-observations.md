# Native preview build observations

Archive builds report optional numeric host observations on stderr while Cargo
keeps its existing JSON stdout file, arguments, selected profile, parallelism and inherited SDK process group. The observer cannot authorize an
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

The dependency-seed authority contract reads local composite action metadata
through one function-local `clippy::disallowed_methods` expectation. This bounded
filesystem work runs synchronously on the CI integration-test thread, never on a
runtime or rendering thread. `File::open` with
`Read::take(MAX_ACTION_METADATA_BYTES + 1)` bounds oversized and growing reads.
The reviewed `clippy::disallowed_methods@jackin-xtask` ratchet bound increases
from 11 to 12 for this one site; all other budgets and the runtime/render
blocking prohibition remain unchanged.

## Preview optimization tradeoff

Native preview leaves explicitly select Cargo's `preview` profile, which inherits
`release` with `lto = "off"`. Stable `release-archives` always selects `release`,
even for a preview-looking version, and `desktop-release` is unchanged. This is
an intentional optimization tradeoff: disabling LTO can reduce build work but
increase executable size or reduce runtime performance. No speedup, runtime
performance equivalence or hosted capacity claim follows from the selection.
Existing size limits and runtime/security admission remain mandatory.

A closed typed layout supplies Cargo's selector, target and host output roots,
artifact-record path, packaging and SBOM custody. Neither archive names nor
version strings choose a profile. Cross-profile, mixed host/target, traversal
and descendant symlink records are rejected; trusted root aliases resolve once.
Both `CARGO_PROFILE_RELEASE_CODEGEN_UNITS` and
`CARGO_PROFILE_PREVIEW_CODEGEN_UNITS` are removed from archive build children,
so neither profile inherits the retired whole-graph override. Other profile
environment settings retain their existing semantics; native execution supplies
its qualified, closed environment. Jobs remain three and producer limits twenty
minutes. Assembly consumes admitted archive/SBOM bytes without compilation.

Acceptance still requires all six actual cold hosted payloads, every declared
binary, archive/SBOM/native verification and publication gates. If the measured
candidate does not fit, stop and reassess evidence rather than automatically
tuning more parameters or extending deadlines.
