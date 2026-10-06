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
| v8 | Launcher `45291b393424ab976f8181b8001e08ce3210014d0666ced45d179db7705a37ca`; timer `40c60d10c411d0c8fae2bdd7c3c1449ae7c9d29520f44cdafc71ae9239c0f554`; diff `d0271787510a02fb9dc44346d3e3494e2df8c3f206dbe0f4ee3f365ce27b77c2`. | SECURITY+METHOD PASS; RECOVERY PASS; COLD-2/3 PASS | Cold-2 exited 0 (inner 91.055 s, outer 91.287 s): 0 hits/misses, 1,086 unconsulted, 55 bypass rows, 580.462 ms permit wait, 90.466 ms flight wait. Cold-3 exited 0 (inner 98.997 s, outer 99.223 s): 0 hits/misses, 1,086 unconsulted, 616.495 ms permit wait, 104.307 ms flight wait. Host-build overlap with cold-2 is unproven; cold-3 snapshots showed no compiler but elevated load and heavy Codex RSS. |

The v5 prepare phase used a rootfs-only `cc` symlink. Its guard failure is not a compiler result. Do not rerun v5.

The v6 cold-1 inner build completed, but its collector failed before cold-2. Cargo 1.97 wrote one HTML output with `st_nlink=2`: canonical and timestamped names share a hard link. The file was owner 65534, mode `0600`, and 540,739 bytes. The owner salvaged MBX statistics through a validated no-follow directory descriptor.

The partial statistics report zero hits, zero misses, 1,086 unconsulted commands, 55 bypass rows, 596,324,893 ns permit wait, 90,716,606 ns flight wait, and 4.27 GB stored. Read-only v8 recovery preserved these same values. This is not cache reuse or a repeated scenario. Do not count it as a valid baseline or treat a warm target as cold.

The original cold-1 observation was contended: the method reviewer found unrelated Rust compiler processes active in the host snapshot. Its inner build duration is a single contended observation, not an uncontended median. v7's authorized recovery attempt did not reach the read-only collector because its self-reexec size guard rejected the 70,659-byte launcher; v8 recovered the original evidence without running Cargo. Cold-2's after-snapshot observed external rustc PID 2335295 at 1.52 GiB, but its start time was not captured, so overlap with the last build milliseconds is unproven. A later UID0 Cargo process had cwd `/tmp/velnor-tofu-exact-cache-admission` and exited before cold-3. Cold-3 snapshots showed no cargo/rustc/compiler process, but host load was 20.72 to 25.76 during the run and Codex RSS was high. The three inner durations were 90.373, 91.055, and 98.997 s (about 9.5% span); all reported 0 MBX hits/misses. These are not cache-reuse results and do not yet justify a performance claim. Do not rerun the now-populated cold-1 target as cold. Continue only under the reviewed sequence and exclusive heavy-window coordination.

### Cold-1 critical path and unchanged-target probe

A read-only parse of the recovered Cargo timing graph on source `0aa821a088e1bacf3d4d85a4c9faaa67faa85132` found the late dependency chain `pastey → turso_core → turso_sdk_kit → turso_sync_sdk_kit → turso → jackin-usage` metadata → `jackin-runtime` metadata → Jackin library → requested binaries. `turso_core` occupied 27.85 s from 46.24 to 74.09 s; the `jackin` binary finished at 90.11 s. These are intervals on Cargo's elapsed dependency graph, not summed CPU or linker times. The largest isolated unit was the `aws-lc-sys` build-script run at 44.48 s (9.48–53.96), before the late dependency chain. `termrock` took 15.78 s. This `-p jackin` build did not include the `jackin-usage-ffi` unit or desktop frameworks; two `cdylib` bypasses have no package IDs, so they do not establish FFI workload. Evidence: HTML `ef933ce66183a18a84b824b9526681e5deef15961569fc4eea1c96ea88e1f240`, stats `00bd89e1913d557816bde5c1a419201f8b378cece122c02b94fb19455053842f`, bypass TSV `ed100c63677f588b9f90176ea17c742ed75fd1ea867a20067a86e6e7725dc8dc`, timer `84c2088dd6cd8d35cbe25e3c6d12857440d333e3427647a1816ba2a0380b1a99`, and timer log `7f2cd9c60c7af849ffb9b828fab817dfd323bab86cc50b23ba4c3fba3b38773c`. The timing report is one contended observation and must not be projected onto later source heads.

