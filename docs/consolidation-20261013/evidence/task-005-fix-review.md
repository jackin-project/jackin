# TASK-005 fix review

Independent read-only review of the listed commits, each against its parent, in `/Users/donbeave/Projects/jackin-project/wt-integration-20261013`. No cargo run. This file does not approve the branch, a merge, or TASK-005 completion.

Primary clone `/Users/donbeave/Projects/jackin-project/jackin` is `main` at `1c3dd1be4a3b1ed518133ec3899c084c792370ff` (`1c3dd1be`) and clean.

## Verdicts

- `5619ba88` **ACCEPT.** `assert_ne!` replaces `assert!(!….is_empty())` for the file-export and clipboard byte slices and `Vec<u8>` chunks in `crates/core/jackin-protocol/src/attach.rs`, and the commit does not restore schema 6, wire v9, global sccache, ScopedCapability, or an untrimmed model.
- `04cd2b9f` **ACCEPT.** The macOS keychain helpers and their collector call sites are gated with `cfg(any(target_os = "macos", test))`, and the Linux non-test 401 path drops the keychain read before returning the first 401.
- `242a497a` **ACCEPT.** The docs command test now joins `../../../docs/content/reference/runtime/configuration.mdx` from `crates/apps/jackin`, and that file exists at the commit while `crates/docs/content/reference/runtime/configuration.mdx` does not.
- `38a1b6ac` **REJECT.** Hyperfine 1.20.0 is added only in `mise.lock` and is the only locked tool absent from `mise.toml`, so those rows are not a pin the toolchain selects.
- `1431e67a` **ACCEPT.** The changed ICU, rustix 1.1.5, tinystr, writeable, zerotrie, and zerovec exception versions match `Cargo.lock`, and removing `RUSTSEC-2026-0173` matches the advisory's sole package `proc-macro-error2`, which is absent from the lock.
- `77e7b546` **ACCEPT.** `native/mise.toml` runs `xtask desktop xcframework` after bindings-check and before `xcodegen`, and `native/project.yml` links `../target/xcframework/JackinUsage.xcframework`, which that xtask command writes.
- `fc589fda` **ACCEPT.** `require_qualified` fails the rollup when the denominator, fetch proof, unclassified runs, or first-attempt failures (including missing and cancellation) are bad, or when the live workflow, Git, and artifact binding is absent, and `six_nines_claimed` stays false on evidence schema 7.
- `010cb38a` **ACCEPT.** The thirteen restored tests cover the v8 relay shape, migration from store schema 2 to `USAGE_MONITOR_SCHEMA_VERSION` 4 with future schemas rejected, lease reclaim, and later-purge fences, and they do not restore schema 6, wire v9, global sccache, ScopedCapability, or an untrimmed model.
- `2ee01c8c` **ACCEPT.** The restored Claude tests require revoked consent before the request, after a 401, and after a keychain reread to skip another fetch, and a dropped older lease must leave a newer cache generation in place under the existing macOS `cfg` gates.
- `41954fc4` **REJECT.** Rewritten lines still cite `crates/services/jackin-usage/src/token_monitor/pricing.rs` and `crates/services/jackin-usage/src/usage/kimi.rs`, and neither path exists at this commit.
- `3329780c` **REJECT.** Rewritten lines still cite `crates/jackin-usage/src/host/discovery.rs`, `crates/jackin-runtime/src/runtime/launch/account_config.rs`, `crates/jackin-capsule/Cargo.toml`, `crates/jackin-usage/src/usage.rs`, and `crates/services/jackin-usage/src/usage/kimi.rs`, and none of those paths exist at this commit.
