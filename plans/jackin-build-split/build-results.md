# Build Results

- Status: IN PROGRESS
- Main-source proof: one complete reviewed build passed. A later v6 cold-1 inner build passed, but collection failed.
- Comparable performance baseline: NOT RUN.

## Static setup

- Mise version: `2026.10.1`, reported by the environment inventory.
- The [Velnor configuration](../../.velnor/config.toml) sets compiler and test process budgets to four.
- The Rust stack uses MBX as its compile driver.
- The current generated workflow pins MBX `1.21.0`.
- No ambient MBX setup was reported on the host.
- Host inventory reports 96 logical processors, 125 GiB RAM, and 3.5 TiB disk.

These static facts do not establish a comparable build duration or cache hit.

## First main-source attempt

The MBX owner reports one build attempt at source `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. The run used default development profile and features on `x86_64-unknown-linux-gnu`. The launcher SHA was `d31315193da05f29746509b3e395aca1394c6b7160c39b8bb075782c9998c748`.

```sh
/usr/bin/env -i PATH=/usr/bin:/bin /usr/bin/python3 -I /root/.velnor-work/mbx-review.uXg3BZsJ/run-build-direct-linker-timed.py
# The timer verified the launcher hash, then invoked:
/usr/bin/env -i PATH=/usr/sbin:/usr/bin:/sbin:/bin /usr/bin/bash --noprofile --norc /root/.velnor-work/mbx-review.uXg3BZsJ/launcher-direct-linker-v2.sh build
# The launcher ran this inner command:
/tools/mise/bin/mise exec --deny-net -- /usr/bin/bash -euo pipefail -c 'cd /source && exec mbx build --locked --offline -p jackin'
```

The command exited `101` after `1.224` seconds. The private chroot lacked `/etc/alternatives`; `/usr/bin/cc` pointed there. MBX reported four crates attempted, zero hits, zero misses, six bypasses, and `88.9 KiB` of local MBX metadata. It uploaded no compiler outputs. No successful source compilation occurred. Treat the duration as a failed-attempt observation, not a baseline measurement.

The owner reports that `register-rust`, artifact acquisition, `register-MBX`, and locked fetch passed before the build. Timer JSON: `/root/.velnor-work/mbx-review.uXg3BZsJ/build-time-python.json`. Logs and statistics: `/opt/.jackin-mbx-review.uXg3BZsJ/work/out/build.log` and `mbx-stats.json`. No task mount or process remained.

## Compiler image review

The first direct-linker candidate `9fe5f1143587373d0ab5df821ff68e6abcc72280f642bdea986df02d1d0e85d4` failed Sol review before execution. It exports `CC`, `AR`, and Cargo linker variables before a staged environment checker that rejects them. The generated checker changes only during later stage creation.

Sol approved a bounded build-only launcher candidate at hash `9d94884ac525c91f5a2e7c5abb6f068cdb814ef0780097fa840d35b57c212851`. It does not authorize cold-baseline or cache-behavior claims. An independent checker found the read-only host tool inventory outside the private root: it did not mount `/usr`, `/root/.codex`, or `/etc/alternatives`. The checker executed no compiler.

## Reviewed linker-v2 source proof

The exact reviewed launcher `9d94884ac525c91f5a2e7c5abb6f068cdb814ef0780097fa840d35b57c212851` completed one main-source build at `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. The timer JSON's verified mtime is `2026-10-05 05:18:49 +0200`. This is a file timestamp, not an exact process end time. The timer records no wall-clock start or end epoch.

```sh
mise exec --deny-net -- /usr/bin/bash -euo pipefail -c 'cd /source && exec mbx build --locked --offline -p jackin'
```

Mise was `2026.10.1`; MBX was `1.21.0`; Rust and Cargo were `1.97.1`. The build used default development profile and features on `x86_64-unknown-linux-gnu`, with `CARGO_BUILD_JOBS=4`. No tests ran. The command exited `0`.

| Measurement | One-run result | Limit |
|---|---:|---|
| Cargo duration | 118 s | Single run. |
| Monotonic wall time | 118.367 s | Single run. |
| Direct-child maximum RSS | 2,165,908 KiB | Not aggregate process memory. |
| User / system CPU | 335.146 s / 61.318 s | Single run. |
| MBX hits / misses | 0 / 0 | Source and cache were not proved cold. |
| MBX bypasses | 626 | 495 unknown-codegen-option, 109 unportable-native-link, 11 compiler-query, 9 stdin, 2 unsupported-crate-type. |
| MBX unconsulted / permit wait | 6 / 1.0739 s | `--jobs` was unset; MBX used its default logical-processor count. |
| Local MBX metadata | 10,291 bytes | No transfers or restored files. |
| Binary SHA-256 | `e899a8e5f51ebb5f20fce5a379a3f4de4555911549e743ca625efb4a3988c2ac` | Successful output from this run. |

