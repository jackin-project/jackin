//! Mount spec parsing — re-exported from `jackin-config`.
pub use jackin_config::mounts::covers;
pub use jackin_config::{parse_mount_spec, parse_mount_spec_resolved};

#[cfg(test)]
mod tests;
