# Debian Results

Status: Route source review PASS; synthetic CLI checks complete; Jackin runtime route NOT RUN.

## Host

- Host: `bastion`, Debian 13.7, x86_64.
- Kernel: `6.12.94`.
- User: root, UID 0.
- CPU: 96 logical processors.
- Memory: 125 GiB total, 117 GiB available at initial inventory time.
- A repeat at `2026-10-05T02:37:32+02:00` showed 113 GiB available.
- Disk: 3.5 TiB total, 3.5 TiB free at inventory time.
- The read-only host command and repeat output are in [checklist](checklist.md#host-inventory-command).
- Codex CLI reports version `0.160.0` and “Logged in using ChatGPT.”
- `HOME` is `/root`. `CODEX_HOME` is unset.
- Auth-file metadata reports mode `0600`. No auth contents were read or copied.

## Account route

- At initial main `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`, Jackin's Codex discovery targeted `~/.codex`, mapped it to `default-codex`, and ignored `CODEX_HOME`.
- Task commit `0556ce39b1abb9cd6b387583d932e1556ca9dfd4` changes Jackin discovery to honor `CODEX_HOME`; an unset value selects `~/.codex`, while an empty value reports an issue without fallback.
- Follow-up commit `688057f40173d32dda04a55bff1e3868c219710d` updates the explicit `CODEX_HOME` source and injection route. Commit `45fbb65c84e359ea3c577120a80ed7a2fa92cf2a` adds only a test-only scan seam attribute. Exact-source review PASS across these commits; review found no production change in the follow-up.
- The MBX-private `jackin` binary was built from base `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`, before the route commits. It does not validate them. No default Jackin configuration exists.
- Account registration, workspace selection, and launch forwarding are NOT RUN.
- No live role response was requested.

## Synthetic `CODEX_HOME` checks

The route owner reports local CLI path checks in a private synthetic root. These checks cover Codex CLI path handling only. They do not validate Jackin runtime discovery or a provider request.

Read-only identity commands were `readlink -f /root/.local/bin/codex`, `/root/.local/bin/codex --version`, `sha256sum /root/.codex/packages/standalone/releases/0.160.0-x86_64-unknown-linux-musl/bin/codex`, and `stat -c '%s bytes, mode %a, mtime %y' /root/.codex/packages/standalone/releases/0.160.0-x86_64-unknown-linux-musl/bin/codex`. The symlink resolves to `/root/.codex/packages/standalone/releases/0.160.0-x86_64-unknown-linux-musl/bin/codex`. The binary reports `codex-cli 0.160.0`; its SHA-256 is `12eb3e81114588aca3b7998f4f19e8997b056aca08e57a7ca7c8a3ec8c652aad`. Its size is 289,101,384 bytes and its mode is `0755`.

The owner used fixture root `/tmp/jackin-codex-route.sYgjX4` with owner `root:root` and mode `0700`. It created `home`, `home/.codex`, `relative-home`, and `absolute-home` directories with owner `root:root` and mode `0700`. It created `regular-file` with mode `0600`.

The route owner supplied this setup:

```sh
umask 077
fixture=$(mktemp -d -p /tmp jackin-codex-route.XXXXXX)
chmod 700 "$fixture"
mkdir -m 700 "$fixture/home" "$fixture/home/.codex" "$fixture/relative-home" "$fixture/absolute-home"
touch "$fixture/regular-file"
chmod 600 "$fixture/regular-file"
cd "$fixture"

# CODEX_HOME unset
env -i HOME="$fixture/home" PATH=/usr/bin:/bin LANG=C.UTF-8 timeout 10s /root/.local/bin/codex login status
# CODEX_HOME empty
env -i HOME="$fixture/home" PATH=/usr/bin:/bin LANG=C.UTF-8 CODEX_HOME= timeout 10s /root/.local/bin/codex login status
# CODEX_HOME relative
env -i HOME="$fixture/home" PATH=/usr/bin:/bin LANG=C.UTF-8 CODEX_HOME=relative-home timeout 10s /root/.local/bin/codex login status
# CODEX_HOME existing absolute directory
env -i HOME="$fixture/home" PATH=/usr/bin:/bin LANG=C.UTF-8 CODEX_HOME="$fixture/absolute-home" timeout 10s /root/.local/bin/codex login status
# CODEX_HOME missing absolute path
env -i HOME="$fixture/home" PATH=/usr/bin:/bin LANG=C.UTF-8 CODEX_HOME="$fixture/missing" timeout 10s /root/.local/bin/codex login status
# CODEX_HOME regular file
env -i HOME="$fixture/home" PATH=/usr/bin:/bin LANG=C.UTF-8 CODEX_HOME="$fixture/regular-file" timeout 10s /root/.local/bin/codex login status
```

The working directory was `$fixture` for all six cases. Stdout was empty. Stderr paths below use `<fixture>` for the temporary root.

| Case | Result |
|---|---|
| Unset | Exits 1 with `Not logged in`; resolves to `<fixture>/home/.codex`. |
| Empty | Exits 1 with `Not logged in`; resolves to `<fixture>/home/.codex`. |
| Relative `relative-home` | Exits 1 with `Not logged in`; resolves to `<fixture>/relative-home`. |
| Existing absolute `absolute-home` | Accepted; exits 1 with `Not logged in`. |
| Missing absolute `missing` | Reports `CODEX_HOME points to <fixture>/missing, but that path does not exist`. |
| Regular file `regular-file` | Reports `<fixture>/regular-file is not a directory`. |

Valid cases also emitted a benign warning about refusing PATH aliases under `/tmp`.

- The owner used no credentials. The checks made no enrollment, network request, or config write.
- The owner removed the fixture root. `test ! -e /tmp/jackin-codex-route.sYgjX4` passed.
- No separate raw transcript was retained. The commands, fixture, and summarized result record came from `debian_codex_route`.

## Rejected MCP-list preflight

The host-probe owner reports a separate synthetic MCP listing with empty home and config paths. It redirected `CODEX_SQLITE_HOME` to another empty fixture. The original temporary path was not retained.

```sh
set -eu
umask 077
probe=$(mktemp -d -p /tmp codex-mcp-preflight.XXXXXX)
trap 'rm -rf -- "$probe"' EXIT
mkdir -m 700 "$probe/home" "$probe/codex" "$probe/sqlite"
/usr/bin/env -i HOME="$probe/home" CODEX_HOME="$probe/codex" CODEX_SQLITE_HOME="$probe/sqlite" PATH=/usr/bin:/bin LANG=C.UTF-8 /usr/bin/timeout 10s /root/.local/bin/codex mcp list --json --disable plugins
```

The owner reported exit 0 and a JSON configured-server count of 0. The output contained no server names or details. The empty tree remained unchanged. The owner removed logs and temporary files. Sol rejected this command as a general profile preflight because auth-status discovery may contact configured MCP endpoints. The synthetic empty-home result does not validate a real profile. No real profile or model request ran.

The replacement uses schema-verified app-server `config/read` with synthetic empty-home input. Its response returned `system`, `user`, and `sessionFlags` layers. `ConfigReadResponse.layers[].config` is generic JSON. The wrapper must inspect raw `mcp_servers` values and fail closed if configuration is enabled or incomplete. This replacement awaits implementation and independent review.

## Owner

`debian_codex_route` completed static route inspection, source review, and local CLI path checks. Exact-source review PASS covers commits `0556ce39b1abb9cd6b387583d932e1556ca9dfd4`, `688057f40173d32dda04a55bff1e3868c219710d`, and `45fbb65c84e359ea3c577120a80ed7a2fa92cf2a`. Jackin runtime confirmation remains NOT RUN because the available MBX binary predates these changes and no default configuration exists. Live requests remain NOT RUN.

See [crate plan](crate-plan.md) and [reviews](reviews.md).

## CLI model/effort change: local command-route miss

On the PR #1111 worktree `codex/credential-routing-recovery-20260930` at source head
`a64af27dbefdf4d9239ad9e94209cb2416a4dbc4`, the CLI implementation worker reports that two
format/test operations used direct `cargo` rather than the task's reviewed Mise/MBX route. This is
recorded as a command-route miss, not as Rust test or compile evidence. The wrapper did not expose
exact exit codes or UTC timestamps; no network access was used.

The first command was:

```text
cargo fmt --manifest-path crates/jackin/Cargo.toml -- --check
```

It returned a formatting diff in `crates/jackin/src/cli/role/tests.rs:313` for assertion wrapping;
the worker applied that formatting manually. The later combined command
reported no formatter or diff-check diagnostics, but its exit code was not captured:

```text
cargo fmt --manifest-path crates/jackin/Cargo.toml -- --check && git diff --check && git diff --stat -- <four owned paths> && git status --short --branch
```

The test command was:

```text
cargo test --locked --offline --manifest-path crates/jackin/Cargo.toml --lib model_and_effort
```

It stopped during offline dependency resolution with `no matching package named rusqlite found`,
required by the newly registered `jackin-omp-store v0.6.4` workspace package. It did not compile or
run tests. The CLI model/effort tests remain NOT RUN until Cargo.lock is updated and the exact
source is exercised through the reviewed MBX runner.

## PR #1111 exact-source CLI review and CI failure

The source-only CLI review covers commit `24b8d5423a212130e4a1c60beffed860248b7f27`, tree
`63e21941838f728583e22220e9432f6afceb90b9`, parent `4afc5a6eaa5dea404784c6176a32d9bd0c7f80b4`.
The immutable archive is
`/root/.jackin-pr1111-source-24b8-20261005/jackin-24b8d5423a212130e4a1c60beffed860248b7f27.tar`
(SHA-256 `c8454e72854767370cbcb484fe2abec7e8bd7c851d7adf5780d09228df4e500a`, 39,147,520 bytes).
Its tree inventory SHA-256 is `1a6ed9bdb90c4204942ff9aecc5d1518e2b4ab1770d3ed5928cfb4d8e4a1c5a2`
and its exact-source packet SHA-256 is
`22a0a9e3a777fbac13c3fbbec01505ab53bb3ebe18acc3d91f3fb1cfbe28bf2a`. The export handshake
confirmed the pull-request API and `refs/pull/1111/head`; the only changed path at this head is
`crates/jackin/src/app/load_cmd.rs`. Independent source review passed the four-file CLI model/effort
change after its private-interface correction. This is source evidence only; no local Rust test,
compile, or CLI runtime was performed.

GitHub Actions run `37353665241` was a `pull_request` run for this exact head and completed at
`2026-10-05T18:18:25Z` with 28 successful jobs and two failures. The sole independent failure is
`Rust / jackin`: Clippy reports `too_many_lines` at `crates/jackin/src/app/load_cmd.rs:43`, where
`handle_load` is 151/150 lines. The other failure is the dependent `Required` gate. The failed job
log is retained at `/var/tmp/jackin-pr1111-job111910828878.zip`, 93,736 bytes, mode 0600,
SHA-256 `a1ebe1b0be395dcac86c2e73357eccea0eeda5d56af49994f47f11dfc1d2ff4c`; despite its suffix,
the downloaded file is plaintext CI output. A bounded extraction fix is assigned; no lint
suppression is planned.

Final-head feedback remains open: the automated Codex review is `COMMENTED` and says it reviewed
`a64af27dbefdf4d9239ad9e94209cb2416a4dbc4`. Its three inline comments are two known OMP P1s
(rollback-journal snapshots and ambiguous joint WAL salt/checksum corruption) plus a P2 at
`crates/jackin-runtime/src/runtime/universe.rs:596` about a process killed after creating a pending
claim token. The latter can leave an orphan token that causes later exits to report `Missing`; a
bounded recovery fix is assigned. The PR remains open and mergeable; source tests, OMP runtime,
docs specs, and live Usage/broker smoke remain separate gates.

The CI line-count failure was addressed in commit `c7aede8f9eb3296027ab652135501486d2786e00`
(tree `1610cbea8815416ac8d5b957a90cff5606741fdc`, parent 24b8). It extracts a private dry-run
plan helper from `handle_load`, with no lint suppression. The exact commit has not yet completed
source-bound MBX verification. Its new GitHub run `37355227790` is still in progress: DCO and
Actionlint succeeded, while Plan was running at the last query (`2026-10-05T18:20:47Z`). The
24b8 archive above is historical for this new source head; the immutable c7 archive is pending.

## Historical Velnor 86 source-index semantic revalidation

The frozen source manifest
`/root/.velnor-generator-preview-20261005/main-source-86f-manifest.json` (SHA-256
`f12e0de7c5c22c5dc69e4f141d38d704a94f50494448dee3e896ea499a3e61b3`) binds Velnor commit
`86f864aabc2192a9f8ccd3f01f2e25b7858c3f20`, tree `1e16a9270f3ea99b04e68521cfc876fa19db67ac`,
repository `https://github.com/tailrocks/velnor-new.git`, and original Git-index digest
`b60394dee2640d075d013df7f5d3fbc47d6092aaa28f7e6a632e6f718672eb6a`. The source fetch log SHA is
`e64bc5b742a797b1e4e1f07658d1ea32aa8e469e4e4741a9fbbf2c64d9e82226`. The current materialized
source index at `/root/.velnor-generator-preview-20261005/main-source-86f/.git/index` has SHA-256
`50ad81de6e038c3d5bf179442518709faf18f182cca35b4f8bd514e7f161e226`, mode 0644, size 204,634;
it differs bytewise from the digest recorded in the frozen manifest. The owner reports a separate
bounded parser check found a checksum-valid v2 index with 1,599 stage-0 entries plus a TREE
extension and semantic equality of tracked path/mode/object-ID tuples. No cause is inferred, no
byte-identity is claimed, and the original source manifest is unchanged. The parser run's separate
result artifact was not retained, so this semantic check is owner-reported rather than independently
replayed here.

The materialization result JSON SHA-256 is
`e1d0f8638b6beb15a004ebda64c26ec4924246ff4895431de3f59b03f90efe76`; its log SHA-256 is
`fb2095624d47c5c9092a8f471c9d4f9cfb529ec23a9d6031209cc2d9b7f3aa32`. A sanitized minimal Git
capsule has not yet been created; the owner is preparing a separate exact identity record from the
immutable raw-tree manifest. No Cargo cache was inspected and no Cargo/helper/generator execution
occurred in this index review.
