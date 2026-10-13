//! jackin-runtime-cleanup-eject-resources: prevalidated Docker resource ejection.
//!
//! **Architecture Invariant:** T7.
//! Entry point: [`eject_resources::eject_docker_role_with_resources`] —
//! remove the role and `DinD` containers, certs volume, and network for
//! prevalidated handles, then the host-side socket dir.
//!
//! Split out of `jackin-runtime` (S7 split 106): the terminal
//! destructive step shared by the hub eject flows (direct eject
//! plus the attach reconnect-lease path), decoupled from the
//! handle resolution that stays in the hub. The old
//! `jackin_runtime::runtime::cleanup::eject::eject_docker_role_with_resources`
//! path keeps working through the hub re-export.

pub mod eject_resources;
