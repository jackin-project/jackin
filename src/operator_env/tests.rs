// Tests moved to `crates/jackin-env/`. The tests that exercised private
// items (OP_STDERR_MAX, truncate_stderr, RawOpField, apply_field_edit,
// op_section_id, resolve_edited_field_ref, with_binary_and_timeout,
// format_launch_diagnostic_for_test, emit_launch_diagnostic) are now
// collocated with the implementation in the jackin-env crate.
//
// Tests that exercised integration paths using AppConfig, WorkspaceConfig,
// etc. are covered by the migration in `crates/jackin-env/src/resolve.rs`
// and `crates/jackin-env/src/token_setup/tests.rs`.
