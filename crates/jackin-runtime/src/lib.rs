//! jackin-runtime: container bootstrap pipeline.
//!
//! Future home of `src/runtime/`, `src/instance/`, `src/isolation/`, and
//! `src/derived_image/` — the concrete `DockerApi` / `CommandRunner`
//! implementations, image build, DinD sidecar management, mount materialization,
//! and instance lifecycle.
//!
//! **Phase 1 (current):** Crate scaffold. Code migrates from the binary crate
//! in Phase 3 once `jackin-config` and `jackin-env` are extracted and the
//! `config → runtime::list_role_names` upward edge is severed.
//!
//! **Dependency tier:** `jackin-core` → `jackin-config` → `jackin-env` → `jackin-runtime`
