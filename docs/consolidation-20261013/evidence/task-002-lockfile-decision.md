# Task 002 lockfile decision

Read-only comparison of root `mise.lock` in `/Users/donbeave/Projects/jackin-project/jackin`. Blobs were read with `git show <sha>:mise.lock` and `git show <sha>:mise.toml`. The clone was not edited. No commit. No `cargo`.

| Tip | SHA | `mise.lock` blob | `mise.toml` blob |
| --- | --- | --- | --- |
| M main | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` | `51ff344e2d632574bbfc0987e1e4b6698e6a2b8e` | `3f26ba4dba1050675702d42338fe1e4535f9bd7e` |
| P policy | `fba69d60c2373156dc0dcecee8e085c83776966f` | `90d4e8356371a6eb2cc41d6479bed45369fac413` | `3eca6dca0a51373a54bb805b870b5aaed70e5e1f` |

`docs/mise.lock` on both tips, and `native/mise.lock` on P, contain no `hyperfine`, `minisign`, or `zig` rows. P is the only tip with `native/mise.lock`, and that file pins `swiftlint` only.

## Decision

Integration must restore minisign provenance and the hyperfine pin. P's current lock does not record an equivalent of either.

Do not copy P's `mise.lock` (`90d4e8356371a6eb2cc41d6479bed45369fac413`) as-is. Minisign provenance is missing from every Zig platform row, and the hyperfine tool row is absent.

This comparison does not approve `fba69d60c2373156dc0dcecee8e085c83776966f`, `refs/heads/rust-policy-rollout`, or `refs/heads/ci-tooling/pr1121-ledger-454-20261010`. It does not authorize a source edit.

An install-time minisign check of `zig-aarch64-macos-0.17.0.tar.xz` is not a lock key. P's committed row for that archive has `checksum` and `url` only.

## Minisign provenance

M records `provenance = "minisign"` twice, both under Zig 0.16.0. Every other `provenance` value on M is `"github-attestations"` (45) or `"cosign"` (9).

```976:979:mise.lock
[tools.zig."platforms.linux-x64"]
checksum = "sha256:70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00"
url = "https://ziglang.org/download/0.16.0/zig-x86_64-linux-0.16.0.tar.xz"
provenance = "minisign"
```

```993:996:mise.lock
[tools.zig."platforms.macos-arm64"]
checksum = "sha256:b23d70deaa879b5c2d486ed3316f7eaa53e84acf6fc9cc747de152450d401489"
url = "https://ziglang.org/download/0.16.0/zig-aarch64-macos-0.16.0.tar.xz"
provenance = "minisign"
```

M's other seven Zig platform keys have `checksum` and `url` only: `platforms.linux-arm64`, `platforms.linux-arm64-musl`, `platforms.linux-x64-baseline`, `platforms.linux-x64-musl`, `platforms.linux-x64-musl-baseline`, `platforms.macos-x64`, `platforms.macos-x64-baseline`. The tool header is `[[tools.zig]]` with `version = "0.16.0"` and `backend = "core:zig"` (lines 964–966). `mise.toml` line 45 is `zig = "0.16.0"`.

P's Zig header is a different pin:

```932:935:mise.lock
[[tools.zig]]
version = "0.17.0"
backend = "core:zig"
specifiers = ["0.17.0"]
```

`mise.toml` line 42 is `zig = "0.17.0"`. All nine P platform keys stop at `checksum` and `url`. The string `minisign` occurs zero times in P's `mise.lock`. P's other provenance values are `"github-attestations"` (45) and `"cosign"` (18). None of those keys sit on `tools.zig`.

The archive named in the install log is P's macOS arm64 URL, and that row still has no provenance key:

```961:963:mise.lock
[tools.zig."platforms.macos-arm64"]
checksum = "sha256:b607e9b9234790a008116ae5bdb71c6243b84b9fb42a53a9e70fde41c06c536a"
url = "https://ziglang.org/download/0.17.0/zig-aarch64-macos-0.17.0.tar.xz"
```

The same two-field shape is on `[tools.zig."platforms.linux-x64"]` (lines 945–947), which is the other key M marks with minisign. Checksums and URLs differ from M because the version is 0.17.0, so M's 0.16.0 checksums are not a substitute signature record for this pin.

The selected lock keeps Zig `0.17.0` and must record `provenance = "minisign"` on the Zig platform rows that M records it for, regenerated for the 0.17.0 archives. Those keys are `[tools.zig."platforms.linux-x64"]` and `[tools.zig."platforms.macos-arm64"]`.

## Hyperfine pin

M's tool row:

```441:443:mise.lock
[[tools.hyperfine]]
version = "1.20.0"
backend = "aqua:sharkdp/hyperfine"
```

`mise.toml` line 28 is `hyperfine = "1.20.0"`. The lock then has nine platform keys, lines 445–488: `platforms.linux-arm64`, `platforms.linux-arm64-musl`, `platforms.linux-x64`, `platforms.linux-x64-baseline`, `platforms.linux-x64-musl`, `platforms.linux-x64-musl-baseline`, `platforms.macos-arm64`, `platforms.macos-x64`, `platforms.macos-x64-baseline`. Each has `checksum`, `url`, and `url_api`.

P has no `[[tools.hyperfine]]` header, no `tools.hyperfine.*` key, and no `hyperfine` assignment in `mise.toml`. No other backend name in P's tool list stands in for it.

Two M platform keys name one target and point at another archive. Restoring the pin means the `1.20.0` / `aqua:sharkdp/hyperfine` row must be present. Those two URL rows are a platform mismatch inside M's lock:

```450:452:mise.lock
[tools.hyperfine."platforms.linux-arm64-musl"]
checksum = "sha256:90875cb1db7a1d797c311174d061728361e58fc70e3b62262a00635ac3b1997c"
url = "https://github.com/sharkdp/hyperfine/releases/download/v1.20.0/hyperfine-v1.20.0-aarch64-unknown-linux-gnu.tar.gz"
```

```475:477:mise.lock
[tools.hyperfine."platforms.macos-arm64"]
checksum = "sha256:f58d0b90993fadfa122a351428c469ce24afef3865f027f0e6e86f0830d088f1"
url = "https://github.com/sharkdp/hyperfine/releases/download/v1.20.0/hyperfine-v1.20.0-x86_64-apple-darwin.tar.gz"
```

`platforms.linux-arm64` uses that same GNU checksum and URL (lines 445–447). `platforms.macos-x64` and `platforms.macos-x64-baseline` use the same Darwin checksum and URL (lines 480–488).
