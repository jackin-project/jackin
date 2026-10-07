//! jackin-runtime-launch-trust: workspace trust seeding for launches.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`trust::seed_codex_project_trust`] — Codex project trust seeding.
//!
//! Seeds per-workspace trust before launch: Codex project-level
//! `trusted` marks and the `MISE_TRUSTED_CONFIG_PATHS` env. Split
//! out of `jackin-runtime` (S7 split 62); the old
//! `jackin_runtime::runtime::launch::trust::*` paths keep working
//! through a re-export shim.

pub mod trust;
