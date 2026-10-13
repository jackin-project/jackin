# TASK-002 decision review

**Verdict: ACCEPT** for these two decisions only.

This review accepts `task-002-limited-output-decision.md` and `task-002-lockfile-decision.md`. It leaves the integration branch, policy tip `fba69d60c2373156dc0dcecee8e085c83776966f`, commit `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7`, and source edits unapproved.

Reviewer did not write either decision. Repository `/Users/donbeave/Projects/jackin-project/jackin` was read with git only. `git rev-parse HEAD` was `1c3dd1be4a3b1ed518133ec3899c084c792370ff`. The clone was not edited. No commit. No `cargo`.

## Limited-output decision

Stated verdict: **REJECT**.

Re-check: **REJECT stands.**

> `read_limited_output` fails when the next read would pass `limit`. `run_limited_output` turns that into a capture error and kills the child. Policy `read_captured` already does that for stdout and stderr, using `output_limits` when set and `DEFAULT_CAPTURE_LIMIT` (16 MiB) when unset, and `run_once_async` already reaps the child on that error. Main has the same helper at the pre-move path. The snapshot names are absent; the bound is present.

`fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` is a one-parent commit, subject `WIP: preserve unresolved Jackin snapshot`, parent `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. `git cat-file -t` reports `commit`. `git merge-base --is-ancestor` against main and against policy both exit 1. `git cat-file -t 2990df17e25f30afca84804d9c402abc1ce00231` fails; that object is absent.

`git grep` for `fn read_limited_output` and `fn run_limited_output` on `1c3dd1be4a3b1ed518133ec3899c084c792370ff` and `fba69d60c2373156dc0dcecee8e085c83776966f` returns no matches. `git diff --stat --find-renames` between those commits for the process library is a rename with 0 line changes:

```text
crates/{ => core}/jackin-process/src/lib.rs | 0
```

The two file texts are identical, 607 lines. Policy `read_captured` is lines 381–405 and bails with `captured {name} exceeded byte limit {limit}` before extending the buffer. `DEFAULT_CAPTURE_LIMIT` at line 17 is `16 * 1024 * 1024`. `run_once_async` lines 453–462 pass `request.stdout_limit.unwrap_or(DEFAULT_CAPTURE_LIMIT)` and the same for stderr. On that error, lines 484–486 call `owned.kill_and_reap()` and return `Err(error)`. `output_limits` at lines 177–179 stores both limits. `exec_sync` at lines 274–295 blocks on `exec_async`. Policy tests in `crates/core/jackin-process/src/tests/case_01.rs` lines 261–270 and 276–294, and main `crates/jackin-process/src/tests.rs` lines 274 and 296, match the cited assertions. `cap_output` is the later 1 MiB string truncation at the cited capsule lines.

On `fea3ade`, the ours side between lines 650 and 651 is empty. The incoming end marker is line 883, inside `read_limited_output`, so the stored function text does not close cleanly. The bounded gate is lines 451–459 and the call is lines 557–559. The other incoming reader, `read_output` at lines 676–686, uses `read_to_end` with no byte cap. Policy still applies the 16 MiB default when a limit is unset.

## Lockfile decision

The note does not print the word PORT. Its operative verdict is **PORT** of minisign provenance and the hyperfine pin, and **REJECT** of copying P's root `mise.lock` unchanged.

Re-check: **that verdict stands.**

> Integration must restore minisign provenance and the hyperfine pin. P's current lock does not record an equivalent of either.
>
> Do not copy P's `mise.lock` (`90d4e8356371a6eb2cc41d6479bed45369fac413`) as-is.

`git rev-parse <tip>:mise.lock` and `<tip>:mise.toml` match the note's blob table: main `51ff344e2d632574bbfc0987e1e4b6698e6a2b8e` / `3f26ba4dba1050675702d42338fe1e4535f9bd7e`; policy `90d4e8356371a6eb2cc41d6479bed45369fac413` / `3eca6dca0a51373a54bb805b870b5aaed70e5e1f`.

Main `mise.lock` provenance lines are `github-attestations` 45, `cosign` 9, and `minisign` 2. Both minisign rows are Zig 0.16.0:

```text
976|[tools.zig."platforms.linux-x64"]
979|provenance = "minisign"
993|[tools.zig."platforms.macos-arm64"]
996|provenance = "minisign"
```

The other seven Zig platform keys have `checksum` and `url` only. The tool header is lines 964–966: `version = "0.16.0"`, `backend = "core:zig"`. `mise.toml` line 45 is `zig = "0.16.0"`.

Policy Zig is `0.17.0` at lines 932–935, and `mise.toml` line 42 is `zig = "0.17.0"`. All nine platform keys stop at `checksum` and `url`. The string `minisign` occurs zero times. Policy provenance lines are `github-attestations` 45 and `cosign` 18, and none of those keys are on `tools.zig`. The macOS arm64 row is lines 961–963, checksum `sha256:b607e9b9234790a008116ae5bdb71c6243b84b9fb42a53a9e70fde41c06c536a`, URL `zig-aarch64-macos-0.17.0.tar.xz`, with no provenance key. Linux x64 is the same two-field shape at lines 945–947. Those checksums differ from the 0.16.0 rows.

Main hyperfine is lines 441–443: `version = "1.20.0"`, `backend = "aqua:sharkdp/hyperfine"`. `mise.toml` line 28 is `hyperfine = "1.20.0"`. Nine platform keys occupy lines 445–488, each with `checksum`, `url`, and `url_api`. Policy has no `[[tools.hyperfine]]` header, no `hyperfine` substring in `mise.lock` or `mise.toml`, and no backend name containing `hyperfine`.

The two mismatched main URL rows match the note:

```text
450|[tools.hyperfine."platforms.linux-arm64-musl"]
452|url = "https://github.com/sharkdp/hyperfine/releases/download/v1.20.0/hyperfine-v1.20.0-aarch64-unknown-linux-gnu.tar.gz"
475|[tools.hyperfine."platforms.macos-arm64"]
477|url = "https://github.com/sharkdp/hyperfine/releases/download/v1.20.0/hyperfine-v1.20.0-x86_64-apple-darwin.tar.gz"
```

`platforms.linux-arm64` at lines 445–447 uses that same GNU checksum and URL. `platforms.macos-x64` and `platforms.macos-x64-baseline` at lines 480–488 use that same Darwin checksum and URL.

`docs/mise.lock` on both tips contains no `hyperfine`, `minisign`, or `zig` text. Main has no `native/mise.lock`. Policy `native/mise.lock` pins `swiftlint` `0.65.1` only.

Restoring the pin means the `1.20.0` / `aqua:sharkdp/hyperfine` row must be present. The selected Zig pin stays `0.17.0`, with `provenance = "minisign"` regenerated for the 0.17.0 archives on `[tools.zig."platforms.linux-x64"]` and `[tools.zig."platforms.macos-arm64"]`.
