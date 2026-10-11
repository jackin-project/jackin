//! jackin-runtime-prewarm-trigger: background image/sidecar prewarm triggers.
//!
//! **Architecture Invariant:** T7.
//! Entry point: [`prewarm_trigger::spawn_background_image_prewarm`] —
//! best-effort background refresh of stale baked role images, off the
//! operator attach path.
//!
//! Background prewarm-target resolution
//! ([`prewarm_trigger::background_prewarm_targets`]), the image sweep
//! ([`prewarm_trigger::spawn_background_image_prewarm`]), and the kept-`DinD`
//! sidecar sweep ([`prewarm_trigger::spawn_background_sidecar_prewarm`]).
//! Split out of `jackin-runtime` (S7 split 94); the old
//! `jackin_runtime::runtime::prewarm_trigger::*` paths keep working
//! through a re-export shim.

pub mod prewarm_trigger;
