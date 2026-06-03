//! jackin-env: operator-env resolution and 1Password CLI integration.
//!
//! Future home of `src/operator_env/` — the four-layer env resolver
//! (`op://` references, `$VAR` expansion, 1Password CLI subprocess), plus
//! the `OpCli`, `OpStructRunner`, `OpWriteRunner` types used by both the
//! console and the launch pipeline.
//!
//! **Phase 1 (current):** Crate scaffold. Types migrate from the binary crate
//! in Phase 2 once `operator_env/mod.rs` is clean enough to lift without
//! circular dependency through the binary.
//!
//! **Dependency tier:** `jackin-core` → `jackin-config` → `jackin-env`
