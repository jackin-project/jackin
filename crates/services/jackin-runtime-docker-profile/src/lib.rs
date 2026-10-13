//! jackin-runtime-docker-profile: docker security profiles and flag emission.
//!
//! **Architecture Invariant:** T1.
//! Entry point: [`docker_profile::resolve_profile`] — profile resolution.
//!
//! Resolves the effective Docker security profile from CLI flags, role
//! grants, and host probes (cgroup, `AppArmor`), then emits the `docker run`
//! flags (capabilities, tmpfs, labels, network). Split out of
//! `jackin-runtime` (S7 split 49); the old
//! `jackin_runtime::runtime::docker_profile::*` paths keep working
//! through a re-export shim.

pub mod docker_profile;