The host had 90 GiB available at the setup snapshot. `CARGO_BUILD_JOBS=4` and `MISE_JOBS=1`. A process sample showed three `rustc` processes. This sample does not establish concurrency limits. No tests were requested. No UID 65534 process or task mount remained afterward.

The Python timer source is `/root/.velnor-work/mbx-review.uXg3BZsJ/run-build-direct-linker-timed.py`, SHA-256 `12ea4630638a2f02145e17384b29f0d51d7554a115cf242a2013e36d181aa7c8`. It checked the launcher hash through an `O_NOFOLLOW` descriptor before launch. Timer result: `/root/.velnor-work/mbx-review.uXg3BZsJ/build-direct-linker-timed.json`, SHA-256 `27f3b2f8e2192755f104eb059daafa5924d49c7dce3fae8d7823209cc474f8a8`.

Cargo log: `/opt/.jackin-mbx-review.uXg3BZsJ/work/out/build-direct-linker.log`, SHA-256 `6992138b1f2bc73873d4322f1d81ee57984a86d37dbd399345d503d0b6913d49`. It reports a dev-profile finish in 1m 58s. MBX report: `/opt/.jackin-mbx-review.uXg3BZsJ/work/out/mbx-stats-direct-linker.json`, SHA-256 `ab8b40981bc1b2e116e7ff00665ccf4cba17e0fa211410fbe20f75a327080a43`. Binary: `/opt/.jackin-mbx-review.uXg3BZsJ/work/target/debug/jackin`, SHA-256 `e899a8e5f51ebb5f20fce5a379a3f4de4555911549e743ca625efb4a3988c2ac`.

The timer, log, and statistics files have file mtimes near `2026-10-05 05:18:49 +0200`. The exact process start and end times are absent. The binary is ELF64 x86-64, owned by `nobody:nogroup`, mode `0700`, and size 483,382,880 bytes. This output contains no `jackin-role` binary.

This run proves the source builds once through the reviewed launcher. It is not a repeated baseline, cold-cache result, cache-reuse result, or split comparison. Investigate the high bypass counts before controlled cache measurements.

Source inspection attributes 109 `unportable-native-link` bypasses to MBX rejecting the explicit non-Clang linker. The 495 `unknown-codegen-option` attribution remains plausible but lacks package-level tracing. Diagnostic candidate `a56a1822bbec56025fc2cd499de3a09be57c888a87ce40ba56eef70e99a3f671` was withdrawn because `mbx explain --last` diagnoses misses, not bypasses. It was not executed. Further runs remain held pending rootfs-only compiler setup, stronger filesystem evidence, and exact Sol review.

## Measurement launcher review sequence

The v3 and v4 reviews produced no build result. The v5 guard stopped before compilation. V6 produced one partial cold-1 result; the measurement matrix remains incomplete.

| Version | Evidence | Result | Scope |
|---|---|---|---|
| v3 | Timing method review passed; security review found intermediate `work/out` symlink traversal. | FAIL | Do not use this launcher. No matrix result. |
| v4 | Launcher `2d1c6ce45fa163b0bfed598af3f8b6ada424c5486654132d7c675520578f27d9`; timer `fbae0a31d6ba3138153ee64398082203f15446593a74d846228d28176cba1e1b`; diff `26c9a2a7c8edc6e0cfc29a06cb5c8cc596b5259c329112504f56715891e7f81b`. | SECURITY FAIL; METHOD NOT RUN | Sol found a FIFO leaf can block on `O_RDONLY` before `fstat`. Ancestor traversal was fixed. No phase ran. |
| v5 | Launcher SHA-256 `460341e26528da42b7aae5be0449a3438868bf0fe21a2588b55bdbaad264a804`; timer SHA-256 `31560b177eb620665dc91ef28c5bf44aac42c6e024223a85f827542d0d675e08`; diff SHA-256 `caa6ecf17631048f2dad8d099bce60565d9915054185ad5ba004bbc0b14416a3`. | REVIEW PASS; GUARD FAIL | Both Sol reviews approved the exact artifacts. `prepare-linker` passed in 102.038 ms for GCC SHA-256 `a23ecab8ff08f09ad8c80602c2c5df7f49e09c25905cb8975902e101bf72635f`. `verify-linker` exited 1 before compilation because its version check expected the target basename before GCC's version text. |
| v6 | Launcher `9040ccad259d81b4c8705c687b10fbe174a0c3ef34612cc1c055672df0d3c856`; timer `d3789bf2bc38616eb0b6adb594ea8c8f4a724f9af40ce973350085cf278776e9`; diff `f75ef4f90c0c842e8120465f75d420890df7ab37173759131532f2e2867f0547`. | REVIEW PASS; PARTIAL RUN | Security and method reviews passed. `verify-linker` passed. Cold-1 inner build exited 0 in 90.373 s; collector failed before cold-2 because Cargo timing HTML names were hard links. |
| v7 | Launcher `f3e467ab103ef81879b072923725d6ae4153fe85bf6b6cbf07103eae2e068baa`; timer `14e82441448fe5ad53fddb0ed63170fccb6e3272f900cab0f80cbd02eba73087`; diff `8062e8fa4b3c41e41b4e9b75e7b6f9093008c554c8fc935b734a00a8e88c0051`. | SECURITY+METHOD PASS; RECOVERY FAIL BEFORE PHASE | Read-only recovery exited 1 after 11.081 ms with `unsafe launcher file`: launcher size 70,659 exceeded its 65,536-byte self-reexec limit. No recovery phase, compilation, target/cache mutation, or mount occurred. |
| v8 | Launcher `45291b393424ab976f8181b8001e08ce3210014d0666ced45d179db7705a37ca`; timer `40c60d10c411d0c8fae2bdd7c3c1449ae7c9d29520f44cdafc71ae9239c0f554`; diff `d0271787510a02fb9dc44346d3e3494e2df8c3f206dbe0f4ee3f365ce27b77c2`. | SECURITY+METHOD PASS; RECOVERY PASS; COLD-2 PASS | One read-only recovery succeeded without Cargo. Cold-2 then exited 0 (inner 91.055 s, outer 91.287 s): 0 hits/misses, 1,086 unconsulted, 55 bypass rows, 580.462 ms permit wait, 90.466 ms flight wait. Host build processes were observed immediately afterward; ownership/overlap is under bounded metadata review, so do not call this uncontended or comparable yet. |

