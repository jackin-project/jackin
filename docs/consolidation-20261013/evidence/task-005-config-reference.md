# TASK-005 config reference path

Read-only check in `/Users/donbeave/Projects/jackin-project/wt-integration-20261013`. No cargo, commit, or source edit.

Nextest filter `jackin::docs_commands config_reference_matches_public_schema_fields` is the integration test in package `jackin`. `CARGO_MANIFEST_DIR` is `crates/apps/jackin`.

| Item | Value |
| --- | --- |
| File | `crates/apps/jackin/tests/docs_commands.rs` |
| Test | `config_reference_matches_public_schema_fields` (line 694) |
| Path literal | line 701 |
| Open | line 707, `fs::read_to_string(&docs_path)?` |
| Join | `../../docs/content/reference/runtime/configuration.mdx` |
| Missing path | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013/crates/docs/content/reference/runtime/configuration.mdx` |
| Exists | no (`crates/docs` is absent) |
| Closest existing file | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013/docs/content/reference/runtime/configuration.mdx` |

That resolved path is the only `configuration.mdx` open that fails. The four schema reads earlier in the same test exist:

- `crates/core/jackin-config/src/schema.rs`
- `crates/core/jackin-config/src/app_config.rs`
- `crates/core/jackin-config/src/auth.rs`
- `crates/core/jackin-config/src/accounts.rs`

`fs::read_to_string` returns `std::io::Error` without the path, which matches stderr `Error: No such file or directory (os error 2)`.
