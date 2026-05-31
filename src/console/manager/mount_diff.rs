pub(crate) use jackin_console::mount_diff::{MountDiffItem, classify_mount_diffs};

pub(crate) type MountDiff<'a> =
    jackin_console::mount_diff::MountDiff<'a, crate::workspace::MountConfig>;

impl MountDiffItem for crate::workspace::MountConfig {
    fn dst(&self) -> &str {
        &self.dst
    }
}