The first unchanged-target probe, `nochange-1`, ran `mise exec --deny-net -- mbx build --locked --offline --timings -p jackin` on the same archived source. Cargo exited 0 after 0.32 s, but the outer timer stopped with validation error `MBX statistics lack scheduler wait fields`; therefore there is no accepted outer duration, and no later no-op phase ran under that first artifact. The target received a new 292,713-byte Cargo timing HTML report; this means the target was modified by report output. MBX stats showed zero lookups/hits/misses/unconsulted, empty compiler/bypass/wrapper maps, and zero stored/downloaded/uploaded/restored bytes. Do not describe physical cache immutability. Evidence: inner JSON `88918a8576c4e415566eac670fabf0b3b81f8d2472a062c88c4046ed8348f0b5`, MBX stats `f894eee317321a6f3cd0181bc9600839bf165f21491ee9bceb80b5f4e0527e84`, HTML `8038c43090f265194c4ad2adc1e5ea7af4586e1eb1e0f933de8ad6ce04205f50`, and timer log `f05a5245454ca6164edf99e2952b2b73a979e64533aeeddbc3cda957cc8a2727`. A source cross-check against MBX `201b9df3d18e8e96831bee631035f6b7c7ae20e0` found scheduler-wait timing is emitted only for wrapper/action events; the no-change session JSONL `a041421e9211ddf3d148de3c867a49e8f8a2aed1f19b60a71749b4ae4961e56d` contains only start/finish events. Missing waits therefore mean “not measured,” not zero. A later reviewed collector records absent waits as null with `scheduler_waits_observed=false`, but this original `nochange-1` event stream has no pre-run inventory and remains excluded. A later `nochange-2` attempt failed its timestamp guard because MBX samples the session filename and start event separately; the observed values differed by 1 ms. Nochange-3/4 did not run under that artifact. V10 corrected the check to bounded timestamp ordering, and nochange-5/6/7 passed the narrow collector gate below. These failures are collector-validation failures after Cargo no-ops, not Cargo build failures.

### Narrow initial-main matrix checkpoint

The exact v8/v9/v10 launcher sequence was reviewed for its specific phases. The baseline-method reviewer granted a narrow PASS for the initial-main observations. They used source `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`, Mise `2026.10.1`, Rust `1.97.1`, and MBX `1.21.0`. This does not accept the full build matrix, a split-benefit claim, or the required post-integration main baseline.

Three cold builds completed at 90.373, 91.055, and 98.997 seconds. They reported zero cache hits and misses. The series is contended. Cold-1 had unrelated compilers active. Cold-2 has an after-snapshot Rust compiler without a captured start time. Cold-3 had elevated host load and heavy Codex processes. The roughly 9.5% span and contention prevent an uncontended timing claim. Cold-1's recovered MBX stats recorded 1,086 unconsulted commands, 55 bypass rows, 596.325 ms permit wait, 90.717 ms flight wait, and 4.27 GB stored. These runs do not prove cache reuse.

The warm-store scenario now has three reviewed fresh-target repetitions on source `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`, using Mise `2026.10.1`, Rust `1.97.1`, and MBX `1.21.0`. Each reported 1,046 MBX lookups, 865 hits, 181 misses, 2 unconsulted commands, and 25 bypasses. Each restored 1,956 files totaling 4,223,186,799 bytes. Each still has an unexplained `aws-lc-sys` build-script miss.

The separate seed invocation completed in 90.657478281 seconds. Its counters show 0 hits, 0 misses, 0 restored files or bytes, and 1,086 unconsulted commands. Do not count it as a warm-cache repetition or evidence that the cache was populated.

| Warm run | Inner / outer seconds | Largest child RSS | Permit / flight wait | Host observation |
|---|---:|---:|---:|---|
| 1 | 24.213847621 / 24.590943702 | 591,460 KiB | 49.924 / 30.648 ms | Five Codex processes above 256 MiB; 1-minute load 13.23 before, 32.56 after. |
| 2 | 17.828313209 / 18.082065420 | 604,368 KiB | 39.833 / 13.819 ms | Five Codex processes above 256 MiB; 1-minute load 22.80 before, 17.48 after. |
| 3 | 17.275419153 / 17.517525327 | 595,468 KiB | 38.222 / 14.546 ms | Five Codex processes above 256 MiB; 1-minute load 12.93 before, 10.36 after. |

Inner duration median/minimum/maximum was 17.828313209 / 17.275419153 / 24.213847621 seconds. Outer duration median/minimum/maximum was 18.082065420 / 17.517525327 / 24.590943702 seconds. Largest-child RSS median/minimum/maximum was 595,468 / 591,460 / 604,368 KiB. RSS is Linux `RUSAGE_CHILDREN` for the largest child with waited-descendant accounting; it is not aggregate build-tree RSS. The snapshots show varying host load and five large Codex processes. Keep these as repeated cache observations, not an uncontended build-performance baseline.

