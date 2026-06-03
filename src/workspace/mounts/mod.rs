//! Mount spec parsing — re-exported from `jackin-config`.
pub use jackin_config::{parse_mount_spec, parse_mount_spec_resolved};
pub use jackin_config::mounts::covers;

#[cfg(test)]
mod tests;