The v5 prepare phase used a rootfs-only `cc` symlink. Its guard failure is not a compiler result. Do not rerun v5.

The v6 cold-1 inner build completed, but its collector failed before cold-2. Cargo 1.97 wrote one HTML output with `st_nlink=2`: canonical and timestamped names share a hard link. The file was owner 65534, mode `0600`, and 540,739 bytes. The owner salvaged MBX statistics through a validated no-follow directory descriptor.

The partial statistics report zero hits, zero misses, 1,086 unconsulted commands, 55 bypass rows, 596,324,893 ns permit wait, 90,716,606 ns flight wait, and 4.27 GB stored. Read-only v8 recovery preserved these same values. This is not cache reuse or a repeated scenario. Do not count it as a valid baseline or treat a warm target as cold.

The original cold-1 observation was contended: the method reviewer found unrelated Rust compiler processes active in the host snapshot. The inner build duration is therefore a single contended observation, not an uncontended median or comparable performance result. v7's authorized recovery attempt did not reach the read-only collector because its self-reexec size guard rejected the 70,659-byte launcher. v8 recovered the original evidence without running Cargo. Cold-2 completed under v8, but the subsequent host snapshot found multiple root-owned Cargo/rustc PIDs; the MBX owner did not stop them and paused before cold-3 pending a bounded process metadata check. No cache hits or misses were observed for cold-2. Do not treat either sample as a cache-reuse result, rerun the now-populated cold-1 target as cold, or compare runs until contention context is resolved. Resume only under the reviewed sequence and exclusive heavy-window coordination.

## Required measurements

Every comparable build scenario requires at least three repetitions. Report median, minimum, maximum, peak memory when measurable, timing output, and MBX cache hits, misses, and bypasses. Record object provenance for every MBX run.

### Scenario matrix

| Scenario | Status | Reason |
|---|---|---|
| Empty target with an isolated empty MBX store | NOT RUN | No comparable cold-cache series exists. |
| Fresh target with a warm MBX store | NOT RUN | No comparable warm-cache series exists. |
| Unchanged source with an existing target | NOT RUN | No comparable target-reuse series exists. |
| Private change in one small crate | NOT RUN | Extraction and security review are pending. |
| Shared account-types change | NOT RUN | Extraction and security review are pending. |
| Codex discovery or authentication change | NOT RUN | Extraction and security review are pending. |
| Usage-provider change | NOT RUN | Extraction and security review are pending. |
| CLI or Console change | NOT RUN | Extraction and security review are pending. |
| Focused tests for the changed crate | NOT RUN | Tests are outside this checkpoint. |
| Workspace verification | NOT RUN | Security review gates compilation. |
| Required release build | NOT RUN | Release design and security review are pending. |
| Two consecutive compatible CI runs | NOT RUN | Coverage review and implementation remain pending. |

Do not set numeric split targets before measurements show build variation. Each scenario above must use the same reporting fields when those fields apply.

The `build_baseline` owner must record commands, elapsed time, resource use, cache state, and output hashes in a later authorized execution phase.

Further cache and performance runs require MBX cache and object provenance. See [reviews](reviews.md). Tests, workspace verification, release builds, post-extraction builds, and valid repeated cache-behavior scenarios remain NOT RUN.