Private evidence remains under `/root/.velnor-work/mbx-review.uXg3BZsJ/`; build logs, stats, and Cargo timing reports remain under `/opt/.jackin-mbx-review.uXg3BZsJ/work/out/`.

- Seed: timer `3b3b27f204a017ae4375e98fe398fb62934ddefff1f83c750dc3d2107bec2153`; stats `c225851ba58a2218a0684f8ae05b4051cb441b0ff75060886be2b1d93eb8fae2`.
- Warm-1: timer `dc6282e4b3782e40ad5f88cd77f27a24b7939febd69285bdf220fc512c2cac22`; stats `7714dba88812528c28c4e9cc921b307f437188429bd9536838cded5ae9069dd1`; log `b0bbbed3434f5c25afd12eedd2e0ac8d3949f9ddac93e7094fb73aeea33999a3`; HTML `896fa9d349855eb062fe60c6664c566a02dc7f2d88e1c2383a3681ca848ed442`.
- Warm-2: timer `5b0fa9e198c39fc726d17d45f6ec16ccbc8af1ea83fde41907a6557853752e44`; stats `d8ed994f2987a96fb959dbf666a0dfba4eaa8e65717ff6fa8f16d79c1fdde85a`; log `a73affd506e15b81de06e9dd8ad625b17fa9d8d4e97295a34c98117440cb549b`; HTML `384fcde045c9630783bcb1c6dfae09a7a9fe5c8cee7c5668b6514794628777b8`.
- Warm-3: timer `ad9ce2643db04bdd8fa4a7b1af8b8259ddbfb037b15731477aeb5e899e007429`; stats `529d6a807fda530214e959b7b4f1bec597d2678df48f5be84b5e41705b2de112`; log `b0c2624b603a5a1336401c1dbf62f030768a87d4e1f0d7759f426fa9e4abcdd6`; HTML `c7677fa60d0767ec0448524df66bc5b7133b3478dffc3ef257928cfb08bbd261`.

The seed timer is `timed-warm-seed.json`. The three repeat timers are `timed-warm-1.json`, `timed-warm-2.json`, and `timed-warm-3.json`. Each timer includes before/after load, memory, PSI, and heavy-process snapshots. All three warm runs had five Codex processes above 256 MiB, but their host load differed. The method reviewer checked the exact timer contents and hashes. The RSS measure is per-run largest-child RSS; it does not report aggregate process-tree memory.

The three validated no-op runs used the same source and existing target. Their inner/outer durations were `0.888005191/1.136232634 s`, `0.694005454/1.273673344 s`, and `0.697539470/1.086162425 s`; the medians are `0.697539470 s` inner and `1.136232634 s` outer. Each run had zero compiler actions and zero MBX lookups, hits, or misses. Scheduler wait fields were absent and are represented as unobserved, not zero. Cargo wrote a new timing HTML report on each run, so the target was not physically unchanged. Host snapshots show five to seven heavy Codex processes, a `zizmor` process during nochange-6, and a Rust compiler during nochange-7; the last run's load rose from 33.01 to 42.86. Treat these values as narrow no-op collection evidence, not a performance baseline.

Private timer records remain under `/root/.velnor-work/mbx-review.uXg3BZsJ/`.

- `nochange-5`: timer `9dd18846f0cc2cb5bd2b64b76ca887e8a8fa17ea1ecd1f3db83999a76ba19956`; stats `d0693f74090e586e8fe7f7aa4a8990d7d8ababc666ce7b06a4fd1937e1c45b9b`.
- `nochange-6`: timer `746c3e22be366508b0fab76a5e027c7445e694635cfc8eca099ae6a506fdee9a`; stats `e8936f89dbd25d71f68a0c4ae73c60f09b724d4cb24eb5e170d53c4211856f2e`.
- `nochange-7`: timer `5e818a77d9383ccf6cc3c554584ae64a5ee74c554b741f76f923ce6646d1a16b`; stats `32a15f9fc201022c6d52d478113d48d6e2aa746d118ac64f19a05c48994707b5`.

## Required measurements

Every comparable build scenario requires at least three repetitions. Report median, minimum, maximum, peak memory when measurable, timing output, and MBX cache hits, misses, and bypasses. Record object provenance for every MBX run.

### Scenario matrix

| Scenario | Status | Reason |
|---|---|---|
| Empty target with an isolated empty MBX store | NARROW PASS | Three cold observations completed, but contention prevents an uncontended comparable baseline or performance claim. |
| Fresh target with a warm MBX store | NARROW PASS | Three repeated cache observations passed method review; the `aws-lc-sys` miss remains unexplained, and varying host load prevents a performance claim. |
| Unchanged source with an existing target | NARROW PASS | Three no-op runs passed the collector gate; contention and timing-report writes prevent a performance or immutable-target claim. |
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
