//! jackin-runtime-launch-dind: `DinD` sidecar launch and retained-sidecar prewarm.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`launch_dind::run_dind_sidecar_headless`] — create the role
//! network and start the TLS `DinD` sidecar; [`launch_dind::adopt_prewarmed_dind_sidecar`] —
//! consume a kept prewarmed sidecar as a one-shot launch resource.
//!
//! Docker network creation ([`launch_dind::create_role_network`]), `DinD`
//! sidecar launch, sidecar prewarm ([`launch_dind::prewarm_dind_sidecar_container_with_paths`]),
//! and the retained-sidecar identity state on disk (`prewarm-dind.json`).
//! Split out of `jackin-runtime` (S7 split 92); the old
//! `jackin_runtime::runtime::launch::launch_dind::*` paths keep working
//! through the hub shim re-export.

pub mod launch_dind;
